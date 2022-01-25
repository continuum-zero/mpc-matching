use std::{
    future::Future,
    ops::{Add, Mul},
};

pub mod circuits;
pub mod plaintext;

/// Context of an MPC computation over field `T`.
/// This interface handles operations that require communication between nodes.
pub trait MpcContext<T> {
    /// Concrete Share type that is used by this MPC computation.
    type Share: MpcShare<T>;

    /// Future type returned from `mul` (async workaround).
    type MulOutput: Future<Output = Self::Share>;

    /// Future type returned from `open` (async workaround).
    type OpenOutput: Future<Output = T>;

    /// Number of parties participating in MPC computation.
    fn num_parties(&self) -> usize;

    /// ID of current party.
    fn party_id(&self) -> usize;

    /// Get sharing of next input value of party `id`.
    /// If `party_id() != id()`, then value must be None.
    /// If `party_id() == id()`, then plain input value must be provided.
    /// Doesn't require communication.
    fn input(&self, id: usize, value: Option<T>) -> Self::Share;

    /// Multiply shared values. Requires communication.
    fn mul(&self, a: Self::Share, b: Self::Share) -> Self::MulOutput;

    /// Open provided share. Requires communication.
    fn open(&self, a: Self::Share) -> Self::OpenOutput;
}

/// Share of value from field `T`. Sharing is linear and supports addition of scalars.
pub trait MpcShare<T>:
    Copy + Clone + Add<Output = Self> + Add<T, Output = Self> + Mul<T, Output = Self>
{
}
