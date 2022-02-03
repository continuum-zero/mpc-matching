use std::{
    cell::Cell,
    marker::PhantomData,
    ops::{Add, Mul},
};

use async_trait::async_trait;
use rand::thread_rng;

use crate::*;

/// Fake MPC engine that computes result in plain on a single node.
pub struct PlainMpcEngine<T> {
    _phantom: PhantomData<T>,
    num_openings: Cell<usize>,
    num_rounds: Cell<usize>,
}

impl<T> PlainMpcEngine<T> {
    /// Create new instance.
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

impl<T: ff::Field> MpcContext for PlainMpcEngine<T> {
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
impl<T: ff::Field> MpcEngine for PlainMpcEngine<T> {
    type Dealer = Self;
    type Error = ();

    fn dealer(&mut self) -> &mut Self::Dealer {
        self
    }

    async fn process_inputs(
        &mut self,
        inputs: Vec<Self::Field>,
    ) -> Result<Vec<Vec<Self::Share>>, ()> {
        Ok(vec![inputs.iter().map(|&x| PlainShare(x)).collect()])
    }

    async fn process_openings_unchecked(
        &mut self,
        requests: Vec<Self::Share>,
    ) -> Result<Vec<Self::Field>, ()> {
        self.num_openings
            .set(self.num_openings.get() + requests.len());
        self.num_rounds.set(self.num_rounds.get() + 1);
        Ok(requests.iter().map(|r| r.0).collect())
    }

    async fn check_integrity(&mut self) -> Result<(), Self::Error> {
        // Nothing to do here.
        Ok(())
    }
}

impl<T: ff::Field> MpcDealer for PlainMpcEngine<T> {
    fn share_plain(&self, x: Self::Field) -> Self::Share {
        PlainShare(x)
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
