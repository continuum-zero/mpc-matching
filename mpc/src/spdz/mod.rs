mod engine;
pub use engine::{SpdzEngine, SpdzError, SpdzMessage};

mod fake_dealer;
pub use fake_dealer::FakeSpdzDealer;

mod share;
pub use share::SpdzShare;

use crate::MpcDealer;

/// Dealer of precomputed parameters for SPDZ protocol.
pub trait SpdzDealer: MpcDealer {
    /// Raw sharing of the authentication key.
    fn authentication_key_share(&self) -> Self::Field;

    /// Random sharing of a random value with plaintext known to this party.
    fn next_input_mask_own(&mut self) -> (Self::Share, Self::Field);

    /// Random sharing of a random value with plaintext known to a specified party.
    fn next_input_mask_for(&mut self, id: usize) -> Self::Share;
}
