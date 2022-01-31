use async_trait::async_trait;
use futures::{Sink, Stream};
use serde::{Deserialize, Serialize};

use crate::{transport::MultipartyTransport, MpcContext, MpcEngine};

use super::{SpdzDealer, SpdzShare};

// TODO: proper error handling

/// SPDZ protocol message
#[derive(Clone, Serialize, Deserialize)]
pub enum SpdzMessage<T> {
    Input(Vec<T>),
    PartialOpenShares(Vec<T>),
    PartialOpenSum(Vec<T>),
}

/// SPDZ protocol implementation.
pub struct SpdzEngine<T, Dealer, Channel> {
    dealer: Dealer,
    transport: MultipartyTransport<SpdzMessage<T>, Channel>,
}

impl<T, Dealer, Channel> SpdzEngine<T, Dealer, Channel> {
    /// Create SPDZ protocol engine.
    pub fn new(dealer: Dealer, transport: MultipartyTransport<SpdzMessage<T>, Channel>) -> Self {
        Self { dealer, transport }
    }
}

impl<T, Dealer, Channel> MpcContext for SpdzEngine<T, Dealer, Channel>
where
    T: ff::Field,
    Dealer: SpdzDealer<Field = T, Share = SpdzShare<T>>,
{
    type Field = T;
    type Share = SpdzShare<T>;

    fn num_parties(&self) -> usize {
        self.transport.num_parties()
    }

    fn party_id(&self) -> usize {
        self.transport.party_id()
    }
}

#[async_trait(?Send)]
impl<T, E, Dealer, Channel> MpcEngine for SpdzEngine<T, Dealer, Channel>
where
    T: ff::Field,
    Dealer: SpdzDealer<Field = T, Share = SpdzShare<T>>,
    Channel: Stream<Item = Result<SpdzMessage<T>, E>> + Sink<SpdzMessage<T>> + Unpin,
{
    type Dealer = Dealer;

    fn dealer(&mut self) -> &mut Self::Dealer {
        &mut self.dealer
    }

    async fn process_inputs(&mut self, inputs: Vec<Self::Field>) -> Vec<Vec<Self::Share>> {
        let (own_shares, own_deltas): (Vec<_>, Vec<_>) = inputs
            .into_iter()
            .map(|x| {
                let (share, plain) = self.dealer.next_input_mask_own();
                let delta = x - plain;
                (share + self.dealer.share_plain(delta), delta)
            })
            .unzip();

        let received_messages = self
            .transport
            .exchange_with_all(SpdzMessage::Input(own_deltas))
            .await
            .unwrap();

        let mut shares = vec![Vec::new(); self.num_parties()];
        shares[self.party_id()] = own_shares;

        for (other_id, msg) in received_messages {
            if let SpdzMessage::Input(deltas) = msg {
                shares[other_id] = deltas
                    .into_iter()
                    .map(|delta| {
                        let share = self.dealer.next_input_mask_for(other_id);
                        share + self.dealer.share_plain(delta)
                    })
                    .collect();
            } else {
                panic!("Unexpected message");
            }
        }

        shares
    }

    async fn process_openings_unchecked(&mut self, requests: Vec<Self::Share>) -> Vec<Self::Field> {
        let mut values: Vec<_> = requests.into_iter().map(|x| x.value).collect();

        if self.party_id() == 0 {
            for (_, msg) in self.transport.receive_from_all().await.unwrap() {
                if let SpdzMessage::PartialOpenShares(parts) = msg {
                    if parts.len() != values.len() {
                        panic!("Number of shares mismatched");
                    }
                    for (i, part) in parts.into_iter().enumerate() {
                        values[i] += part;
                    }
                } else {
                    panic!("Unexpected message");
                }
            }
            self.transport
                .send_to_all(SpdzMessage::PartialOpenSum(values.clone()))
                .await
                .unwrap();
            values
        } else {
            self.transport
                .send_to(0, SpdzMessage::PartialOpenShares(values))
                .await
                .unwrap();
            let msg = self.transport.receive_from(0).await.unwrap();
            if let SpdzMessage::PartialOpenSum(values) = msg {
                values
            } else {
                panic!("Unexpected message");
            }
        }
    }

    async fn process_outputs(&mut self, requests: Vec<Self::Share>) -> Vec<Self::Field> {
        self.process_openings_unchecked(requests).await
    }
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;

    use futures::{stream::FuturesUnordered, Future, StreamExt};
    use serde::{Deserialize, Serialize};

    use crate::{
        circuits::{self, join_circuits_all},
        executor::{self, MpcExecutionContext},
        spdz::{FakeSpdzDealer, SpdzShare},
        transport::{self, BincodeDuplex},
    };

    use super::{SpdzEngine, SpdzMessage};

    #[derive(ff::PrimeField, Serialize, Deserialize)]
    #[PrimeFieldModulus = "4611686018427387903"]
    #[PrimeFieldGenerator = "7"]
    #[PrimeFieldReprEndianness = "little"]
    struct Fp([u64; 1]);

    type MockSpdzEngine = SpdzEngine<Fp, FakeSpdzDealer<Fp>, BincodeDuplex<SpdzMessage<Fp>>>;

    async fn run_spdz<F>(inputs: Vec<Vec<Fp>>, circuit_fn: F) -> Vec<Fp>
    where
        F: Copy
            + Fn(
                &'_ MpcExecutionContext<MockSpdzEngine>,
                Vec<Vec<SpdzShare<Fp>>>,
            ) -> Pin<Box<dyn Future<Output = Vec<SpdzShare<Fp>>> + '_>>,
    {
        let num_parties = inputs.len();
        let channel_matrix = transport::mock_multiparty_channels(num_parties, 512);
        let futures = FuturesUnordered::new();

        for (party_id, transport) in channel_matrix.into_iter().enumerate() {
            let dealer = FakeSpdzDealer::new(num_parties, party_id, 123);
            let engine = MockSpdzEngine::new(dealer, transport);
            futures.push(executor::run_circuit(engine, &inputs[party_id], circuit_fn));
        }

        let outputs: Vec<_> = futures.collect().await;
        for i in 1..num_parties {
            assert_eq!(outputs[i], outputs[0], "Mismatched outputs",);
        }
        outputs.into_iter().next().unwrap()
    }

    #[tokio::test]
    async fn test_spdz() {
        let outputs = run_spdz(
            vec![
                vec![1.into(), 2.into(), 3.into()],
                vec![4.into(), 5.into(), 6.into()],
                vec![7.into(), 8.into(), 9.into()],
            ],
            |ctx, inputs| {
                Box::pin(async move {
                    let num_elems = inputs[0].len();
                    join_circuits_all((0..num_elems).map(|i| {
                        circuits::elementary::product(ctx, inputs.iter().map(move |x| x[i]))
                    }))
                    .await
                })
            },
        )
        .await;
        assert_eq!(outputs, vec![28.into(), 80.into(), 162.into()]);
    }
}
