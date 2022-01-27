use std::ops::{Add, Mul, Neg, Sub};

use async_trait::async_trait;
use num_traits::{One, Zero};

pub mod circuits;
pub mod executor;
pub mod plaintext;

/// Marker trait that needs to be implemented on types that represent fields used in MPC computation.
/// TODO: require inverse
pub trait MpcField:
    Copy
    + Clone
    + Send
    + Sync
    + Add<Output = Self>
    + Sub<Output = Self>
    + Neg<Output = Self>
    + Mul<Output = Self>
    + Zero
    + One
{
}

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
    type Field: MpcField;
}

/// Sharing-based MPC computation context.
pub trait MpcContext {
    /// Field type used by this MPC protocol.
    type Field: MpcField;

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

    /// Process bundle of partial open requests.
    async fn process_openings_bundle(&self, requests: Vec<Self::Share>) -> Vec<Self::Field>;
}
