pub mod circuits;
pub mod executor;
pub mod fields;
pub mod plaintext;
pub mod spdz;
pub mod transport;

use std::ops::{Add, AddAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use async_trait::async_trait;

/// Prime field that can be used in MPC computation.
pub trait MpcField: ff::PrimeField {
    /// Largest k such that 2^(k+1)-2 doesn't overflow.
    const SAFE_BITS: usize;

    /// Returns preprocessed integer 2^k embedded in field. Panics if k > SAFE_BITS.
    fn power_of_two(k: usize) -> Self;

    /// Returns preprocessed inverse of 2^k. Panics if k > SAFE_BITS.
    fn power_of_two_inverse(k: usize) -> Self;

    /// Convert to u64 by truncating remaining bits.
    fn truncated(&self) -> u64;
}

/// Private share of a field element.
/// Sharing is addtive and supports multiplication by scalars without communication.
pub trait MpcShare:
    Copy
    + Clone
    + Add<Output = Self>
    + Sub<Output = Self>
    + Neg<Output = Self>
    + Mul<Self::Field, Output = Self>
    + AddAssign
    + SubAssign
    + MulAssign<Self::Field>
{
    /// Field type of value represented by this share.
    type Field: MpcField;

    /// Sharing of zero.
    fn zero() -> Self;

    /// Multiply share by two.
    fn double(&self) -> Self;
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
    /// Sharing of plaintext element.
    fn share_plain(&self, x: Self::Field) -> Self::Share;

    /// Random sharing of a secret random triple (a, b, c) that satisfies ab = c.
    fn next_beaver_triple(&mut self) -> (Self::Share, Self::Share, Self::Share);

    /// Random sharing of a secret N-bit unsigned integer.
    fn next_uint(&mut self, bits: usize) -> Self::Share;

    /// Returns true if dealer cannot produce more parameters of some type.
    /// Once this happens, other all methods return invalid random values.
    fn is_exhausted(&self) -> bool;
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
