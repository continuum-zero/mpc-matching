pub mod circuits;
pub mod executor;
pub mod fields;
pub mod plaintext;
pub mod spdz;
pub mod transport;

use std::ops::{Add, Mul, Neg, Sub};

use async_trait::async_trait;

/// Private share of a field element.
/// Sharing is addtive and supports multiplication by scalars without communication.
pub trait MpcShare:
    Copy
    + Clone
    + Send
    + Sync
    + Add<Output = Self>
    + Sub<Output = Self>
    + Neg<Output = Self>
    + Mul<Self::Field, Output = Self>
{
    /// Field type of value represented by this share.
    type Field: ff::Field;

    /// Sharing of zero.
    fn zero() -> Self;

    /// Multiply share by two.
    fn double(&self) -> Self;
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
    /// Sharing of plaintext element.
    fn share_plain(&self, x: Self::Field) -> Self::Share;

    /// Random sharing of a secret random triple (a, b, c) that satisfies ab = c.
    fn next_beaver_triple(&mut self) -> (Self::Share, Self::Share, Self::Share);

    /// Random sharing of a secret random bit.
    fn next_bit(&mut self) -> Self::Share;
}

/// Low-level interface of sharing-based MPC protocol.
#[async_trait(?Send)]
pub trait MpcEngine: MpcContext {
    type Dealer: MpcDealer<Field = Self::Field, Share = Self::Share>;
    type Error;

    /// Get dealer associated with this computation.
    fn dealer(&mut self) -> &mut Self::Dealer;

    /// Process inputs. Each party provides a vector of its own inputs.
    /// Returns vector of input shares for each party.
    async fn process_inputs(
        &mut self,
        inputs: Vec<Self::Field>,
    ) -> Result<Vec<Vec<Self::Share>>, Self::Error>;

    /// Process bundle of partial open requests.
    /// Warning: Integrity checks may be deferred.
    async fn process_openings_unchecked(
        &mut self,
        requests: Vec<Self::Share>,
    ) -> Result<Vec<Self::Field>, Self::Error>;

    /// Check integrity of everything computed so far.
    async fn check_integrity(&mut self) -> Result<(), Self::Error>;
}
