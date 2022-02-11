use std::{future::Future, pin::Pin};

use futures::{stream::FuturesUnordered, StreamExt};

use mpc::{
    circuits::{self, join_circuits_all, matching, mul, IntShare, WrappedShare},
    executor::{self, MpcExecutionContext},
    fields::Mersenne127,
    spdz::{FakeSpdzDealer, SpdzEngine, SpdzMessage, SpdzShare},
    transport::{self, BincodeDuplex},
    MpcField,
};
use ndarray::Array;

type Fp = Mersenne127;
type MockSpdzEngine = SpdzEngine<Fp, FakeSpdzDealer<Fp>, BincodeDuplex<SpdzMessage<Fp>>>;

async fn run_spdz<F>(inputs: Vec<Vec<Fp>>, circuit_fn: F) -> Vec<Fp>
where
    F: 'static
        + Send
        + Copy
        + Fn(
            &'_ MpcExecutionContext<MockSpdzEngine>,
            Vec<Vec<SpdzShare<Fp>>>,
        ) -> Pin<Box<dyn Future<Output = Vec<SpdzShare<Fp>>> + '_>>,
{
    let num_parties = inputs.len();
    let channel_matrix = transport::mock_multiparty_channels(num_parties, 512);
    let futures = FuturesUnordered::new();

    for (party_id, transport) in channel_matrix.into_iter().enumerate() {
        let dealer = FakeSpdzDealer::new(num_parties, party_id, 123);
        let engine = MockSpdzEngine::new(dealer, transport);
        futures.push(executor::run_circuit_in_background(
            engine,
            inputs[party_id].clone(),
            circuit_fn,
        ));
    }

    let outputs: Vec<_> = futures.map(|result| result.unwrap()).collect().await;
    for i in 1..num_parties {
        assert_eq!(outputs[i], outputs[0], "Mismatched outputs",);
    }
    outputs.into_iter().next().unwrap()
}

pub async fn multiply_in_loop(num_parties: u64, num_rounds: u64, width: u64) {
    run_spdz(vec![Vec::new(); num_parties as usize], move |ctx, _| {
        Box::pin(async move {
            let mut elems: Vec<_> = (0..width).map(|x| ctx.plain(Fp::from(x))).collect();
            for _ in 0..num_rounds {
                elems = join_circuits_all(elems.into_iter().map(|x| mul(ctx, x, x))).await;
            }
            elems
        })
    })
    .await;
}

pub async fn mod2k_in_loop(num_parties: u64, num_rounds: u64, width: u64) {
    run_spdz(vec![Vec::new(); num_parties as usize], move |ctx, _| {
        Box::pin(async move {
            let mut elems: Vec<IntShare<_, 64>> =
                (0..width).map(|x| IntShare::plain(ctx, x as i64)).collect();
            for _ in 0..num_rounds {
                elems =
                    join_circuits_all(elems.into_iter().map(|x| x.mod_power_of_two(ctx, 60))).await;
            }
            elems.into_iter().map(|x| x.raw()).collect()
        })
    })
    .await;
}

pub async fn sort_seq(num_parties: u64, length: u64) {
    let sorted = run_spdz(vec![Vec::new(); num_parties as usize], move |ctx, _| {
        Box::pin(async move {
            let weights: Vec<IntShare<_, 64>> = (0..length)
                .map(|x| IntShare::plain(ctx, (length - x) as i64))
                .collect();

            let mut elems: Vec<IntShare<_, 64>> = (0..length)
                .map(|x| IntShare::plain(ctx, x as i64))
                .collect();

            let swaps = circuits::sorting::generate_sorting_swaps(ctx, &weights).await;
            circuits::sorting::apply_swaps(ctx, &mut elems, &swaps).await;

            elems.into_iter().map(|x| x.raw()).collect()
        })
    })
    .await;
    let sorted: Vec<_> = sorted.into_iter().map(|x| x.truncated()).collect();
    dbg!(sorted);
}

pub async fn matching(num_parties: usize, num_verts: usize) {
    let left_matches = run_spdz(vec![Vec::new(); num_parties], move |ctx, _| {
        Box::pin(async move {
            let mut costs = Array::default([num_verts, num_verts]);

            for i in 0..num_verts {
                for j in 0..num_verts {
                    let c = if i == (j + 1) % num_verts { 1 } else { 2 };
                    costs[[i, j]] = IntShare::<_, 64>::plain(ctx, c);
                }
            }

            let (left, _) = matching::min_cost_bipartite_matching(ctx, costs.view())
                .await
                .unwrap();
            left.into_iter().map(|x| x.raw()).collect()
        })
    })
    .await;
    let left_matches: Vec<_> = left_matches.into_iter().map(|x| x.truncated()).collect();
    dbg!(left_matches);
}

#[tokio::main]
async fn main() {
    matching(20, 10).await;
}
