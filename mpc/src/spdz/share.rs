use std::ops::{Add, Mul, Neg, Sub};

use crate::MpcShare;

/// Value share in SPDZ protocol.
#[derive(Copy, Clone)]
pub struct SpdzShare<T: ff::Field> {
    value: T,
    mac: T,
    auth: T,
    is_first: bool,
}

impl<T: ff::Field> MpcShare for SpdzShare<T> {
    type Field = T;
}

impl<T: ff::Field> Add for SpdzShare<T> {
    type Output = SpdzShare<T>;
    fn add(self, rhs: Self) -> Self::Output {
        debug_assert!(
            self.is_first == rhs.is_first && self.auth == rhs.auth,
            "Mismatched shares from different parties"
        );
        SpdzShare {
            value: self.value + rhs.value,
            mac: self.mac + rhs.mac,
            auth: self.auth,
            is_first: self.is_first,
        }
    }
}

impl<T: ff::Field> Sub for SpdzShare<T> {
    type Output = SpdzShare<T>;
    fn sub(self, rhs: Self) -> Self::Output {
        debug_assert!(
            self.is_first == rhs.is_first && self.auth == rhs.auth,
            "Mismatched shares from different parties"
        );
        SpdzShare {
            value: self.value - rhs.value,
            mac: self.mac - rhs.mac,
            auth: self.auth,
            is_first: self.is_first,
        }
    }
}

impl<T: ff::Field> Add<T> for SpdzShare<T> {
    type Output = SpdzShare<T>;
    fn add(self, rhs: T) -> Self::Output {
        SpdzShare {
            value: if self.is_first {
                self.value + rhs
            } else {
                self.value
            },
            mac: self.mac + rhs * self.auth,
            auth: self.auth,
            is_first: self.is_first,
        }
    }
}

impl<T: ff::Field> Sub<T> for SpdzShare<T> {
    type Output = SpdzShare<T>;
    fn sub(self, rhs: T) -> Self::Output {
        SpdzShare {
            value: if self.is_first {
                self.value - rhs
            } else {
                self.value
            },
            mac: self.mac - rhs * self.auth,
            auth: self.auth,
            is_first: self.is_first,
        }
    }
}

impl<T: ff::Field> Neg for SpdzShare<T> {
    type Output = SpdzShare<T>;
    fn neg(self) -> Self::Output {
        SpdzShare {
            value: -self.value,
            mac: -self.mac,
            auth: self.auth,
            is_first: self.is_first,
        }
    }
}

impl<T: ff::Field> Mul<T> for SpdzShare<T> {
    type Output = SpdzShare<T>;
    fn mul(self, rhs: T) -> Self::Output {
        SpdzShare {
            value: self.value * rhs,
            mac: self.mac * rhs,
            auth: self.auth,
            is_first: self.is_first,
        }
    }
}
