use crate::{executor::MpcExecutor, MpcEngine};

pub async fn dot_product<'a, E: MpcEngine>(
    ctx: &MpcExecutor<'a, E>,
    a: E::Share,
    b: E::Share,
    c: E::Share,
    d: E::Share,
) -> E::Share {
    let (x, y) = futures::join!(ctx.mul(a, b), ctx.mul(c, d));
    x + y
}

#[cfg(test)]
mod tests {
    use crate::circuits::*;
    use crate::plaintext::PlainMpcEngine;

    #[tokio::test]
    async fn test_linear_fn() {
        let engine = PlainMpcEngine::<u64>::new();
        let executor = MpcExecutor::new(&engine);
        let result = executor
            .run_circuit(|| async {
                let (a, b, c, d) = futures::join!(
                    executor.input(0, Some(5)),
                    executor.input(0, Some(7)),
                    executor.input(0, Some(3)),
                    executor.input(0, Some(2))
                );
                let result = dot_product(&executor, a, b, c, d).await;
                executor.open(result).await
            })
            .await;
        assert_eq!(result, 41);
    }
}
