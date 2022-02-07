use std::{
    cmp,
    ops::{Add, Mul, Neg, Sub},
};

use crate::{
    executor::MpcExecutionContext,
    fields::{IntoTruncated, MpcField},
    MpcDealer, MpcEngine, MpcShare,
};

use super::{bitwise_compare, BitShare};

/// Share of N-bit signed integer embedded in a prime field.
/// Value should be in range -2^(N-1) < x < 2^(N-1) (we don't allow -2^(N-1), so each value can be negated).
#[derive(Copy, Clone)]
pub struct IntShare<T, const N: usize>(T);

impl<T: MpcShare, const N: usize> IntShare<T, N> {
    /// Wrap raw share. Input is assumed to be a sharing of an N-bit signed integer.
    pub fn wrap(raw: T) -> Self {
        Self(raw)
    }

    /// Wrap plain value. Input must be an N-bit signed integer.
    pub fn plain<E>(ctx: &MpcExecutionContext<E>, value: i64) -> Self
    where
        E: MpcEngine<Share = T>,
    {
        Self::wrap(ctx.plain(embed_int_into_field::<_, N>(value)))
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

    /// Sharing of uniformly random N-bit signed integer.
    pub fn random<E>(ctx: &MpcExecutionContext<E>) -> Self
    where
        E: MpcEngine<Share = T>,
    {
        Self::wrap(ctx.engine().dealer().next_uint(N))
    }

    /// Sharing of number from sharing of its bit decomposition.
    pub fn from_bits(bits: &[BitShare<T>; N]) -> Self {
        Self::wrap(bits_to_raw_share(bits))
    }

    /// Unwrapped MPC share.
    pub fn raw(self) -> T {
        self.0
    }

    /// Open share. Requires communication.
    /// Warning: Integrity checks may be deferred (like in SPDZ protocol). Use with care.
    pub async fn open_unchecked<E>(self, ctx: &MpcExecutionContext<E>) -> i64
    where
        E: MpcEngine<Share = T>,
    {
        let opened = ctx.open_unchecked(self.0).await;
        let unsigned: u64 = (opened + E::Field::power_of_two(N - 1))
            .into_truncated()
            .wrapping_sub(1u64 << (N - 1));
        unsigned as i64
    }

    /// Multiply share by two.
    pub fn double(self) -> Self {
        Self::wrap(self.0.double())
    }

    /// Remainder of N-bit integer modulo 2^k for k < N. Result is given in range [0;2^k).
    /// Warning: guarantees only statistical privacy with (Field::SAFE_BITS - N) bits, input cannot be overflown.
    pub async fn mod_power_of_two<E>(self, ctx: &MpcExecutionContext<E>, k: usize) -> Self
    where
        E: MpcEngine<Share = T>,
    {
        if k >= N {
            panic!("Unsigned modulo overflows");
        }

        // Mod2M algorithm from "Improved Primitives for Secure Multiparty Integer Computation"
        // (https://citeseerx.ist.psu.edu/viewdoc/download?doi=10.1.1.220.9499&rep=rep1&type=pdf)

        let (mask, low, low_bits) = random_bit_mask(ctx, k);
        let masked_value = mask + self.raw() + ctx.plain(E::Field::power_of_two(N - 1));

        ctx.ensure_integrity(); // TODO: are we sure we need it to not leak anything?
        let masked_value = ctx.open_unchecked(masked_value).await;
        let masked_value = masked_value.into_truncated() % (1 << k);

        let (masked_less, _) = bitwise_compare(ctx, masked_value, &low_bits).await;
        let correction = masked_less.raw() * T::Field::power_of_two(k);

        Self::wrap(ctx.plain(masked_value.into()) - low + correction)
    }

    /// Floor division of N-bit integer by 2^k.
    /// Warning: guarantees only statistical privacy with (Field::SAFE_BITS - N) bits, input cannot be overflown.
    pub async fn div_power_of_two<E>(self, ctx: &MpcExecutionContext<E>, k: usize) -> Self
    where
        E: MpcEngine<Share = T>,
    {
        let k = cmp::min(k, N - 1);
        let remainder = self.mod_power_of_two(ctx, k).await;
        Self::wrap((self.raw() - remainder.raw()) * T::Field::power_of_two_inverse(k))
    }

    /// Test if value is less than zero.
    /// Warning: guarantees only statistical privacy with (Field::SAFE_BITS - N) bits, input cannot be overflown.
    pub async fn less_than_zero<E>(self, ctx: &MpcExecutionContext<E>) -> BitShare<T>
    where
        E: MpcEngine<Share = T>,
    {
        BitShare::wrap(-self.div_power_of_two(ctx, N - 1).await.raw())
    }

    /// Test if value is greater than zero.
    /// Warning: guarantees only statistical privacy with (Field::SAFE_BITS - N) bits, input cannot be overflown.
    pub async fn greater_than_zero<E>(self, ctx: &MpcExecutionContext<E>) -> BitShare<T>
    where
        E: MpcEngine<Share = T>,
    {
        (-self).less_than_zero(ctx).await
    }

    /// Test if self < rhs.
    /// Warning: guarantees only statistical privacy with (Field::SAFE_BITS - N) bits, input cannot be overflown.
    pub async fn less<E>(self, ctx: &MpcExecutionContext<E>, rhs: Self) -> BitShare<T>
    where
        E: MpcEngine<Share = T>,
    {
        (rhs - self).less_than_zero(ctx).await
    }

    /// Test if self > rhs.
    /// Warning: guarantees only statistical privacy with (Field::SAFE_BITS - N) bits, input cannot be overflown.
    pub async fn greater<E>(self, ctx: &MpcExecutionContext<E>, rhs: Self) -> BitShare<T>
    where
        E: MpcEngine<Share = T>,
    {
        (self - rhs).less_than_zero(ctx).await
    }

    /// Test if self <= rhs.
    /// Warning: guarantees only statistical privacy with (Field::SAFE_BITS - N) bits, input cannot be overflown.
    pub async fn less_eq<E>(self, ctx: &MpcExecutionContext<E>, rhs: Self) -> BitShare<T>
    where
        E: MpcEngine<Share = T>,
    {
        self.greater(ctx, rhs).await.not(ctx)
    }

    /// Test if self >= rhs.
    /// Warning: guarantees only statistical privacy with (Field::SAFE_BITS - N) bits, input cannot be overflown.
    pub async fn greater_eq<E>(self, ctx: &MpcExecutionContext<E>, rhs: Self) -> BitShare<T>
    where
        E: MpcEngine<Share = T>,
    {
        self.less(ctx, rhs).await.not(ctx)
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

impl<T: MpcShare, const N: usize> Mul<i64> for IntShare<T, N> {
    type Output = IntShare<T, N>;
    fn mul(self, rhs: i64) -> Self::Output {
        let rhs = embed_int_into_field::<_, N>(rhs);
        Self::wrap(self.0 * rhs)
    }
}

/// Embed signed N-bit integer into prime field.
fn embed_int_into_field<T: MpcField, const N: usize>(value: i64) -> T {
    if N < 64 {
        assert!(
            value > -(1 << (N - 1)) && value < (1 << (N - 1)),
            "Input value is out of bounds"
        );
    }
    let elem = T::from(value.unsigned_abs());
    let elem = if value < 0 { -elem } else { elem };
    elem
}

/// Combine sharing of bits into shared integer.
fn bits_to_raw_share<T: MpcShare>(bits: &[BitShare<T>]) -> T {
    bits.iter()
        .rev()
        .fold(T::zero(), |acc, &x| acc.double() + x.raw())
}

/// Function PRandM from "Improved Primitives for Secure Multiparty Integer Computation".
/// Returns sharing of a random integer X, sharing of X mod 2^k and separate sharings of the first k bits.
/// The integer X has Field::SAFE_BITS bits and is provided by dealer.
fn random_bit_mask<E: MpcEngine>(
    ctx: &MpcExecutionContext<E>,
    k: usize,
) -> (E::Share, E::Share, Vec<BitShare<E::Share>>) {
    let high_part = ctx.engine().dealer().next_uint(E::Field::SAFE_BITS - k);
    let low_bits: Vec<_> = (0..k).map(|_| BitShare::random(ctx)).collect();
    let low_part = bits_to_raw_share(&low_bits);
    let mask = high_part * E::Field::power_of_two(k) + low_part;
    (mask, low_part, low_bits)
}

#[cfg(test)]
mod tests {
    use crate::{
        circuits::{testing::*, *},
        plaintext::PlainShare,
    };

    #[tokio::test]
    async fn test_plain_positive() {
        test_circuit(|ctx| {
            Box::pin(async {
                let share: IntShare<_, 16> = IntShare::plain(ctx, 420);
                assert_eq!(share.raw().0, MockField::from(420));
            })
        })
        .await;
    }

    #[tokio::test]
    async fn test_plain_negative() {
        test_circuit(|ctx| {
            Box::pin(async {
                let share: IntShare<_, 16> = IntShare::plain(ctx, -1337);
                assert_eq!(share.raw().0, -MockField::from(1337));
            })
        })
        .await;
    }

    #[tokio::test]
    async fn test_open_positive() {
        test_circuit(|ctx| {
            Box::pin(async {
                let share: IntShare<_, 16> = IntShare::wrap(ctx.plain(1337.into()));
                assert_eq!(share.open_unchecked(ctx).await, 1337);
            })
        })
        .await;
    }

    #[tokio::test]
    async fn test_open_negative() {
        test_circuit(|ctx| {
            Box::pin(async {
                let share: IntShare<_, 16> = IntShare::wrap(ctx.plain(-MockField::from(420)));
                assert_eq!(share.open_unchecked(ctx).await, -420);
            })
        })
        .await;
    }

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

    #[tokio::test]
    async fn test_mod_power_of_two() {
        test_circuit(|ctx| {
            Box::pin(async {
                let cases = [0, 1, -1, 123, -123, 17, -17];
                for power in 0..8 {
                    for value in cases {
                        let share: IntShare<_, 8> = IntShare::plain(ctx, value);
                        let reduced = share.mod_power_of_two(ctx, power).await;
                        let reduced = reduced.open_unchecked(ctx).await;
                        let expected = value.rem_euclid(1 << power);
                        assert_eq!(reduced, expected);
                    }
                }
            })
        })
        .await;
    }

    #[tokio::test]
    async fn test_div_power_of_two() {
        test_circuit(|ctx| {
            Box::pin(async {
                let cases = [0, 1, -1, 123, -123, 17, -17];
                for power in 0..10 {
                    for value in cases {
                        let share: IntShare<_, 8> = IntShare::plain(ctx, value);
                        let reduced = share.div_power_of_two(ctx, power).await;
                        let reduced = reduced.open_unchecked(ctx).await;
                        let expected = value >> power;
                        assert_eq!(reduced, expected);
                    }
                }
            })
        })
        .await;
    }

    #[tokio::test]
    async fn test_less_than_zero() {
        test_circuit(|ctx| {
            Box::pin(async {
                let cases = [0, 1, -1, 123, -123, 17, -17];
                for value in cases {
                    let share: IntShare<_, 8> = IntShare::plain(ctx, value);
                    let bit = share.less_than_zero(ctx).await;
                    assert_eq!(bit.open_unchecked(ctx).await, value < 0);
                }
            })
        })
        .await;
    }
}
