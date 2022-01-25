use crate::MpcContext;

/// An example protocol that evaluates a*x + b.
pub async fn linear_fn<T, C: MpcContext<T>>(
    ctx: &C,
    a: C::Share,
    b: C::Share,
    x: C::Share,
) -> C::Share {
    ctx.mul(a, x).await + b
}

#[cfg(test)]
mod tests {
    use crate::circuits::*;
    use crate::plaintext::PlainMpcContext;
    use crate::MpcContext;

    #[tokio::test]
    async fn test_linear_fn() {
        let ctx = PlainMpcContext::<u64>::new();
        let a = ctx.input(0, Some(5));
        let b = ctx.input(0, Some(7));
        let x = ctx.input(0, Some(3));
        let result = linear_fn(&ctx, a, b, x).await;
        assert_eq!(ctx.open(result).await, 22);
    }
}
