use std::ops::{Add, AddAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

use crate::{MpcField, MpcShare};

/// Private share of a field element in SPDZ protocol.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct SpdzShare<T> {
    pub value: T,
    pub mac: T,
}

impl<T: MpcField> SpdzShare<T> {
    /// Make SPDZ share for given plain value, authorization key share and party ID.
    pub fn from_plain(value: T, auth_key: T, party_id: usize) -> Self {
        Self {
            value: if party_id == 0 { value } else { T::zero() },
            mac: value * auth_key,
        }
    }
}

impl<T: MpcField> MpcShare for SpdzShare<T> {
    type Field = T;

    fn zero() -> Self {
        SpdzShare {
            value: T::zero(),
            mac: T::zero(),
        }
    }

    fn double(&self) -> Self {
        SpdzShare {
            value: self.value.double(),
            mac: self.mac.double(),
        }
    }
}

impl<T: MpcField> Default for SpdzShare<T> {
    fn default() -> Self {
        Self::zero()
    }
}

impl<T: MpcField> Add for SpdzShare<T> {
    type Output = SpdzShare<T>;
    fn add(self, rhs: Self) -> Self::Output {
        SpdzShare {
            value: self.value + rhs.value,
            mac: self.mac + rhs.mac,
        }
    }
}

impl<T: MpcField> Sub for SpdzShare<T> {
    type Output = SpdzShare<T>;
    fn sub(self, rhs: Self) -> Self::Output {
        SpdzShare {
            value: self.value - rhs.value,
            mac: self.mac - rhs.mac,
        }
    }
}

impl<T: MpcField> Neg for SpdzShare<T> {
    type Output = SpdzShare<T>;
    fn neg(self) -> Self::Output {
        SpdzShare {
            value: -self.value,
            mac: -self.mac,
        }
    }
}

impl<T: MpcField> Mul<T> for SpdzShare<T> {
    type Output = SpdzShare<T>;
    fn mul(self, rhs: T) -> Self::Output {
        SpdzShare {
            value: self.value * rhs,
            mac: self.mac * rhs,
        }
    }
}

impl<T: MpcField> AddAssign for SpdzShare<T> {
    fn add_assign(&mut self, rhs: Self) {
        self.value += rhs.value;
        self.mac += rhs.mac;
    }
}

impl<T: MpcField> SubAssign for SpdzShare<T> {
    fn sub_assign(&mut self, rhs: Self) {
        self.value -= rhs.value;
        self.mac -= rhs.mac;
    }
}

impl<T: MpcField> MulAssign<T> for SpdzShare<T> {
    fn mul_assign(&mut self, rhs: T) {
        self.value *= rhs;
        self.mac *= rhs;
    }
}
