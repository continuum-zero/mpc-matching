use std::cmp;

use ndarray::{ArrayViewMut1, ArrayViewMut2, Axis};

use crate::{executor::MpcExecutionContext, MpcEngine};

use super::{join_circuits_all, BitShare, IntShare};

/// Pair of indices in array and hidden result of their comparison, generated by a sorting algorithm.
pub struct MaybeSwap<T> {
    first_index: usize,
    second_index: usize,
    condition: BitShare<T>,
}

/// List of swaps generated by a sorting algorithm in a single round.
type SwappingRound<T> = Vec<MaybeSwap<T>>;

/// Apply a single round of swaps generated by a sorting algorithm.
pub async fn apply_swaps_round<'a, E: MpcEngine + 'a, const N: usize>(
    ctx: &MpcExecutionContext<E>,
    elems: impl Into<ArrayViewMut1<'a, IntShare<E::Share, N>>>,
    instructions: &[MaybeSwap<E::Share>],
) {
    let mut elems = elems.into();
    let results = {
        let elems = elems.view(); // Borrow as immutable, so we can use it asynchronously below.
        join_circuits_all(instructions.into_iter().map(|inst| async move {
            (
                inst.first_index,
                inst.second_index,
                inst.condition
                    .swap_if_int(ctx, elems[inst.first_index], elems[inst.second_index])
                    .await,
            )
        }))
        .await
    };
    for (i, j, (a, b)) in results {
        elems[i] = a;
        elems[j] = b;
    }
}

/// Apply swaps generated by a sorting algorithm.
pub async fn apply_swaps<'a, E: MpcEngine + 'a, const N: usize>(
    ctx: &MpcExecutionContext<E>,
    elems: impl Into<ArrayViewMut1<'a, IntShare<E::Share, N>>>,
    instructions: &[SwappingRound<E::Share>],
) {
    let mut elems = elems.into();
    for swaps in instructions {
        apply_swaps_round(ctx, elems.view_mut(), swaps).await;
    }
}

/// Apply swaps generated by a sorting algorithm to columns and rows of a matrix.
pub async fn apply_swaps_to_matrix<'a, E: MpcEngine, const N: usize>(
    ctx: &MpcExecutionContext<E>,
    mut matrix: ArrayViewMut2<'a, IntShare<E::Share, N>>,
    instructions: &[SwappingRound<E::Share>],
) {
    for i in 0..2 {
        join_circuits_all(
            matrix
                .axis_iter_mut(Axis(i))
                .map(|vec| apply_swaps(ctx, vec, instructions)),
        )
        .await;
    }
}

/// Sort slice of shared integers. Returns list of generated swaps,
/// which can be used to rearrange other sequences without expensive comparisons.
/// Warning: guarantees only statistical privacy with (Field::SAFE_BITS - N) bits, input cannot be overflown.
pub async fn sort<E: MpcEngine, const N: usize>(
    ctx: &MpcExecutionContext<E>,
    elems: &mut [IntShare<E::Share, N>],
) -> Vec<SwappingRound<E::Share>> {
    // Iterative odd-even mergesort algorithm.
    // Based on https://en.wikipedia.org/wiki/Batcher_odd%E2%80%93even_mergesort#Pseudocode
    let n = elems.len();
    let mut segment = 1;
    let mut all_instructions = Vec::new();

    while segment < n {
        let mut step = segment;

        while step >= 1 {
            let mut futures = Vec::new();

            for j in (step % segment..n - step).step_by(step * 2) {
                for i in 0..cmp::min(step, n - j - step) {
                    if (i + j) / (segment * 2) == (i + j + step) / (segment * 2) {
                        let (index1, index2) = (i + j, i + j + step);
                        let (value1, value2) = (elems[index1], elems[index2]);
                        futures.push(async move {
                            MaybeSwap {
                                first_index: index1,
                                second_index: index2,
                                condition: value1.greater(ctx, value2).await,
                            }
                        });
                    }
                }
            }

            let instructions = join_circuits_all(futures).await;
            apply_swaps_round(ctx, &mut *elems, &instructions).await;
            all_instructions.push(instructions);
            step /= 2;
        }

        segment *= 2;
    }

    all_instructions
}

/// Generate swap instructions that sort given sequence.
/// Generated instructions can be used to rearrange other sequences without expensive comparisons.
/// Warning: guarantees only statistical privacy with (Field::SAFE_BITS - N) bits, input cannot be overflown.
pub async fn generate_sorting_swaps<E: MpcEngine, const N: usize>(
    ctx: &MpcExecutionContext<E>,
    elems: &[IntShare<E::Share, N>],
) -> Vec<SwappingRound<E::Share>> {
    let mut elems: Vec<_> = elems.into();
    sort(ctx, &mut elems).await
}

#[cfg(test)]
mod tests {
    use crate::circuits::{sorting::*, testing::*, *};

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

    #[tokio::test]
    async fn test_generate_sorting_swaps() {
        test_circuit(|ctx| {
            Box::pin(async {
                let weights = [2, 1, 9, 3, 4, 7, 6, 8, 5].map(|x| IntShare::<_, 8>::plain(ctx, x));
                let mut elems =
                    [1, 2, 3, 4, 5, 6, 7, 8, 9].map(|x| IntShare::<_, 8>::plain(ctx, x));

                let swaps = generate_sorting_swaps(ctx, &weights).await;
                apply_swaps(ctx, &mut elems, &swaps).await;

                let elems = join_circuits_all(elems.map(|x| x.open_unchecked(ctx))).await;
                assert_eq!(elems, vec![2, 1, 4, 5, 9, 7, 6, 8, 3]);
            })
        })
        .await;
    }

    #[tokio::test]
    async fn test_apply_swaps_to_matrix() {
        test_circuit(|ctx| {
            Box::pin(async {
                let weights = [3, 1, 2].map(|x| IntShare::<_, 8>::plain(ctx, x));

                let mut matrix = ndarray::array![[1, 2, 3], [4, 5, 6], [7, 8, 9]]
                    .map(|&x| IntShare::<_, 8>::plain(ctx, x));

                let swaps = generate_sorting_swaps(ctx, &weights).await;
                apply_swaps_to_matrix(ctx, matrix.view_mut(), &swaps).await;

                let elems = join_circuits_all(matrix.map(|x| x.open_unchecked(ctx))).await;
                assert_eq!(elems, vec![5, 6, 4, 8, 9, 7, 2, 3, 1]);
            })
        })
        .await;
    }
}