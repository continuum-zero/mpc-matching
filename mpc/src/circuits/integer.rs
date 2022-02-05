use std::ops::{Add, Neg, Sub};

use crate::{executor::MpcExecutionContext, MpcEngine, MpcShare};

use super::BitShare;

/// Share of N-bit signed integer embedded in a prime field.
pub struct IntShare<T, const N: usize>(T);

impl<T: MpcShare, const N: usize> IntShare<T, N> {
    /// Wrap raw share. Input is assumed to be a sharing of an N-bit signed integer.
    pub fn wrap(raw: T) -> Self {
        Self(raw)
    }

    /// Unwrapped MPC share.
    pub fn raw(&self) -> T {
        self.0
    }

    /// Sharing of zero.
    pub fn zero() -> Self {
        Self::wrap(T::zero())
    }

    /// Sharing of one.
    pub fn one<E>(ctx: &MpcExecutionContext<E>) -> Self
    where
        E: MpcEngine<Share = T>,
    {
        Self::wrap(ctx.one())
    }

    /// Sharing of two.
    pub fn two<E>(ctx: &MpcExecutionContext<E>) -> Self
    where
        E: MpcEngine<Share = T>,
    {
        Self::wrap(ctx.two())
    }

    /// Sharing of number from sharing of its bits.
    pub fn from_bits(bits: &[BitShare<T>; N]) -> Self {
        bits.iter()
            .rev()
            .fold(Self::zero(), |acc, &x| acc.double() + x.into())
    }

    /// Multiply share by two.
    pub fn double(&self) -> Self {
        Self::wrap(self.0.double())
    }
}

impl<T: MpcShare, const N: usize> From<BitShare<T>> for IntShare<T, N> {
    fn from(bit: BitShare<T>) -> Self {
        Self::wrap(bit.raw())
    }
}

impl<T: MpcShare, const N: usize> Add for IntShare<T, N> {
    type Output = IntShare<T, N>;
    fn add(self, rhs: Self) -> Self::Output {
        Self::wrap(self.0 + rhs.0)
    }
}

impl<T: MpcShare, const N: usize> Sub for IntShare<T, N> {
    type Output = IntShare<T, N>;
    fn sub(self, rhs: Self) -> Self::Output {
        Self::wrap(self.0 - rhs.0)
    }
}

impl<T: MpcShare, const N: usize> Neg for IntShare<T, N> {
    type Output = IntShare<T, N>;
    fn neg(self) -> Self::Output {
        Self::wrap(-self.0)
    }
}

/// Generate shared uniformly random N-bit signed integer. Returns share of integer and all its bits.
pub fn random_bit_int<E: MpcEngine, const N: usize>(
    ctx: &MpcExecutionContext<E>,
) -> (IntShare<E::Share, N>, [BitShare<E::Share>; N]) {
    let bits = [(); N].map(|_| BitShare::random(ctx));
    (IntShare::from_bits(&bits), bits)
}

#[cfg(test)]
mod tests {
    use crate::{
        circuits::{testing::*, *},
        plaintext::PlainShare,
    };

    #[tokio::test]
    async fn test_from_bits() {
        test_circuit(|_| {
            Box::pin(async {
                let bits = [1, 0, 0, 1, 1, 1, 0, 0, 1, 0, 1]
                    .map(|b| BitShare::wrap(PlainShare(MockField::from(b))));
                let composed = IntShare::from_bits(&bits);
                assert_eq!(composed.raw().0, MockField::from(1337));
            })
        })
        .await;
    }
}
