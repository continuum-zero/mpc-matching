// TODO: These circuits will be moved later to another crate.

use ndarray::{Array, Array2, ArrayViewMut2};

use crate::{
    circuits::WrappedShare, executor::MpcExecutionContext, join_circuits, MpcEngine, MpcShare,
};

use super::{
    fold_tree, join_circuits_all,
    sorting::{apply_swaps, apply_swaps_to_matrix, generate_sorting_swaps},
    BitShare, IntShare,
};

#[derive(Clone, Debug)]
pub struct FlowNetwork<T, const N: usize> {
    pub adjacency: Array2<BitShare<T>>,
    pub cost: Array2<IntShare<T, N>>,
}

impl<T: MpcShare, const N: usize> FlowNetwork<T, N> {
    pub fn new(num_vertices: usize) -> Self {
        Self {
            adjacency: Array::default([num_vertices, num_vertices]),
            cost: Array::default([num_vertices, num_vertices]),
        }
    }

    pub fn num_vertices(&self) -> usize {
        self.adjacency.shape()[0]
    }

    pub fn set_edge<E>(
        &mut self,
        ctx: &MpcExecutionContext<E>,
        from: usize,
        to: usize,
        cost: IntShare<T, N>,
    ) where
        E: MpcEngine<Share = T>,
    {
        self.adjacency[[from, to]] = BitShare::one(ctx);
        self.adjacency[[to, from]] = BitShare::zero();
        self.cost[[from, to]] = cost;
        self.cost[[to, from]] = -cost;
    }

    pub async fn min_cost_flow<E>(
        self,
        ctx: &MpcExecutionContext<E>,
        source: usize,
        sink: usize,
        flow_limit: usize,
    ) -> Array2<IntShare<T, N>>
    where
        E: MpcEngine<Share = T>,
    {
        let cost_bound = self.total_cost_bound(ctx).await;
        let mut state = FlowState::new(ctx, self, cost_bound);
        state.normalize_source_and_sink(source, sink);
        for _ in 0..flow_limit {
            state.augment().await;
        }
        state.into_flow_matrix().await
    }

    async fn total_cost_bound<E>(&self, ctx: &MpcExecutionContext<E>) -> IntShare<T, N>
    where
        E: MpcEngine<Share = T>,
    {
        join_circuits_all(
            itertools::izip!(&self.cost, &self.adjacency)
                .map(|(&cost, is_edge)| is_edge.select(ctx, cost, IntShare::zero())),
        )
        .await
        .into_iter()
        .fold(IntShare::zero(), |acc, x| acc + x)
    }
}

struct FlowState<'a, E: MpcEngine, const N: usize> {
    ctx: &'a MpcExecutionContext<E>,
    permutation: Vec<IntShare<E::Share, N>>, // Current permutation of vertices.
    cost: Array2<IntShare<E::Share, N>>,     // Permuted cost matrix.
    residual: Array2<BitShare<E::Share>>,    // Permuted residual adjacency matrix.
    adjacency: Array2<BitShare<E::Share>>,   // Original adjacency matrix, not permuted.
    cost_bound: IntShare<E::Share, N>,       // Strict upper bound on cost of cheapest path.
}

#[derive(Clone, Debug)]
struct FlowVertexState<T, const N: usize> {
    stage: VertexProcessingStage, // Processing stage.
    weight: IntShare<T, N>, // Random weight of vertex for settling draws, when choosing next vertex to process.
    distance: IntShare<T, N>, // Distance from source.
    prev_on_path: IntShare<T, N>, // Previous vertex on path from source. Undefined if vertex is unreachable.
    on_best_path: BitShare<T>,    // Is vertex on cheapest augmenting path?
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum VertexProcessingStage {
    Relaxing,
    DijkstraProcessed,
}

impl<'a, E: MpcEngine, const N: usize> FlowState<'a, E, N> {
    /// Initial state with zero flow.
    fn new(
        ctx: &'a MpcExecutionContext<E>,
        net: FlowNetwork<E::Share, N>,
        cost_bound: IntShare<E::Share, N>,
    ) -> Self {
        let n = net.adjacency.shape()[0];
        if net.adjacency.shape() != [n, n] || net.cost.shape() != [n, n] {
            panic!("Invalid input matrices");
        }

        Self {
            ctx,
            permutation: (0..n).map(|x| IntShare::plain(ctx, x as i64)).collect(),
            cost: net.cost,
            residual: net.adjacency.to_owned(),
            adjacency: net.adjacency,
            cost_bound: cost_bound + IntShare::one(ctx),
        }
    }

    /// Number of vertices in flow network.
    fn num_vertices(&self) -> usize {
        self.permutation.len()
    }

    /// Swap vertices so that source has index 0 and sink index 1. Original adjacency matrix is left alone.
    fn normalize_source_and_sink(&mut self, source: usize, mut sink: usize) {
        if source != 0 {
            swap_vertices(self.residual.view_mut(), source, 0);
            swap_vertices(self.cost.view_mut(), source, 0);
            self.permutation.swap(source, 0);
            if sink == 0 {
                sink = source; // We swapped source with sink.
            }
        }
        if sink != 1 {
            swap_vertices(self.residual.view_mut(), sink, 1);
            swap_vertices(self.cost.view_mut(), sink, 1);
            self.permutation.swap(sink, 1);
        }
    }

    /// Get matrix that contains flow amount for each edge.
    async fn into_flow_matrix(mut self) -> Array2<IntShare<E::Share, N>> {
        // Invert permutation of vertices in residual matrix (original adjacency is not permuted).
        let swaps = generate_sorting_swaps(self.ctx, &self.permutation).await;
        apply_swaps_to_matrix(self.ctx, self.residual.view_mut(), &swaps, 0).await;

        // Flow is difference between original capacities and residual capacities.
        let residual = self.residual.map(|&x| x.into());
        let adjacency = self.adjacency.map(|&x| x.into());
        adjacency - residual
    }

    /// Permute randomly all vertices from 2 to n-1 (0 is source, 1 is sink). Original adjacency matrix is left alone.
    async fn permute_randomly(&mut self) {
        let weights: Vec<IntShare<_, N>> = (2..self.num_vertices())
            .map(|_| IntShare::random(self.ctx))
            .collect();
        let swaps = generate_sorting_swaps(self.ctx, &weights).await;

        join_circuits!(
            apply_swaps(self.ctx, &mut self.permutation[2..], &swaps),
            apply_swaps_to_matrix(self.ctx, self.cost.view_mut(), &swaps, 2),
            apply_swaps_to_matrix(self.ctx, self.residual.view_mut(), &swaps, 2),
        );
    }

    /// Improve flow by 1 along the cheapest augmenting path.
    async fn augment(&mut self) {
        use VertexProcessingStage::*;

        let ctx = self.ctx;
        self.permute_randomly().await;

        let mut vertices: Vec<FlowVertexState<_, N>> = (0..self.num_vertices())
            .map(|_| FlowVertexState {
                stage: Relaxing,
                weight: IntShare::random(ctx),
                distance: self.cost_bound,
                prev_on_path: IntShare::zero(),
                on_best_path: BitShare::zero(),
            })
            .collect();

        let mut processing_order = Vec::new();
        let mut current = 0;
        vertices[0].distance = IntShare::zero();

        loop {
            processing_order.push(current);
            vertices[current].stage = DijkstraProcessed;

            let current_as_share = IntShare::plain(ctx, current as i64);
            let cur_dist = vertices[current].distance;
            let cost_row = self.cost.row_mut(current);
            let residual_row = self.residual.row_mut(current);

            join_circuits_all(
                itertools::izip!(vertices.iter_mut(), cost_row.iter(), residual_row.iter())
                    .filter(|(vertex, _, _)| vertex.stage == Relaxing)
                    .map(|(vertex, &edge_cost, &has_edge)| async move {
                        let alt_dist = cur_dist + edge_cost;
                        let is_dist_better = alt_dist.less(ctx, vertex.distance).await;
                        let should_change = has_edge.and(ctx, is_dist_better).await;

                        let (new_dist, new_prev) = join_circuits!(
                            should_change.select(ctx, alt_dist, vertex.distance),
                            should_change.select(ctx, current_as_share, vertex.prev_on_path)
                        );

                        vertex.distance = new_dist;
                        vertex.prev_on_path = new_prev;
                    }),
            )
            .await;

            if processing_order.len() == self.num_vertices() - 1 {
                break;
            }

            let candidates = vertices
                .iter()
                .enumerate()
                .filter(|(id, vertex)| *id >= 2 && vertex.stage == Relaxing)
                .map(|(id, vertex)| {
                    (
                        IntShare::<_, N>::plain(ctx, id as i64),
                        vertex.distance,
                        vertex.weight,
                    )
                });

            let (best_id, _, _) = fold_tree(
                candidates,
                (IntShare::zero(), IntShare::zero(), IntShare::zero()),
                |(id1, dist1, weight1), (id2, dist2, weight2)| async move {
                    let (dist1_less, dist1_equal, weight1_less) = join_circuits!(
                        dist1.less(ctx, dist2),
                        dist1.equal(ctx, dist2),
                        weight1.less(ctx, weight2)
                    );

                    let cond = dist1_less
                        .or(ctx, dist1_equal.and(ctx, weight1_less).await)
                        .await;

                    join_circuits!(
                        cond.select(ctx, id1, id2),
                        cond.select(ctx, dist1, dist2),
                        cond.select(ctx, weight1, weight2),
                    )
                },
            )
            .await;

            ctx.ensure_integrity();
            current = best_id.open_unchecked(ctx).await as usize;

            if current < 2 || current >= self.num_vertices() {
                panic!("Invalid vertex"); // TODO: don't panic, return Err
            }
            if vertices[current].stage != Relaxing {
                panic!("Invalid vertex stage"); // TODO: don't panic, return Err
            }
        }

        processing_order.push(1);
        vertices[1].on_best_path = vertices[1].distance.less(ctx, self.cost_bound).await;

        for i in (0..processing_order.len()).rev() {
            let current = processing_order[i];
            let prev_on_path = vertices[current]
                .on_best_path
                .select(
                    ctx,
                    vertices[current].prev_on_path,
                    IntShare::plain(ctx, -1),
                )
                .await;

            let prev_indicators =
                join_circuits_all(processing_order[0..i].iter().map(|&id| async move {
                    let is_prev = prev_on_path
                        .equal(ctx, IntShare::plain(ctx, id as i64))
                        .await;
                    (id, is_prev)
                }))
                .await;

            for (id, is_prev) in prev_indicators {
                vertices[id].on_best_path =
                    BitShare::wrap(vertices[id].on_best_path.raw() + is_prev.raw());
                self.residual[[id, current]] =
                    BitShare::wrap(self.residual[[id, current]].raw() - is_prev.raw());
                self.residual[[current, id]] =
                    BitShare::wrap(self.residual[[current, id]].raw() + is_prev.raw());
            }
        }

        for i in 0..self.num_vertices() {
            for j in 0..self.num_vertices() {
                self.cost[[i, j]] = self.cost[[i, j]] + vertices[i].distance - vertices[j].distance;
            }
        }
    }
}

fn swap_vertices<'a, T>(mut matrix: ArrayViewMut2<'a, T>, i: usize, j: usize) {
    for k in 0..matrix.shape()[0] {
        matrix.swap([k, i], [k, j]);
    }
    for k in 0..matrix.shape()[1] {
        matrix.swap([i, k], [j, k]);
    }
}

#[cfg(test)]
mod tests {
    use ndarray::{Array, Array2};

    use crate::{
        circuits::{testing::*, *},
        executor::MpcExecutionContext,
    };

    use super::FlowNetwork;

    #[derive(Clone, Debug)]
    struct TestNetwork {
        adjacency: Array2<bool>,
        cost: Array2<i64>,
        expected_flow: Array2<i64>,
    }

    impl TestNetwork {
        fn new(n: usize) -> Self {
            Self {
                adjacency: Array::default([n, n]),
                cost: Array::default([n, n]),
                expected_flow: Array::default([n, n]),
            }
        }

        fn num_vertices(&self) -> usize {
            self.adjacency.shape()[0]
        }

        fn set_edge(mut self, from: usize, to: usize, cost: i64, has_flow: bool) -> Self {
            self.adjacency[[from, to]] = true;
            self.adjacency[[to, from]] = false;
            self.cost[[from, to]] = cost;
            self.cost[[to, from]] = -cost;
            self.expected_flow[[from, to]] = if has_flow { 1 } else { 0 };
            self.expected_flow[[to, from]] = if has_flow { -1 } else { 0 };
            self
        }

        fn shared(&self, ctx: &MpcExecutionContext<MockEngine>) -> FlowNetwork<MockShare, 32> {
            FlowNetwork {
                adjacency: self.adjacency.map(|&x| BitShare::plain(ctx, x)),
                cost: self.cost.map(|&x| IntShare::plain(ctx, x)),
            }
        }

        async fn test(self, source: usize, sink: usize) {
            test_circuit(|ctx| {
                Box::pin(async move {
                    let shared_net = self.shared(ctx);
                    let flow_matrix = shared_net
                        .min_cost_flow(ctx, source, sink, self.num_vertices())
                        .await;
                    let flow_matrix = open_matrix(ctx, flow_matrix).await;
                    assert_eq!(flow_matrix, self.expected_flow);
                })
            })
            .await;
        }
    }

    async fn open_matrix(
        ctx: &MpcExecutionContext<MockEngine>,
        matrix: Array2<IntShare<MockShare, 32>>,
    ) -> Array2<i64> {
        let n = matrix.shape()[0];
        let elems = join_circuits_all(matrix.map(|x| x.open_unchecked(ctx))).await;
        Array2::from_shape_vec([n, n], elems).unwrap()
    }

    #[tokio::test]
    async fn test_min_cost_flow() {
        for _ in 0..10 {
            TestNetwork::new(5)
                .set_edge(0, 2, 1, true)
                .set_edge(0, 4, 5, true)
                .set_edge(2, 4, 1, false)
                .set_edge(2, 3, 10, false)
                .set_edge(2, 1, 5, true)
                .set_edge(4, 3, 1, true)
                .set_edge(3, 1, 1, true)
                .test(0, 1)
                .await;
        }
    }
}
