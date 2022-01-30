use std::{
    cell::Cell,
    marker::PhantomData,
    ops::{Add, Mul},
};

use async_trait::async_trait;
use rand::thread_rng;

use crate::*;

/// Mock MPC engine that computes result in plain on a single node.
pub struct MockMpcEngine<T: ff::Field> {
    _phantom: PhantomData<T>,
    num_openings: Cell<usize>,
    num_rounds: Cell<usize>,
}

impl<T: ff::Field> MockMpcEngine<T> {
    /// Create a new instance of mock.
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
            num_openings: Cell::new(0),
            num_rounds: Cell::new(0),
        }
    }

    /// Get total count of open requests.
    pub fn num_openings(&self) -> usize {
        self.num_openings.get()
    }

    /// Get total number of rounds.
    pub fn num_rounds(&self) -> usize {
        self.num_rounds.get()
    }
}

impl<T: ff::Field> MpcContext for MockMpcEngine<T> {
    type Field = T;
    type Share = PlainShare<T>;

    fn num_parties(&self) -> usize {
        1
    }

    fn party_id(&self) -> usize {
        0
    }
}

#[async_trait(?Send)]
impl<T: ff::Field> MpcEngine for MockMpcEngine<T> {
    type Dealer = Self;

    fn dealer(&mut self) -> &mut Self::Dealer {
        self
    }

    async fn process_inputs(&mut self, inputs: Vec<Self::Field>) -> Vec<Vec<Self::Share>> {
        vec![inputs.iter().map(|&x| PlainShare(x)).collect()]
    }

    async fn process_openings_unchecked(&mut self, requests: Vec<Self::Share>) -> Vec<Self::Field> {
        self.num_openings
            .set(self.num_openings.get() + requests.len());
        self.num_rounds.set(self.num_rounds.get() + 1);
        requests.iter().map(|r| r.0).collect()
    }

    async fn process_outputs(&mut self, requests: Vec<Self::Share>) -> Vec<Self::Field> {
        self.process_openings_unchecked(requests).await
    }
}

impl<T: ff::Field> MpcDealer for MockMpcEngine<T> {
    fn zero(&self) -> Self::Share {
        PlainShare(T::zero())
    }

    fn next_beaver_triple(&mut self) -> (Self::Share, Self::Share, Self::Share) {
        let mut rng = thread_rng();
        let a = Self::Field::random(&mut rng);
        let b = Self::Field::random(&mut rng);
        (PlainShare(a), PlainShare(b), PlainShare(a * b))
    }
}

/// Mock share of a computation run on a single node. Wraps plaintext value.
#[derive(Clone, Copy)]
pub struct PlainShare<T>(pub T);

impl<T: ff::Field> MpcShare for PlainShare<T> {
    type Field = T;
}

impl<T: ff::Field> Add for PlainShare<T> {
    type Output = PlainShare<T>;
    fn add(self, rhs: Self) -> Self::Output {
        PlainShare(self.0 + rhs.0)
    }
}

impl<T: ff::Field> Sub for PlainShare<T> {
    type Output = PlainShare<T>;
    fn sub(self, rhs: Self) -> Self::Output {
        PlainShare(self.0 - rhs.0)
    }
}

impl<T: ff::Field> Add<T> for PlainShare<T> {
    type Output = PlainShare<T>;
    fn add(self, rhs: T) -> Self::Output {
        PlainShare(self.0 + rhs)
    }
}

impl<T: ff::Field> Sub<T> for PlainShare<T> {
    type Output = PlainShare<T>;
    fn sub(self, rhs: T) -> Self::Output {
        PlainShare(self.0 - rhs)
    }
}

impl<T: ff::Field> Neg for PlainShare<T> {
    type Output = PlainShare<T>;
    fn neg(self) -> Self::Output {
        PlainShare(-self.0)
    }
}

impl<T: ff::Field> Mul<T> for PlainShare<T> {
    type Output = PlainShare<T>;
    fn mul(self, rhs: T) -> Self::Output {
        PlainShare(self.0 * rhs)
    }
}
