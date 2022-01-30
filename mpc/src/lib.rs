use std::ops::{Add, Mul, Neg, Sub};

use async_trait::async_trait;

pub mod circuits;
pub mod executor;
pub mod plaintext;

/// Private share of a field element.
/// Sharing is linear and supports addition with plaintext field elements without communication.
pub trait MpcShare:
    Copy
    + Clone
    + Send
    + Sync
    + Add<Output = Self>
    + Sub<Output = Self>
    + Neg<Output = Self>
    + Add<Self::Field, Output = Self>
    + Sub<Self::Field, Output = Self>
    + Mul<Self::Field, Output = Self>
{
    /// Field type of value represented by this share.
    type Field: ff::Field;

    /// Create share from public plaintext value.
    fn from_plain<C>(ctx: &C, value: Self::Field) -> Self
    where
        C: MpcContext<Field = Self::Field, Share = Self>;
}

/// Sharing-based MPC computation context.
pub trait MpcContext {
    /// Field type used by this MPC protocol.
    type Field: ff::Field;

    /// Share type used by this MPC protocol.
    type Share: MpcShare<Field = Self::Field>;

    /// Number of parties participating in MPC computation.
    fn num_parties(&self) -> usize;

    /// ID of current party.
    fn party_id(&self) -> usize;
}

/// Dealer of precomputed parameters for MPC computation.
pub trait MpcDealer: MpcContext {
    /// Random sharing of a secret random triple (a, b, c) that satisfies ab = c.
    fn next_beaver_triple(&self) -> (Self::Share, Self::Share, Self::Share);
}

/// Low-level interface of sharing-based MPC protocol.
#[async_trait(?Send)]
pub trait MpcEngine: MpcContext {
    type Dealer: MpcDealer<Field = Self::Field, Share = Self::Share>;

    /// Get dealer associated with this computation.
    fn dealer(&self) -> &Self::Dealer;

    /// Process inputs. Each party provides a vector of its own inputs.
    /// Returns vector of input shares for each party.
    async fn process_inputs(&self, inputs: Vec<Self::Field>) -> Vec<Vec<Self::Share>>;

    /// Process bundle of partial open requests.
    /// Warning: Integrity checks may be deferred to output phase (like in SPDZ protocol).
    async fn process_openings_unchecked(&self, requests: Vec<Self::Share>) -> Vec<Self::Field>;

    /// Process outputs. Performs integrity checks.
    async fn process_outputs(&self, outputs: Vec<Self::Share>) -> Vec<Self::Field>;
}
