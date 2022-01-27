use std::{
    cell::Cell,
    marker::PhantomData,
    ops::{Add, Mul},
};

use async_trait::async_trait;
use rand::{thread_rng, Rng};

use crate::*;

/// Mock MPC engine that computes result in plain on a single node.
pub struct MockMpcEngine<T: MpcField> {
    _phantom: PhantomData<T>,
    num_openings: Cell<usize>,
    num_rounds: Cell<usize>,
}

impl<T: MpcField> MockMpcEngine<T> {
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

impl<T: MpcField> MpcContext for MockMpcEngine<T> {
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
impl<T: MpcField> MpcEngine for MockMpcEngine<T>
where
    rand::distributions::Standard: rand::prelude::Distribution<Self::Field>,
{
    type Dealer = Self;

    fn dealer(&self) -> &Self::Dealer {
        &self
    }

    async fn process_openings_bundle(&self, requests: Vec<Self::Share>) -> Vec<Self::Field> {
        self.num_openings
            .set(self.num_openings.get() + requests.len());
        self.num_rounds.set(self.num_rounds.get() + 1);
        requests.iter().map(|r| r.0).collect()
    }
}

impl<T: MpcField> MpcDealer for MockMpcEngine<T>
where
    rand::distributions::Standard: rand::prelude::Distribution<Self::Field>,
{
    fn next_beaver_triple(&self) -> (Self::Share, Self::Share, Self::Share) {
        let a: Self::Field = thread_rng().gen();
        let b: Self::Field = thread_rng().gen();
        (PlainShare(a), PlainShare(b), PlainShare(a * b))
    }
}

/// Mock share of a computation run on a single node. Wraps plaintext value.
#[derive(Clone, Copy)]
pub struct PlainShare<T>(pub T);

impl<T: MpcField> MpcShare for PlainShare<T>
where
    T: Copy + Clone + Add<Output = T> + Sub<Output = T> + Neg<Output = T> + Mul<Output = T>,
{
    type Field = T;
}

impl<T: Add<Output = T>> Add for PlainShare<T> {
    type Output = PlainShare<T>;
    fn add(self, rhs: Self) -> Self::Output {
        PlainShare(self.0 + rhs.0)
    }
}

impl<T: Sub<Output = T>> Sub for PlainShare<T> {
    type Output = PlainShare<T>;
    fn sub(self, rhs: Self) -> Self::Output {
        PlainShare(self.0 - rhs.0)
    }
}

impl<T: Add<Output = T>> Add<T> for PlainShare<T> {
    type Output = PlainShare<T>;
    fn add(self, rhs: T) -> Self::Output {
        PlainShare(self.0 + rhs)
    }
}

impl<T: Sub<Output = T>> Sub<T> for PlainShare<T> {
    type Output = PlainShare<T>;
    fn sub(self, rhs: T) -> Self::Output {
        PlainShare(self.0 - rhs)
    }
}

impl<T: Neg<Output = T>> Neg for PlainShare<T> {
    type Output = PlainShare<T>;
    fn neg(self) -> Self::Output {
        PlainShare(-self.0)
    }
}

impl<T: Mul<Output = T>> Mul<T> for PlainShare<T> {
    type Output = PlainShare<T>;
    fn mul(self, rhs: T) -> Self::Output {
        PlainShare(self.0 * rhs)
    }
}
