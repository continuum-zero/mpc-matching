mod fake_dealer;
pub use fake_dealer::FakeSpdzDealer;

mod share;
pub use share::SpdzShare;

use crate::MpcDealer;

/// Dealer of precomputed parameters for SPDZ protocol.
pub trait SpdzDealer: MpcDealer {
    /// Raw sharing of random authentication key.
    fn authentication_key_share(&self) -> Self::Field;
}
