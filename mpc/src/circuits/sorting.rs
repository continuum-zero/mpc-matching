use std::cmp;

use crate::{executor::MpcExecutionContext, MpcEngine};

use super::{join_circuits_all, IntShare};

pub async fn sort<E: MpcEngine, const N: usize>(
    ctx: &MpcExecutionContext<E>,
    elems: &mut [IntShare<E::Share, N>],
) {
    let n = elems.len();
    let mut segment = 1;

    while segment < n {
        let mut step = segment;

        while step >= 1 {
            let mut comparisons = Vec::new();

            for j in (step % segment..n - step).step_by(step * 2) {
                for i in 0..cmp::min(step, n - j - step) {
                    if (i + j) / (segment * 2) == (i + j + step) / (segment * 2) {
                        comparisons.push((i + j, i + j + step));
                    }
                }
            }

            let results = {
                let elems = &*elems;
                join_circuits_all(comparisons.iter().copied().map(|(i, j)| async move {
                    let (a, b) = (elems[i], elems[j]);
                    let should_swap = a.greater(ctx, b).await;
                    let (a, b) = should_swap.swap_if_int(ctx, a, b).await;
                    (i, j, a, b)
                }))
                .await
            };

            for (i, j, a, b) in results.into_iter() {
                elems[i] = a;
                elems[j] = b;
            }

            step /= 2;
        }

        segment *= 2;
    }
}

#[cfg(test)]
mod tests {
    use crate::circuits::{testing::*, *};

    #[tokio::test]
    async fn test_sort() {
        test_circuit(|ctx| {
            Box::pin(async {
                let mut elems =
                    [2, 1, 9, 3, 4, 7, 6, 8, 5].map(|x| IntShare::<_, 8>::plain(ctx, x));
                sort(ctx, &mut elems).await;
                let elems = join_circuits_all(elems.map(|x| x.open_unchecked(ctx))).await;
                assert_eq!(elems, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
            })
        })
        .await;
    }
}
