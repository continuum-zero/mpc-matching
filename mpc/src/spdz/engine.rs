use async_trait::async_trait;
use futures::{Sink, Stream};

use crate::{transport::MultipartyTransport, MpcContext, MpcEngine};

use super::{SpdzDealer, SpdzShare};

// TODO: proper error handling

/// SPDZ protocol message
#[derive(Clone)]
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

        self.transport
            .send_to_all(SpdzMessage::Input(own_deltas))
            .await
            .unwrap();

        let mut shares = vec![Vec::new(); self.num_parties()];
        shares[self.party_id()] = own_shares;

        for (other_id, msg) in self.transport.receive_from_all().await.unwrap() {
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
