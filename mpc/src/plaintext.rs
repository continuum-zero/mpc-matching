use std::{
    marker::PhantomData,
    ops::{Add, Mul},
};

use async_trait::async_trait;

use crate::*;

/// Mock context of a computation run on a single node.
pub struct PlainMpcEngine<T> {
    _phantom: PhantomData<T>,
}

impl<T> PlainMpcEngine<T> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

/// Mock share of a computation run on a single node. Wraps plaintext value.
#[derive(Clone, Copy)]
pub struct PlainShare<T>(T);

impl<T> MpcContext for PlainMpcEngine<T>
where
    T: Copy + Clone + Add<Output = T> + Mul<Output = T>,
{
    type Field = T;
    type Share = PlainShare<T>;

    fn num_parties(&self) -> usize {
        1
    }

    fn party_id(&self) -> usize {
        0
    }
}

#[async_trait]
impl<T> MpcEngine for PlainMpcEngine<T>
where
    T: Copy + Clone + Add<Output = T> + Mul<Output = T> + Send + Sync,
{
    async fn process_round(&self, requests: MpcRoundInput<Self>) -> MpcRoundOutput<Self> {
        MpcRoundOutput {
            input_responses: requests
                .input_requests
                .iter()
                .map(|r| InputResponse(PlainShare(r.value.unwrap())))
                .collect(),
            mul_responses: requests
                .mul_requests
                .iter()
                .map(|r| MulResponse(PlainShare((r.0).0 * (r.1).0)))
                .collect(),
            open_responses: requests
                .open_requests
                .iter()
                .map(|r| OpenResponse((r.0).0))
                .collect(),
        }
    }
}

impl<T> MpcShare for PlainShare<T>
where
    T: Copy + Clone + Add<Output = T> + Mul<Output = T>,
{
    type Field = T;
}

impl<T: Add<Output = T>> Add for PlainShare<T> {
    type Output = PlainShare<T>;
    fn add(self, rhs: Self) -> Self::Output {
        PlainShare(self.0 + rhs.0)
    }
}

impl<T: Add<Output = T>> Add<T> for PlainShare<T> {
    type Output = PlainShare<T>;
    fn add(self, rhs: T) -> Self::Output {
        PlainShare(self.0 + rhs)
    }
}

impl<T: Mul<Output = T>> Mul<T> for PlainShare<T> {
    type Output = PlainShare<T>;
    fn mul(self, rhs: T) -> Self::Output {
        PlainShare(self.0 * rhs)
    }
}
