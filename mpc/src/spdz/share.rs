use std::ops::{Add, Mul, Neg, Sub};

use crate::MpcShare;

/// Value share in SPDZ protocol.
#[derive(Copy, Clone)]
pub struct SpdzShare<T: ff::Field> {
    pub(super) value: T,
    pub(super) mac: T,
}

impl<T: ff::Field> MpcShare for SpdzShare<T> {
    type Field = T;
}

impl<T: ff::Field> Add for SpdzShare<T> {
    type Output = SpdzShare<T>;
    fn add(self, rhs: Self) -> Self::Output {
        SpdzShare {
            value: self.value + rhs.value,
            mac: self.mac + rhs.mac,
        }
    }
}

impl<T: ff::Field> Sub for SpdzShare<T> {
    type Output = SpdzShare<T>;
    fn sub(self, rhs: Self) -> Self::Output {
        SpdzShare {
            value: self.value - rhs.value,
            mac: self.mac - rhs.mac,
        }
    }
}

impl<T: ff::Field> Neg for SpdzShare<T> {
    type Output = SpdzShare<T>;
    fn neg(self) -> Self::Output {
        SpdzShare {
            value: -self.value,
            mac: -self.mac,
        }
    }
}

impl<T: ff::Field> Mul<T> for SpdzShare<T> {
    type Output = SpdzShare<T>;
    fn mul(self, rhs: T) -> Self::Output {
        SpdzShare {
            value: self.value * rhs,
            mac: self.mac * rhs,
        }
    }
}
