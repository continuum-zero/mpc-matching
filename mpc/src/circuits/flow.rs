// TODO: These circuits will be moved later to another crate.

use std::fmt;

use ndarray::{Array, Array2, ArrayViewMut2};

use crate::{
    circuits::WrappedShare, executor::MpcExecutionContext, join_circuits, MpcEngine, MpcShare,
};

use super::{
    fold_tree, join_circuits_all,
    sorting::{apply_swaps, apply_swaps_to_matrix, generate_sorting_swaps},
    BitShare, IntShare,
};

/// Error during oblivious flow computation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlowError {
    PickedInvalidVertex,
}

impl fmt::Display for FlowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::PickedInvalidVertex => write!(f, "Next vertex to process is invalid"),
        }
    }
}

/// Sharing of a flow network with unit capacities and edge costs.
/// Edges must be unidirectional, i.e. `adjacency[i,j] = 0` or `adjacency[j,i] = 0`.
/// Cost matrix must be antisymmetric, i.e. `cost[i,j] = -cost[j,i]`.
/// Costs along the edges must be non-negative, i.e. `adjacency[i,j] => cost[i,j] >= 0`.
#[derive(Clone, Debug)]
pub struct FlowNetwork<T, const N: usize> {
    pub adjacency: Array2<BitShare<T>>,
    pub cost: Array2<IntShare<T, N>>,
}

impl<T: MpcShare, const N: usize> FlowNetwork<T, N> {
    /// Create new instance for given number of vertices.
    pub fn new(num_vertices: usize) -> Self {
        Self {
            adjacency: Array::default([num_vertices, num_vertices]),
            cost: Array::default([num_vertices, num_vertices]),
        }
    }

    /// Number of vertices in this flow network.
    pub fn num_vertices(&self) -> usize {
        self.adjacency.shape()[0]
    }

    /// Set edge direction and cost, given endpoints in plain and sharing of cost.
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

    /// Compute min cost flow given source, sink and limit for flow amount.
    pub async fn min_cost_flow<E>(
        self,
        ctx: &MpcExecutionContext<E>,
        source: usize,
        sink: usize,
        flow_limit: usize,
    ) -> Result<Array2<IntShare<T, N>>, FlowError>
    where
        E: MpcEngine<Share = T>,
    {
        let cost_bound = self.total_cost_bound(ctx).await;
        let mut state = FlowState::new(ctx, self, cost_bound);
        state.normalize_source_and_sink(source, sink);
        for _ in 0..flow_limit {
            state.augment().await?;
        }
        Ok(state.into_flow_matrix().await)
    }

    /// Get bound on cost of the most expensive path. Returns sum of costs on existing edges.
    async fn total_cost_bound<E>(&self, ctx: &MpcExecutionContext<E>) -> IntShare<T, N>
    where
        E: MpcEngine<Share = T>,
    {
        join_circuits_all(
            itertools::izip!(&self.adjacency, &self.cost)
                .map(|(is_edge, &cost)| is_edge.select(ctx, cost, IntShare::zero())),
        )
        .await
        .into_iter()
        .fold(IntShare::zero(), |acc, x| acc + x)
    }
}

/// State of oblivious min cost flow algorithm.
struct FlowState<'a, E: MpcEngine, const N: usize> {
    ctx: &'a MpcExecutionContext<E>,
    permutation: Vec<IntShare<E::Share, N>>, // Current permutation of vertices.
    cost: Array2<IntShare<E::Share, N>>,     // Permuted cost matrix.
    residual: Array2<BitShare<E::Share>>,    // Permuted residual adjacency matrix.
    adjacency: Array2<BitShare<E::Share>>,   // Original adjacency matrix, not permuted.
    cost_bound: IntShare<E::Share, N>,       // Strict upper bound on cost of cheapest path.
    vertices: Vec<FlowVertexState<E::Share, N>>, // States of permuted vertices.
}

/// State of vertex in oblivious min cost flow algorithm.
#[derive(Clone, Debug)]
struct FlowVertexState<T, const N: usize> {
    weight: IntShare<T, N>, // Random weight of vertex for settling draws when choosing next vertex to process.
    distance: IntShare<T, N>, // Distance from source.
    prev_on_path: IntShare<T, N>, // Previous vertex on path from source. Undefined if vertex is unreachable.
    on_best_path: BitShare<T>,    // Is vertex on cheapest augmenting path?
    processed: bool,              // Did we process this vertex in Dijkstra loop?
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
            vertices: Vec::new(),
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

    /// Improve flow by 1 along the cheapest augmenting path from source vertex 0 to sink vertex 1.
    async fn augment(&mut self) -> Result<(), FlowError> {
        self.permute_randomly().await;
        self.reset_vertices();

        let mut processing_order = vec![0];
        self.vertices[0].distance = IntShare::zero();
        self.vertices[0].processed = true;
        self.relax_distances(0).await;

        for _ in 2..self.num_vertices() {
            let id = self.pick_next_vertex().await?;
            processing_order.push(id);
            self.vertices[id].processed = true;
            self.relax_distances(id).await;
        }

        processing_order.push(1);
        self.invert_shortest_path(&processing_order).await;
        self.update_potential();
        Ok(())
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

    /// Reset states of all vertices.
    fn reset_vertices(&mut self) {
        self.vertices = (0..self.num_vertices())
            .map(|_| FlowVertexState {
                weight: IntShare::random(self.ctx),
                distance: self.cost_bound,
                prev_on_path: IntShare::zero(),
                on_best_path: BitShare::zero(),
                processed: false,
            })
            .collect();
    }

    /// Try to improve distances of neighbours of given vertex (i.e. step of Dijkstra algorithm).
    async fn relax_distances(&mut self, current: usize) {
        let ctx = self.ctx;
        let current_as_share = IntShare::plain(ctx, current as i64);
        let cur_dist = self.vertices[current].distance;
        let cost_row = self.cost.row_mut(current);
        let residual_row = self.residual.row_mut(current);

        join_circuits_all(
            itertools::izip!(
                self.vertices.iter_mut(),
                residual_row.iter(),
                cost_row.iter(),
            )
            .filter(|(vertex, _, _)| !vertex.processed)
            .map(|(vertex, &has_edge, &edge_cost)| async move {
                let alt_dist = cur_dist + edge_cost;
                let is_alt_dist_better = alt_dist.less(ctx, vertex.distance).await;
                let should_change = has_edge.and(ctx, is_alt_dist_better).await;

                let (new_dist, new_prev) = join_circuits!(
                    should_change.select(ctx, alt_dist, vertex.distance),
                    should_change.select(ctx, current_as_share, vertex.prev_on_path)
                );

                vertex.distance = new_dist;
                vertex.prev_on_path = new_prev;
            }),
        )
        .await;
    }

    /// Find unprocessed vertex other than sink, which is closest to source, and output its index in plain.
    /// Draws between equally distanced vertices are settled using vertex weights, which should be random.
    async fn pick_next_vertex(&mut self) -> Result<usize, FlowError> {
        let ctx = self.ctx;

        // 1. Build list of triples (vertex ID, distance, weight).
        let candidates = self
            .vertices
            .iter()
            .enumerate()
            .filter(|(id, vertex)| *id >= 2 && !vertex.processed)
            .map(|(id, vertex)| {
                (
                    IntShare::<_, N>::plain(self.ctx, id as i64),
                    vertex.distance,
                    vertex.weight,
                )
            });

        // 2. Find triple with smallest pair (distance, weight).
        let (best_id, _, _) = fold_tree(
            candidates,
            (IntShare::zero(), IntShare::zero(), IntShare::zero()),
            |(id1, dist1, weight1), (id2, dist2, weight2)| async move {
                let (dist1_less, dist1_equal, weight1_less) = join_circuits!(
                    dist1.less(ctx, dist2),
                    dist1.equal(ctx, dist2),
                    weight1.less(ctx, weight2)
                );

                let is_first_better = dist1_less
                    .or(ctx, dist1_equal.and(ctx, weight1_less).await)
                    .await;

                join_circuits!(
                    is_first_better.select(ctx, id1, id2),
                    is_first_better.select(ctx, dist1, dist2),
                    is_first_better.select(ctx, weight1, weight2),
                )
            },
        )
        .await;

        // Check integrity before opening the index in plain, so attacker cannot leak anything.
        self.ctx.ensure_integrity();
        let best_id = best_id.open_unchecked(self.ctx).await as usize;

        if best_id >= 2 && best_id < self.num_vertices() && !self.vertices[best_id].processed {
            Ok(best_id)
        } else {
            Err(FlowError::PickedInvalidVertex)
        }
    }

    /// Invert shortest path from source vertex 0 to sink vertex 1, given Dijkstra processing order.
    async fn invert_shortest_path(&mut self, processing_order: &[usize]) {
        let ctx = self.ctx;

        // If distance from source to sink is equal to cost bound, then there is no path.
        // If that's not the case, mark the sink vertex.
        self.vertices[1].on_best_path = self.vertices[1].distance.less(ctx, self.cost_bound).await;

        // If the shortest path exists, then its consecutive vertices form subsequence of processing order.
        // We can thus iterate in the reversed order and mark vertices of shortest path one by one.
        for i in (1..processing_order.len()).rev() {
            let current = processing_order[i];

            // If current vertex is not on the shortest path, then we set `prev_on_path` to -1, so nothing happens.
            let prev_on_path = self.vertices[current]
                .on_best_path
                .select(
                    ctx,
                    self.vertices[current].prev_on_path,
                    IntShare::plain(ctx, -1),
                )
                .await;

            // For each possible previous vertex, compute bit denoting if it's equal to prev_on_path.
            let prev_indicators =
                join_circuits_all(processing_order[0..i].iter().map(|&id| async move {
                    let is_prev = prev_on_path
                        .equal(ctx, IntShare::plain(ctx, id as i64))
                        .await;
                    (id, is_prev)
                }))
                .await;

            // Mark predecesssor and invert appropriate edge (if current vertex is on path).
            for (id, is_prev) in prev_indicators {
                // The following happens at most once for each vertex and edge, so it's safe to do this using addition.
                *self.vertices[id].on_best_path.raw_mut() += is_prev.raw();
                *self.residual[[id, current]].raw_mut() -= is_prev.raw();
                *self.residual[[current, id]].raw_mut() += is_prev.raw();
            }
        }
    }

    /// Update edge costs after inverting path, so that they are non-negative and shortest paths don't change.
    fn update_potential(&mut self) {
        for i in 0..self.num_vertices() {
            for j in 0..self.num_vertices() {
                self.cost[[i, j]] += self.vertices[i].distance - self.vertices[j].distance;
            }
        }
    }
}

/// Update matrix so that vertices `i` and `j` are swapped.
fn swap_vertices<T>(mut matrix: ArrayViewMut2<T>, i: usize, j: usize) {
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
                        .await
                        .unwrap();
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
