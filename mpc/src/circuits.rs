use crate::{executor::MpcExecutionContext, MpcDealer, MpcEngine};

// TODO: don't use join

pub async fn mul<E: MpcEngine>(ctx: &MpcExecutionContext<E>, x: E::Share, y: E::Share) -> E::Share {
    let (a, b, c) = ctx.dealer().next_beaver_triple();
    let (e, d) = futures::join!(ctx.partial_open(x - a), ctx.partial_open(y - b));
    c + b * e + a * d + e * d
}

pub async fn dot_product<E: MpcEngine>(
    ctx: &MpcExecutionContext<E>,
    a: E::Share,
    b: E::Share,
    c: E::Share,
    d: E::Share,
) -> E::Share {
    let (x, y) = futures::join!(mul(ctx, a, b), mul(ctx, c, d));
    x + y
}

#[cfg(test)]
mod tests {
    use std::num::Wrapping;

    use crate::circuits::*;
    use crate::executor::MpcExecutor;
    use crate::plaintext::{MockMpcEngine, PlainShare};
    use crate::MpcField;

    impl MpcField for Wrapping<u64> {} // TODO: u64 is not a field...

    #[tokio::test]
    async fn test_linear_fn() {
        let engine = MockMpcEngine::<Wrapping<u64>>::new();

        MpcExecutor::new(engine)
            .run(|ctx| {
                Box::pin(async {
                    let a = PlainShare(Wrapping(5u64));
                    let b = PlainShare(Wrapping(7u64));
                    let c = PlainShare(Wrapping(3u64));
                    let d = PlainShare(Wrapping(2u64));
                    let result = dot_product(ctx, a, b, c, d).await;
                    let result = ctx.partial_open(result).await;
                    assert_eq!(result, Wrapping(41));
                })
            })
            .await;
    }
}
