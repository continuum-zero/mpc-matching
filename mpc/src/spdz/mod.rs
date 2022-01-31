// mod engine;
// pub use engine::{SpdzEngine, SpdzMessage};

mod fake_dealer;
pub use fake_dealer::FakeSpdzDealer;

mod share;
pub use share::SpdzShare;

use crate::MpcDealer;

/// Dealer of precomputed parameters for SPDZ protocol.
pub trait SpdzDealer: MpcDealer {
    /// Raw sharing of random authentication key.
    fn authentication_key_share(&self) -> Self::Field;

    /// Random sharing of a random value with plaintext known to current party.
    fn next_input_mask_own(&mut self) -> (Self::Share, Self::Field);

    /// Random sharing of a random value with plaintext known to specified party.
    fn next_input_mask_for(&mut self, id: usize) -> Self::Share;
}
