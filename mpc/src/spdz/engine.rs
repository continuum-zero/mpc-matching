use async_trait::async_trait;
use futures::{Sink, Stream};

use crate::{MpcContext, MpcEngine};

use super::{SpdzDealer, SpdzShare};

/// SPDZ protocol message
pub enum SpdzMessage<T: ff::Field> {
    Input(Vec<T>),
    PartialOpen(Vec<T>),
}

/// SPDZ protocol implementation.
pub struct SpdzEngine<T, Dealer, Channel>
where
    T: ff::Field,
    Dealer: SpdzDealer<Field = T, Share = SpdzShare<T>>,
    Channel: Stream<Item = SpdzMessage<T>> + Sink<SpdzMessage<T>>,
{
    num_parties: usize,
    party_id: usize,
    dealer: Dealer,
    channels: Vec<Option<Channel>>,
}

impl<T, Dealer, Channel> SpdzEngine<T, Dealer, Channel>
where
    T: ff::Field,
    Dealer: SpdzDealer<Field = T, Share = SpdzShare<T>>,
    Channel: Stream<Item = SpdzMessage<T>> + Sink<SpdzMessage<T>>,
{
    pub fn new(dealer: Dealer, channels: Vec<Option<Channel>>, party_id: usize) -> Self {
        for (j, channel) in channels.iter().enumerate() {
            if j != party_id && channel.is_none() {
                panic!("Channel missing for party {}", j);
            }
        }
        Self {
            num_parties: channels.len(),
            party_id,
            dealer,
            channels,
        }
    }
}

impl<T, Dealer, Channel> MpcContext for SpdzEngine<T, Dealer, Channel>
where
    T: ff::Field,
    Dealer: SpdzDealer<Field = T, Share = SpdzShare<T>>,
    Channel: Stream<Item = SpdzMessage<T>> + Sink<SpdzMessage<T>>,
{
    type Field = T;
    type Share = SpdzShare<T>;

    fn num_parties(&self) -> usize {
        self.num_parties
    }

    fn party_id(&self) -> usize {
        self.party_id
    }
}

#[async_trait(?Send)]
impl<T, Dealer, Channel> MpcEngine for SpdzEngine<T, Dealer, Channel>
where
    T: ff::Field,
    Dealer: SpdzDealer<Field = T, Share = SpdzShare<T>>,
    Channel: Stream<Item = SpdzMessage<T>> + Sink<SpdzMessage<T>>,
{
    type Dealer = Dealer;

    fn dealer(&mut self) -> &mut Self::Dealer {
        &mut self.dealer
    }

    async fn process_inputs(&mut self, inputs: Vec<Self::Field>) -> Vec<Vec<Self::Share>> {
        todo!()
    }

    async fn process_openings_unchecked(&mut self, requests: Vec<Self::Share>) -> Vec<Self::Field> {
        todo!()
    }

    async fn process_outputs(&mut self, requests: Vec<Self::Share>) -> Vec<Self::Field> {
        todo!()
    }
}
