use std::{fmt::Debug, future::Future, net::SocketAddr, pin::Pin};

use mpc::{
    circuits::{graphs, join_circuits_all, IntShare},
    executor::{self, MpcExecutionContext},
    fields::Mersenne127,
    spdz::{FakeSpdzDealer, SpdzEngine, SpdzMessage, SpdzShare},
    transport::{self, NetChannel, NetworkPartyConfig},
};
use ndarray::Array;
use tokio::sync::oneshot;

type Fp = Mersenne127;
type MockSpdzEngine = SpdzEngine<Fp, FakeSpdzDealer<Fp>, NetChannel<SpdzMessage<Fp>>>;

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
    let mut results = Vec::new();

    let configs: Vec<_> = (0..num_parties)
        .map(|id| NetworkPartyConfig {
            addr: SocketAddr::new("127.0.0.1".parse().unwrap(), (id + 2500) as u16),
        })
        .collect();

    for party_id in 0..num_parties {
        let inputs = inputs.clone();
        let configs = configs.clone();
        let (sender, receiver) = oneshot::channel();
        results.push(receiver);

        tokio::spawn(async move {
            let channels = transport::connect_multiparty(&configs, party_id)
                .await
                .unwrap();
            let dealer = FakeSpdzDealer::new(num_parties, party_id, 123);
            let engine = MockSpdzEngine::new(dealer, channels);
            let result =
                executor::run_circuit_in_background(engine, inputs[party_id].clone(), circuit_fn)
                    .await
                    .unwrap();
            let _ = sender.send(result);
        });
    }

    let outputs: Vec<_> = futures::future::try_join_all(results.into_iter())
        .await
        .unwrap();
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
                    costs[[i, j]] = IntShare::<_, 64>::from_plain(ctx, c);
                }
            }

            let (left, _) = graphs::min_cost_bipartite_matching(ctx, costs.view())
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
    matching(14, 7).await;
}
