mod engine;
pub use engine::{SpdzEngine, SpdzError, SpdzMessage};

mod fake_dealer;
pub use fake_dealer::FakeSpdzDealer;

mod share;
pub use share::SpdzShare;

use crate::MpcDealer;

/// Hashing function used by SPDZ implementation.
pub type SpdzDigest = sha3::Sha3_256;

/// Output of hashing function used by SPDZ implementation.
pub type SpdzDigestOutput = [u8; 32];

/// Random number generator used by SPDZ implementation.
pub type SpdzRng = rand::rngs::StdRng;

/// Dealer of precomputed parameters for SPDZ protocol.
pub trait SpdzDealer: MpcDealer {
    /// Raw sharing of random authentication key.
    fn authentication_key_share(&self) -> Self::Field;

    /// Random sharing of a random value with plaintext known to current party.
    fn next_input_mask_own(&mut self) -> (Self::Share, Self::Field);

    /// Random sharing of a random value with plaintext known to specified party.
    fn next_input_mask_for(&mut self, id: usize) -> Self::Share;
}
