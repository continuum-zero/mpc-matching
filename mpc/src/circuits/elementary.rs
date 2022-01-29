use crate::{executor::MpcExecutionContext, join_circuits, MpcDealer, MpcEngine};

pub async fn mul<E: MpcEngine>(ctx: &MpcExecutionContext<E>, x: E::Share, y: E::Share) -> E::Share {
    let (a, b, c) = ctx.dealer().next_beaver_triple();
    let (e, d) = join_circuits!(ctx.partial_open(x - a), ctx.partial_open(y - b));
    c + b * e + a * d + e * d
}

pub async fn dot_product<E: MpcEngine>(
    ctx: &MpcExecutionContext<E>,
    a: E::Share,
    b: E::Share,
    c: E::Share,
    d: E::Share,
) -> E::Share {
    let (x, y) = join_circuits!(mul(ctx, a, b), mul(ctx, c, d));
    x + y
}

#[cfg(test)]
mod tests {
    use crate::circuits::elementary::*;
    use crate::executor::MpcExecutor;
    use crate::plaintext::{MockMpcEngine, PlainShare};

    #[derive(ff::PrimeField)]
    #[PrimeFieldModulus = "4611686018427387903"]
    #[PrimeFieldGenerator = "7"]
    #[PrimeFieldReprEndianness = "little"]
    struct Fp([u64; 1]);

    #[tokio::test]
    async fn test_linear_fn() {
        let engine = MockMpcEngine::<Fp>::new();

        MpcExecutor::new(engine)
            .run(|ctx| {
                Box::pin(async {
                    let a = PlainShare(5.into());
                    let b = PlainShare(7.into());
                    let c = PlainShare(3.into());
                    let d = PlainShare(2.into());
                    let result = dot_product(ctx, a, b, c, d).await;
                    let result = ctx.partial_open(result).await;
                    assert_eq!(result, 41.into());
                })
            })
            .await;
    }
}
