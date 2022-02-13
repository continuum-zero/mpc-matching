use mpc::{circuits::IntShare, executor::MpcExecution, MpcEngine};
use ndarray::ArrayView2;

use super::{FlowError, FlowNetwork};

/// Given a square matrix of costs, compute perfect bipartite matching with smallest total cost.
pub async fn min_cost_bipartite_matching<'a, E: MpcEngine + 'a, const N: usize>(
    ctx: &MpcExecution<E>,
    costs: ArrayView2<'a, IntShare<E::Share, N>>,
) -> Result<(Vec<IntShare<E::Share, N>>, Vec<IntShare<E::Share, N>>), FlowError> {
    let n = costs.shape()[0];
    if costs.shape() != [n, n] || costs.shape() != [n, n] {
        panic!("Cost matrix must be a square matrix");
    }

    // We use the standard reduction from bipartite matching to a flow problem.
    // The following flow network has 2n+2 vertices.
    // The source vertex has index 0, the sink vertex has index 1.
    // Vertices with indices 2, ..., n+1 correspond to matrix rows 0, ..., n-1.
    // Vertices with indices n+2, ..., 2n+1 correspond to matrix columns 0, ..., n-1.

    let mut network = FlowNetwork::new(n * 2 + 2);

    for i in 0..n {
        network.set_edge(ctx, 0, i + 2, IntShare::zero());
        network.set_edge(ctx, n + i + 2, 1, IntShare::zero());
        for j in 0..n {
            network.set_edge(ctx, i + 2, n + j + 2, costs[[i, j]]);
        }
    }

    let flow_matrix = network.min_cost_flow(ctx, 0, 1, n).await?;

    let mut left_matches = vec![IntShare::zero(); n];
    let mut right_matches = vec![IntShare::zero(); n];

    for i in 0..n {
        for j in 0..n {
            // Edge (i,j) is in matching if and only if there is flow through edge (i+2, n+j+2) in the network.
            let flow = flow_matrix[[i + 2, n + j + 2]];
            left_matches[i] += flow * (j as i64);
            right_matches[j] += flow * (i as i64);
        }
    }

    Ok((left_matches, right_matches))
}

#[cfg(test)]
mod tests {
    use mpc::circuits::{testing::*, *};

    use super::min_cost_bipartite_matching;

    #[tokio::test]
    async fn test_min_cost_bipartite_matching() {
        test_circuit(|ctx| {
            Box::pin(async {
                let cost_matrix = ndarray::array![
                    [5, 5, 5, 1, 5],
                    [5, 5, 1, 5, 5],
                    [1, 5, 5, 5, 5],
                    [5, 5, 5, 5, 1],
                    [5, 1, 5, 5, 5],
                ];

                let cost_matrix = cost_matrix.map(|&x| IntShare::<_, 16>::from_plain(ctx, x));
                let (left_matches, right_matches) =
                    min_cost_bipartite_matching(ctx, cost_matrix.view())
                        .await
                        .unwrap();

                let left_matches =
                    join_circuits_all(left_matches.into_iter().map(|x| x.open_unchecked(ctx)))
                        .await;

                let right_matches =
                    join_circuits_all(right_matches.into_iter().map(|x| x.open_unchecked(ctx)))
                        .await;

                assert_eq!(left_matches, [3, 2, 0, 4, 1]);
                assert_eq!(right_matches, [2, 4, 1, 0, 3]);
            })
        })
        .await;
    }
}
