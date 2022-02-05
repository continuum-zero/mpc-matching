use std::ops::{Add, Mul, Neg, Sub};

use crate::{fields::MpcField, MpcShare};

/// Value share in SPDZ protocol.
#[derive(Copy, Clone)]
pub struct SpdzShare<T> {
    pub(super) value: T,
    pub(super) mac: T,
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
