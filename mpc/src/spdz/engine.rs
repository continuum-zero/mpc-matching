use async_trait::async_trait;
use futures::{Sink, Stream};
use serde::{Deserialize, Serialize};

use crate::{
    transport::{ChannelError, MultipartyTransport},
    MpcContext, MpcEngine,
};

use super::{SpdzDealer, SpdzShare};

/// SPDZ protocol message.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum SpdzMessage<T> {
    Input(Vec<T>),
    PartialOpenShares(Vec<T>),
    PartialOpenSum(Vec<T>),
}

/// SPDZ error.
#[derive(Copy, Clone, Debug)]
pub enum SpdzError {
    Send,
    Recv,
    Protocol(&'static str),
}

impl From<ChannelError> for SpdzError {
    fn from(err: ChannelError) -> Self {
        match err {
            ChannelError::Send => SpdzError::Send,
            ChannelError::Recv => SpdzError::Recv,
        }
    }
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
    type Error = SpdzError;

    fn dealer(&mut self) -> &mut Self::Dealer {
        &mut self.dealer
    }

    async fn process_inputs(
        &mut self,
        inputs: Vec<Self::Field>,
    ) -> Result<Vec<Vec<Self::Share>>, SpdzError> {
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
            .await?;

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
                return Err(SpdzError::Protocol("Unexpected message"));
            }
        }

        Ok(shares)
    }

    async fn process_openings_unchecked(
        &mut self,
        requests: Vec<Self::Share>,
    ) -> Result<Vec<Self::Field>, SpdzError> {
        let mut values: Vec<_> = requests.into_iter().map(|x| x.value).collect();
        let values_count = values.len();

        if self.party_id() == 0 {
            for (_, msg) in self.transport.receive_from_all().await? {
                if let SpdzMessage::PartialOpenShares(parts) = msg {
                    if parts.len() != values_count {
                        return Err(SpdzError::Protocol("Received incorrect number of shares"));
                    }
                    for (i, part) in parts.into_iter().enumerate() {
                        values[i] += part;
                    }
                } else {
                    return Err(SpdzError::Protocol("Unexpected message"));
                }
            }
            self.transport
                .send_to_all(SpdzMessage::PartialOpenSum(values.clone()))
                .await?;
        } else {
            self.transport
                .send_to(0, SpdzMessage::PartialOpenShares(values))
                .await?;
            if let SpdzMessage::PartialOpenSum(sum) = self.transport.receive_from(0).await? {
                if sum.len() != values_count {
                    return Err(SpdzError::Protocol("Received incorrect number of shares"));
                }
                values = sum;
            } else {
                return Err(SpdzError::Protocol("Unexpected message"));
            }
        }

        Ok(values)
    }

    async fn process_outputs(
        &mut self,
        requests: Vec<Self::Share>,
    ) -> Result<Vec<Self::Field>, SpdzError> {
        self.process_openings_unchecked(requests).await
    }
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;

    use futures::{stream::FuturesUnordered, Future, StreamExt};

    use crate::{
        circuits::{self, join_circuits_all},
        executor::{self, MpcExecutionContext},
        spdz::{FakeSpdzDealer, SpdzShare},
        transport::{self, BincodeDuplex},
    };

    use super::{SpdzEngine, SpdzMessage};

    type Fp = crate::fields::Mersenne127;
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

        let outputs: Vec<_> = futures.map(|result| result.unwrap()).collect().await;
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
