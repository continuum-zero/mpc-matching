use std::{
    future,
    marker::PhantomData,
    ops::{Add, Mul},
};

use crate::{MpcContext, MpcShare};

/// Mock context of a computation run on a single node.
pub struct PlainMpcContext<T> {
    _phantom: PhantomData<T>,
}

impl<T> PlainMpcContext<T> {
    /// Create new mock context.
    pub fn new() -> PlainMpcContext<T> {
        PlainMpcContext {
            _phantom: PhantomData,
        }
    }
}

/// Mock share of a computation run on a single node. Wraps plaintext value.
#[derive(Clone, Copy)]
pub struct PlainShare<T>(T);

impl<T> MpcContext<T> for PlainMpcContext<T>
where
    T: Copy + Clone + Add<Output = T> + Mul<T, Output = T>,
{
    type Share = PlainShare<T>;
    type MulOutput = future::Ready<Self::Share>;
    type OpenOutput = future::Ready<T>;

    fn num_parties(&self) -> usize {
        1
    }

    fn party_id(&self) -> usize {
        0
    }

    fn input(&self, id: usize, value: Option<T>) -> Self::Share {
        if id != 0 {
            panic!("Invalid party ID");
        }
        match value {
            Some(value) => PlainShare(value),
            None => panic!("Missing input value"),
        }
    }

    fn mul(&self, a: Self::Share, b: Self::Share) -> Self::MulOutput {
        future::ready(PlainShare(a.0 * b.0))
    }

    fn open(&self, a: Self::Share) -> Self::OpenOutput {
        future::ready(a.0)
    }
}

impl<T> MpcShare<T> for PlainShare<T> where T: Copy + Clone + Add<Output = T> + Mul<Output = T> {}

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
