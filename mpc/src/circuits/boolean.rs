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

    /// Sharing of random bit.
    pub fn random<E>(ctx: &MpcExecutionContext<E>) -> Self
    where
        E: MpcEngine<Share = T>,
    {
        Self::wrap(ctx.engine().dealer().next_bit())
    }

    /// Negate bit.
    pub fn not<E>(&self, ctx: &MpcExecutionContext<E>) -> Self
    where
        E: MpcEngine<Share = T>,
    {
        Self::wrap(ctx.one() - self.0)
    }
}

/// Logical AND.
pub async fn and<E: MpcEngine>(
    ctx: &MpcExecutionContext<E>,
    a: BitShare<E::Share>,
    b: BitShare<E::Share>,
) -> BitShare<E::Share> {
    BitShare::wrap(mul(ctx, a.raw(), b.raw()).await)
}

/// Logical OR.
pub async fn or<E: MpcEngine>(
    ctx: &MpcExecutionContext<E>,
    a: BitShare<E::Share>,
    b: BitShare<E::Share>,
) -> BitShare<E::Share> {
    BitShare::wrap(mul(ctx, a.not(ctx).raw(), b.not(ctx).raw()).await).not(ctx)
}

/// Logical XOR.
pub async fn xor<E: MpcEngine>(
    ctx: &MpcExecutionContext<E>,
    a: BitShare<E::Share>,
    b: BitShare<E::Share>,
) -> BitShare<E::Share> {
    let x = a.raw() + b.raw();
    let y = ctx.two() - x;
    BitShare::wrap(mul(ctx, x, y).await)
}

/// Ternary IF operator.
pub async fn if_cond<E: MpcEngine>(
    ctx: &MpcExecutionContext<E>,
    cond: BitShare<E::Share>,
    true_val: E::Share,
    false_val: E::Share,
) -> E::Share {
    let delta = true_val - false_val;
    false_val + mul(ctx, delta, cond.raw()).await
}

#[cfg(test)]
mod tests {
    use crate::{
        circuits::{testing::*, *},
        plaintext::PlainShare,
    };

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
                        let result = and(ctx, bits[i], bits[j]).await;
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
                        let result = or(ctx, bits[i], bits[j]).await;
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
                        let result = xor(ctx, bits[i], bits[j]).await;
                        assert_eq!(result.raw(), bits[i ^ j].raw());
                    }
                }
            })
        })
        .await;
    }

    #[tokio::test]
    async fn test_if_cond() {
        test_circuit(|ctx| {
            Box::pin(async {
                let bits = [BitShare::zero(), BitShare::one(ctx)];
                let vals = [PlainShare(420.into()), PlainShare(1337.into())];
                for i in 0..=1 {
                    let result = if_cond(ctx, bits[i], vals[1], vals[0]).await;
                    assert_eq!(result, vals[i]);
                }
            })
        })
        .await;
    }
}
