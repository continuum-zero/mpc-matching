use crate::{executor::MpcExecution, join_circuits, MpcDealer, MpcEngine};

/// Multiply two shared values.
/// Cost: 1 Beaver triple, 2 partial openings, 1 communication round.
pub async fn mul<E: MpcEngine>(ctx: &MpcExecution<E>, x: E::Share, y: E::Share) -> E::Share {
    let (mask_for_x, mask_for_y, mask_for_xy) = ctx.engine().dealer().next_beaver_triple();
    let (masked_x, masked_y) = join_circuits!(
        ctx.open_unchecked(x - mask_for_x),
        ctx.open_unchecked(y - mask_for_y),
    );
    mask_for_xy + mask_for_y * masked_x + mask_for_x * masked_y + ctx.plain(masked_x * masked_y)
}

#[cfg(test)]
mod tests {
    use crate::circuits::{testing::*, *};
    use crate::plaintext::PlainShare;

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
}
