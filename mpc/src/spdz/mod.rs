mod share;
pub use share::SpdzShare;

use crate::MpcDealer;

/// Dealer of precomputed parameters for SPDZ protocol.
pub trait SpdzDealer: MpcDealer {
    /// Sharing of random authentication key.
    fn authentication_key(&self) -> Self::Share;
}
