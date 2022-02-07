use ff::Field;

use crate::{executor::MpcExecutionContext, MpcDealer, MpcEngine, MpcShare};

use super::mul;

/// Share of bit value embedded in a prime field.
#[derive(Copy, Clone)]
pub struct BitShare<T>(T);

impl<T: MpcShare> BitShare<T> {
    /// Wrap raw share. Input is assumed to be a sharing of a single bit.
    pub fn wrap(raw: T) -> Self {
        Self(raw)
    }

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

    /// Unwrapped MPC share.
    pub fn raw(self) -> T {
        self.0
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
    pub async fn select<E: MpcEngine>(
        self,
        ctx: &MpcExecutionContext<E>,
        true_val: T,
        false_val: T,
    ) -> T
    where
        E: MpcEngine<Share = T>,
    {
        let delta = true_val - false_val;
        false_val + mul(ctx, delta, self.0).await
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
                assert_eq!(zero.open_unchecked(ctx).await, false);
                assert_eq!(one.open_unchecked(ctx).await, true);
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
