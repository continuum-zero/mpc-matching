// TODO: These circuits will be moved later to another crate.

use ndarray::{Array, Array2, ArrayViewMut2};

use crate::{executor::MpcExecutionContext, join_circuits, MpcEngine, MpcShare};

use super::{
    fold_tree, join_circuits_all,
    sorting::{apply_swaps, apply_swaps_to_matrix, generate_sorting_swaps},
    BitShare, IntShare,
};

#[derive(Clone)]
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
        let mut state = FlowState::new(ctx, self);
        state.normalize_source_and_sink(source, sink);
        for _ in 0..flow_limit {
            state.augment().await;
        }
        state.into_flow_matrix().await
    }
}

struct FlowState<'a, E: MpcEngine, const N: usize> {
    ctx: &'a MpcExecutionContext<E>,
    permutation: Vec<IntShare<E::Share, N>>, // Current permutation of vertices.
    cost: Array2<IntShare<E::Share, N>>,     // Permuted cost matrix.
    residual: Array2<BitShare<E::Share>>,    // Permuted residual adjacency matrix.
    adjacency: Array2<BitShare<E::Share>>,   // Original adjacency matrix, not permuted.
}

struct FlowVertexState<T, const N: usize> {
    stage: VertexProcessingStage, // Processing stage.
    weight: IntShare<T, N>, // Random weight of vertex for settling draws, when choosing next vertex to process.
    distance: IntShare<T, N>, // Distance from source.
    prev_on_path: IntShare<T, N>, // Previous vertex on path from source. Undefined if vertex is unreachable.
    on_best_path: BitShare<T>,    // Is vertex on cheapest augmenting path?
}

#[derive(PartialEq, Eq)]
enum VertexProcessingStage {
    Relaxing,
    DijkstraProcessed,
    PathProcessed,
}

impl<'a, E: MpcEngine, const N: usize> FlowState<'a, E, N> {
    /// Initial state with zero flow.
    fn new(ctx: &'a MpcExecutionContext<E>, net: FlowNetwork<E::Share, N>) -> Self {
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
        apply_swaps_to_matrix(self.ctx, self.residual.view_mut(), &swaps).await;

        // Flow is difference between residual capacities and original capacities.
        let residual = self.residual.map(|&x| x.into());
        let adjacency = self.adjacency.map(|&x| x.into());
        residual - adjacency
    }

    /// Permute randomly all vertices from 2 to n-1 (0 is source, 1 is sink). Original adjacency matrix is left alone.
    async fn permute_randomly(&mut self) {
        let weights: Vec<IntShare<_, N>> = (2..self.num_vertices())
            .map(|_| IntShare::random(self.ctx))
            .collect();
        let swaps = generate_sorting_swaps(self.ctx, &weights).await;

        join_circuits!(
            apply_swaps(self.ctx, &mut self.permutation[2..], &swaps),
            apply_swaps_to_matrix(self.ctx, self.cost.slice_mut(ndarray::s![2.., 2..]), &swaps),
            apply_swaps_to_matrix(
                self.ctx,
                self.residual.slice_mut(ndarray::s![2.., 2..]),
                &swaps,
            ),
        );
    }

    /// Improve flow by 1 along the cheapest augmenting path.
    async fn augment(&mut self) {
        use VertexProcessingStage::*;

        let ctx = self.ctx;
        self.permute_randomly().await;

        // TODO: set this properly
        let distance_bound = IntShare::plain(ctx, 1000);

        let mut vertices: Vec<FlowVertexState<_, N>> = (0..self.num_vertices())
            .map(|_| FlowVertexState {
                stage: Relaxing,
                weight: IntShare::random(ctx),
                distance: distance_bound,
                prev_on_path: IntShare::zero(),
                on_best_path: BitShare::zero(),
            })
            .collect();

        let mut processing_order = Vec::new();
        let mut current = 0;
        vertices[0].distance = IntShare::zero();

        loop {
            processing_order.push(current);
            vertices[current].stage = PathProcessed;

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
                .filter(|(_, vertex)| vertex.stage == Relaxing)
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
        vertices[1].on_best_path = vertices[1].distance.less(ctx, distance_bound).await;

        for current in processing_order.into_iter().rev() {
            let prev_on_path = vertices[current]
                .on_best_path
                .select(
                    ctx,
                    IntShare::plain(ctx, -1),
                    vertices[current].prev_on_path,
                )
                .await;
            vertices[current].stage = PathProcessed;

            let changes = {
                let residual = &self.residual;
                join_circuits_all(
                    vertices
                        .iter()
                        .enumerate()
                        .filter(|(_, vertex)| vertex.stage == DijkstraProcessed)
                        .map(|(id, vertex)| async move {
                            let is_prev = prev_on_path
                                .equal(ctx, IntShare::plain(ctx, id as i64))
                                .await;
                            (
                                id,
                                join_circuits!(
                                    vertex.on_best_path.or(ctx, is_prev),
                                    is_prev.select(ctx, BitShare::zero(), residual[[id, current]]),
                                    is_prev.or(ctx, residual[[current, id]]),
                                ),
                            )
                        }),
                )
                .await
            };

            for (id, (new_on_best_path, new_edge_fwd, new_edge_bck)) in changes {
                vertices[id].on_best_path = new_on_best_path;
                self.residual[[id, current]] = new_edge_fwd;
                self.residual[[current, id]] = new_edge_bck;
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
