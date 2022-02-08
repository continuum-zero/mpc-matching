// TODO: These circuits will be moved later to another crate.

use ndarray::ArrayView2;

use crate::{executor::MpcExecutionContext, MpcEngine};

use super::{flow::FlowNetwork, IntShare};

pub async fn min_cost_max_matching<'a, E: MpcEngine + 'a, const N: usize>(
    ctx: &MpcExecutionContext<E>,
    costs: ArrayView2<'a, IntShare<E::Share, N>>,
) -> (Vec<IntShare<E::Share, N>>, Vec<IntShare<E::Share, N>>) {
    let n = costs.shape()[0];
    if costs.shape() != [n, n] || costs.shape() != [n, n] {
        panic!("Invalid input matrix");
    }

    let mut network = FlowNetwork::new(n * 2 + 2);

    for i in 0..n {
        network.set_edge(ctx, 0, i + 2, IntShare::zero());
        network.set_edge(ctx, n + i + 2, 1, IntShare::zero());
        for j in 0..n {
            network.set_edge(ctx, i + 2, n + j + 2, costs[[i, j]]);
        }
    }

    let flow_matrix = network.min_cost_flow(ctx, 0, 1, n).await;

    let mut left_matches = vec![IntShare::zero(); n];
    let mut right_matches = vec![IntShare::zero(); n];

    for i in 0..n {
        for j in 0..n {
            let flow = flow_matrix[[i + 2, n + j + 2]];
            left_matches[i] = left_matches[i] + flow * (j as i64);
            right_matches[j] = right_matches[i] + flow * (i as i64);
        }
    }

    (left_matches, right_matches)
}
