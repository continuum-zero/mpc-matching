use std::future::Future;

use itertools::Itertools;

use crate::{executor::MpcExecution, MpcEngine};

use super::{join_circuits_all, mul};

/// Single element or pair of elements of the same type.
enum SingleOrPair<T> {
    Single(T),
    Pair(T, T),
}

/// Batch iterator into pairs and maybe a leftover single element.
fn batch_pairs<T>(it: impl IntoIterator<Item = T>) -> impl Iterator<Item = SingleOrPair<T>> {
    it.into_iter().batching(|it| {
        it.next().map(|first| match it.next() {
            Some(second) => SingleOrPair::Pair(first, second),
            None => SingleOrPair::Single(first),
        })
    })
}

/// Aggregate an iterator of elements by combining distinct pairs in log_2(n) rounds.
pub async fn fold_tree<T, F, Fut>(iter: impl IntoIterator<Item = T>, default: T, combine_fn: F) -> T
where
    F: Copy + Fn(T, T) -> Fut,
    Fut: Future<Output = T>,
{
    let mut elems: Vec<_> = iter.into_iter().collect();

    while elems.len() > 1 {
        elems = join_circuits_all(batch_pairs(elems).map(|p| async move {
            match p {
                SingleOrPair::Single(value) => value,
                SingleOrPair::Pair(first, second) => combine_fn(first, second).await,
            }
        }))
        .await;
    }

    elems.into_iter().next().unwrap_or(default)
}

/// Compute product of given sequence of shares.
/// Cost: n-1 multiplications, log_2(n) communication rounds, where n is sequence length.
pub async fn product<E: MpcEngine>(
    ctx: &MpcExecution<E>,
    elems: impl IntoIterator<Item = E::Share>,
) -> E::Share {
    fold_tree(elems, ctx.one(), |a, b| mul(ctx, a, b)).await
}

#[cfg(test)]
mod tests {
    use std::iter;

    use crate::circuits::{testing::*, *};
    use crate::plaintext::PlainShare;
    use ff::Field;

    #[tokio::test]
    async fn test_product() {
        test_circuit(|ctx| {
            Box::pin(async {
                let elems = [2, 5, 7, 11, 13, 17, 19, 1, 2, 3].map(|x| x.into());
                let expected = elems.iter().fold(MockField::one(), |x, y| x * y);
                let result = product(ctx, elems.map(PlainShare)).await;
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
