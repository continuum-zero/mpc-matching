use std::{fmt::Debug, future::Future, pin::Pin};

use futures::{stream::FuturesUnordered, StreamExt};

use mpc::{
    circuits::{join_circuits_all, matching, IntShare},
    executor::{self, MpcExecutionContext},
    fields::Mersenne127,
    spdz::{FakeSpdzDealer, SpdzEngine, SpdzMessage, SpdzShare},
    transport::{self, BincodeDuplex},
};
use ndarray::Array;

type Fp = Mersenne127;
type MockSpdzEngine = SpdzEngine<Fp, FakeSpdzDealer<Fp>, BincodeDuplex<SpdzMessage<Fp>>>;

async fn run_spdz<F, T>(inputs: Vec<Vec<Fp>>, circuit_fn: F) -> T
where
    T: 'static + PartialEq + Eq + Debug + Send,
    F: 'static
        + Send
        + Copy
        + Fn(
            &'_ MpcExecutionContext<MockSpdzEngine>,
            Vec<Vec<SpdzShare<Fp>>>,
        ) -> Pin<Box<dyn Future<Output = T> + '_>>,
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

    let (result, stats) = outputs.into_iter().next().unwrap();
    dbg!(stats);
    result
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

            ctx.ensure_integrity();
            join_circuits_all(left.into_iter().map(|x| x.open_unchecked(ctx))).await
        })
    })
    .await;
    dbg!(left_matches);
}

#[tokio::main]
async fn main() {
    matching(20, 10).await;
}
