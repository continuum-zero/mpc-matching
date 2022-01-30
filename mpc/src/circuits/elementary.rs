use ff::Field;

use crate::{executor::MpcExecutionContext, join_circuits, MpcDealer, MpcEngine};

use super::join_circuits_all;

/// Multiply two shared values.
/// Cost: 1 Beaver triple, 2 partial openings, 1 communication round.
pub async fn mul<E: MpcEngine>(ctx: &MpcExecutionContext<E>, x: E::Share, y: E::Share) -> E::Share {
    let (a, b, c) = ctx.engine().dealer().next_beaver_triple();
    let (e, d) = join_circuits!(ctx.open_unchecked(x - a), ctx.open_unchecked(y - b));
    c + b * e + a * d + e * d
}

/// Compute product of given sequence of shares.
/// Cost: n-1 multiplications, log_2(n) communication rounds, where n is sequence length.
pub async fn product<E: MpcEngine>(
    ctx: &MpcExecutionContext<E>,
    elems: impl IntoIterator<Item = E::Share>,
) -> E::Share {
    let mut elems: Vec<_> = elems.into_iter().collect();

    if elems.is_empty() {
        return ctx.engine().dealer().zero() + E::Field::one();
    }

    while elems.len() > 1 {
        let mut reduced =
            join_circuits_all(elems.chunks_exact(2).map(|pair| mul(ctx, pair[0], pair[1]))).await;
        if elems.len() % 2 == 1 {
            reduced.push(*elems.last().unwrap());
        }
        elems = reduced;
    }

    elems[0]
}

#[cfg(test)]
mod tests {
    use std::iter;

    use crate::circuits::elementary::*;
    use crate::executor::run_circuit;
    use crate::plaintext::{MockMpcEngine, PlainShare};

    #[derive(ff::PrimeField)]
    #[PrimeFieldModulus = "4611686018427387903"]
    #[PrimeFieldGenerator = "7"]
    #[PrimeFieldReprEndianness = "little"]
    struct Fp([u64; 1]);

    #[tokio::test]
    async fn test_mul() {
        run_circuit(MockMpcEngine::<Fp>::new(), &[], |ctx, _| {
            Box::pin(async {
                let a = PlainShare(1337.into());
                let b = PlainShare(420.into());
                let result = mul(ctx, a, b).await;
                let result = ctx.open_unchecked(result).await;
                assert_eq!(result, Fp::from(1337 * 420));
                vec![]
            })
        })
        .await;
    }

    #[tokio::test]
    async fn test_product() {
        run_circuit(MockMpcEngine::<Fp>::new(), &[], |ctx, _| {
            Box::pin(async {
                let elems = [2, 5, 7, 11, 13, 17, 19, 1, 2, 3].map(|x| Fp::from(x));
                let expected = elems.iter().fold(Fp::one(), |x, y| x * y);
                let result = product(ctx, elems.map(|x| PlainShare(x.into()))).await;
                let result = ctx.open_unchecked(result).await;
                assert_eq!(result, expected);
                vec![]
            })
        })
        .await;
    }

    #[tokio::test]
    async fn test_product_empty_sequence() {
        run_circuit(MockMpcEngine::<Fp>::new(), &[], |ctx, _| {
            Box::pin(async {
                let result = product(ctx, iter::empty()).await;
                let result = ctx.open_unchecked(result).await;
                assert_eq!(result, Fp::one());
                vec![]
            })
        })
        .await;
    }
}
