use crate::{executor::MpcExecutionContext, join_circuits, MpcDealer, MpcEngine};

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
