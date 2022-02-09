use ff::Field;

use crate::{executor::MpcExecutionContext, MpcDealer, MpcEngine, MpcShare};

use super::{mul, WrappedShare};

/// Share of bit value embedded in a prime field.
#[derive(Copy, Clone, Debug)]
pub struct BitShare<T>(T);

impl<T: MpcShare> WrappedShare for BitShare<T> {
    type Item = T;

    /// Wrap raw share. Input is assumed to be a sharing of a single bit.
    fn wrap(raw: T) -> Self {
        Self(raw)
    }

    /// Unwrapped MPC share.
    fn raw(&self) -> T {
        self.0
    }

    /// Reference to unwrapped MPC share.
    fn raw_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: MpcShare> BitShare<T> {
    /// Wrap plaintext boolean value.
    pub fn plain<E>(ctx: &MpcExecutionContext<E>, value: bool) -> Self
    where
        E: MpcEngine<Share = T>,
    {
        Self::wrap(if value { ctx.one() } else { T::zero() })
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

    /// Sharing of random bit.
    pub fn random<E>(ctx: &MpcExecutionContext<E>) -> Self
    where
        E: MpcEngine<Share = T>,
    {
        Self::wrap(ctx.engine().dealer().next_uint(1))
    }

    /// Open share. Requires communication.
    /// Warning: Integrity checks may be deferred (like in SPDZ protocol). Use with care.
    pub async fn open_unchecked<E>(self, ctx: &MpcExecutionContext<E>) -> bool
    where
        E: MpcEngine<Share = T>,
    {
        ctx.open_unchecked(self.0).await != E::Field::zero()
    }

    /// Logical negation.
    pub fn not<E>(self, ctx: &MpcExecutionContext<E>) -> Self
    where
        E: MpcEngine<Share = T>,
    {
        Self::wrap(ctx.one() - self.0)
    }

    /// Logical AND.
    pub async fn and<E>(self, ctx: &MpcExecutionContext<E>, rhs: Self) -> Self
    where
        E: MpcEngine<Share = T>,
    {
        Self::wrap(mul(ctx, self.0, rhs.0).await)
    }

    /// Logical OR.
    pub async fn or<E>(self, ctx: &MpcExecutionContext<E>, rhs: Self) -> Self
    where
        E: MpcEngine<Share = T>,
    {
        self.not(ctx).and(ctx, rhs.not(ctx)).await.not(ctx)
    }

    /// Logical XOR.
    pub async fn xor<E>(self, ctx: &MpcExecutionContext<E>, rhs: Self) -> Self
    where
        E: MpcEngine<Share = T>,
    {
        let x = self.0 + rhs.0;
        let y = ctx.two() - x;
        Self::wrap(mul(ctx, x, y).await)
    }

    /// Ternary IF operator.
    pub async fn select<E, Q>(self, ctx: &MpcExecutionContext<E>, true_val: Q, false_val: Q) -> Q
    where
        E: MpcEngine<Share = T>,
        Q: WrappedShare<Item = T>,
    {
        let delta = true_val.raw() - false_val.raw();
        Q::wrap(false_val.raw() + mul(ctx, delta, self.0).await)
    }

    /// Returns (x, y) if self is 0, or (y, x) if self is 1.
    pub async fn swap_if<E, Q>(self, ctx: &MpcExecutionContext<E>, x: Q, y: Q) -> (Q, Q)
    where
        E: MpcEngine<Share = T>,
        Q: WrappedShare<Item = T>,
    {
        let delta = mul(ctx, x.raw() - y.raw(), self.0).await;
        (Q::wrap(x.raw() - delta), Q::wrap(y.raw() + delta))
    }
}

impl<T: MpcShare> Default for BitShare<T> {
    fn default() -> Self {
        Self::zero()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        circuits::{testing::*, *},
        plaintext::PlainShare,
    };

    #[tokio::test]
    async fn test_plain() {
        test_circuit(|ctx| {
            Box::pin(async {
                assert_eq!(BitShare::plain(ctx, false).raw().0, ff::Field::zero());
                assert_eq!(BitShare::plain(ctx, true).raw().0, ff::Field::one());
            })
        })
        .await;
    }

    #[tokio::test]
    async fn test_open() {
        test_circuit(|ctx| {
            Box::pin(async {
                let zero = BitShare::zero();
                let one = BitShare::one(ctx);
                assert!(!zero.open_unchecked(ctx).await);
                assert!(one.open_unchecked(ctx).await);
            })
        })
        .await;
    }

    #[tokio::test]
    async fn test_not() {
        test_circuit(|ctx| {
            Box::pin(async {
                let zero = BitShare::zero();
                let one = BitShare::one(ctx);
                assert_eq!(zero.not(ctx).raw(), one.raw());
                assert_eq!(one.not(ctx).raw(), zero.raw());
            })
        })
        .await;
    }

    #[tokio::test]
    async fn test_and() {
        test_circuit(|ctx| {
            Box::pin(async {
                let bits = [BitShare::zero(), BitShare::one(ctx)];
                for i in 0..=1 {
                    for j in 0..=1 {
                        let result = bits[i].and(ctx, bits[j]).await;
                        assert_eq!(result.raw(), bits[i & j].raw());
                    }
                }
            })
        })
        .await;
    }

    #[tokio::test]
    async fn test_or() {
        test_circuit(|ctx| {
            Box::pin(async {
                let bits = [BitShare::zero(), BitShare::one(ctx)];
                for i in 0..=1 {
                    for j in 0..=1 {
                        let result = bits[i].or(ctx, bits[j]).await;
                        assert_eq!(result.raw(), bits[i | j].raw());
                    }
                }
            })
        })
        .await;
    }

    #[tokio::test]
    async fn test_xor() {
        test_circuit(|ctx| {
            Box::pin(async {
                let bits = [BitShare::zero(), BitShare::one(ctx)];
                for i in 0..=1 {
                    for j in 0..=1 {
                        let result = bits[i].xor(ctx, bits[j]).await;
                        assert_eq!(result.raw(), bits[i ^ j].raw());
                    }
                }
            })
        })
        .await;
    }

    #[tokio::test]
    async fn test_select() {
        test_circuit(|ctx| {
            Box::pin(async {
                let bits = [BitShare::zero(), BitShare::one(ctx)];
                let vals = [PlainShare(420.into()), PlainShare(1337.into())];
                for i in 0..=1 {
                    let result = bits[i].select(ctx, vals[1], vals[0]).await;
                    assert_eq!(result, vals[i]);
                }
            })
        })
        .await;
    }
}
