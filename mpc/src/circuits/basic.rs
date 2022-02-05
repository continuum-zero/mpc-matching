use crate::{executor::MpcExecutionContext, join_circuits, MpcDealer, MpcEngine};

use super::join_circuits_all;

/// Sharing of a plain value.
pub fn plain<E: MpcEngine>(ctx: &MpcExecutionContext<E>, value: E::Field) -> E::Share {
    ctx.engine().dealer().share_plain(value)
}

/// Multiply two shared values.
/// Cost: 1 Beaver triple, 2 partial openings, 1 communication round.
pub async fn mul<E: MpcEngine>(ctx: &MpcExecutionContext<E>, x: E::Share, y: E::Share) -> E::Share {
    let (a, b, c) = ctx.engine().dealer().next_beaver_triple();
    let (e, d) = join_circuits!(ctx.open_unchecked(x - a), ctx.open_unchecked(y - b));
    c + b * e + a * d + plain(ctx, e * d)
}

/// Compute product of given sequence of shares.
/// Cost: n-1 multiplications, log_2(n) communication rounds, where n is sequence length.
pub async fn product<E: MpcEngine>(
    ctx: &MpcExecutionContext<E>,
    elems: impl IntoIterator<Item = E::Share>,
) -> E::Share {
    let mut elems: Vec<_> = elems.into_iter().collect();

    if elems.is_empty() {
        return ctx.one();
    }

    while elems.len() > 1 {
        let mut reduced =
            join_circuits_all(elems.chunks_exact(2).map(|p| mul(ctx, p[0], p[1]))).await;
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

    use crate::circuits::{testing::*, *};
    use crate::plaintext::PlainShare;
    use ff::Field;

    #[tokio::test]
    async fn test_mul() {
        test_circuit(|ctx| {
            Box::pin(async {
                let a = PlainShare(1337.into());
                let b = PlainShare(420.into());
                let result = mul(ctx, a, b).await;
                assert_eq!(result.0, (1337 * 420).into());
            })
        })
        .await;
    }

    #[tokio::test]
    async fn test_product() {
        test_circuit(|ctx| {
            Box::pin(async {
                let elems = [2, 5, 7, 11, 13, 17, 19, 1, 2, 3].map(|x| x.into());
                let expected = elems.iter().fold(MockField::one(), |x, y| x * y);
                let result = product(ctx, elems.map(|x| PlainShare(x))).await;
                assert_eq!(result.0, expected);
            })
        })
        .await;
    }

    #[tokio::test]
    async fn test_product_empty_sequence() {
        test_circuit(|ctx| {
            Box::pin(async {
                let result = product(ctx, iter::empty()).await;
                assert_eq!(result.0, MockField::one());
            })
        })
        .await;
    }
}
