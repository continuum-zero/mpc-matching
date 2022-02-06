use crate::{
    circuits::mul, executor::MpcExecutionContext, join_circuits, MpcEngine, MpcField, MpcShare,
};

use super::{fold_tree, BitShare};

/// Compare plaintext unsigned integer with a hidden integer, provided sharings of its individual bits.
/// Returns pair of bits ([lhs < rhs], [lhs > rhs]).
pub async fn bitwise_compare<E: MpcEngine>(
    ctx: &MpcExecutionContext<E>,
    lhs: u64,
    rhs: &[BitShare<E::Share>],
) -> (BitShare<E::Share>, BitShare<E::Share>) {
    // Given bit sequences L and R, let us define f(L, R) to be a pair (cmp, neq) such that
    // a) if L < R, then cmp = -1, neq = 1;
    // b) if L > R, then cmp = 1, neq = 1;
    // c) if L = R, then cmp = 0, neq = 0.
    // The algorithm is based on observation that f(AB, CD) can be computed
    // from f(A, C) and f(B, D) using 2 multiplications and 1 communication round.
    // This enables us to compute the result in log_2(bits) rounds in binary-tree fashion.

    // 1. Map individual bits into pairs (cmp, neq).
    let base_cases = rhs.iter().enumerate().map(|(i, rhs_bit)| {
        let lhs_bit = (lhs >> i) & 1;
        if lhs_bit == 0 {
            (-rhs_bit.raw(), rhs_bit.raw())
        } else {
            let not_rhs_bit = rhs_bit.not(ctx);
            (not_rhs_bit.raw(), not_rhs_bit.raw())
        }
    });

    // 2. Fold the sequence of pairs.
    let (cmp, neq) = fold_tree(
        base_cases,
        (E::Share::zero(), E::Share::zero()),
        |lhs, rhs| async move {
            let (a, b) = join_circuits!(mul(ctx, lhs.0, rhs.1), mul(ctx, lhs.1, rhs.1));
            (lhs.0 + rhs.0 - a, lhs.1 + rhs.1 - b)
        },
    )
    .await;

    // 3. Convert aggregated pair (cmp, neq) into sharings of [lhs < rhs] and [lhs > rhs].
    let scale = E::Field::power_of_two_inverse(1);
    let is_less = BitShare::wrap((neq - cmp) * scale);
    let is_greater = BitShare::wrap((neq + cmp) * scale);
    (is_less, is_greater)
}

#[cfg(test)]
mod tests {
    use crate::circuits::{testing::*, *};

    #[tokio::test]
    async fn test_bitwise_compare() {
        test_circuit(|ctx| {
            Box::pin(async {
                let cases = [(100, 100), (100, 101), (101, 100), (100, 200), (200, 100)];

                for (lhs, rhs) in cases {
                    let rhs_bits: Vec<_> = (0..8)
                        .map(|i| BitShare::plain(ctx, ((rhs >> i) & 1) == 1))
                        .collect();

                    let (is_less, is_greater) = bitwise_compare(ctx, lhs, &rhs_bits).await;
                    let is_less = is_less.open_unchecked(ctx).await;
                    let is_greater = is_greater.open_unchecked(ctx).await;

                    assert_eq!(is_less, lhs < rhs);
                    assert_eq!(is_greater, lhs > rhs);
                }
            })
        })
        .await;
    }
}
