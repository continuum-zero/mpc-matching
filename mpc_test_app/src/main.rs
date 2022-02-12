use mpc::{
    circuits::{graphs, join_circuits_all, IntShare},
    executor::{self, MpcExecutionContext, MpcExecutionStats},
    fields::Mersenne127,
    spdz::{FakeSpdzDealer, SpdzEngine},
    transport::{self, NetworkConfig},
    MpcEngine,
};
use ndarray::Array;

type Fp = Mersenne127;

pub async fn test_circuit<E: MpcEngine>(ctx: &MpcExecutionContext<E>) -> Vec<i64> {
    let num_verts = 5;
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
}

async fn run_node(conf: NetworkConfig, party_id: usize) -> (Vec<i64>, MpcExecutionStats) {
    let private_key =
        transport::load_private_key(format!("test-env/node{party_id}/private.key")).unwrap();

    let connection = transport::connect_multiparty(&conf, private_key, party_id)
        .await
        .unwrap();

    let dealer: FakeSpdzDealer<Fp> = FakeSpdzDealer::new(conf.parties.len(), party_id, 123);
    let engine: SpdzEngine<Fp, _, _> = SpdzEngine::new(dealer, connection);

    executor::run_circuit_in_background(engine, Vec::new(), |ctx, _| Box::pin(test_circuit(ctx)))
        .await
        .unwrap()
}

#[tokio::main]
async fn main() {
    let conf = NetworkConfig::load("test-env/common/config.json").unwrap();

    let results: Vec<_> = futures::future::join_all(
        (0..conf.parties.len())
            .map(|id| tokio::spawn(run_node(conf.clone(), id)))
            .map(|task| async move { task.await.unwrap() }),
    )
    .await;

    let (result, stats) = &results[0];
    dbg!(result, stats);
    assert!(results.iter().all(|x| *x == results[0]));
}
