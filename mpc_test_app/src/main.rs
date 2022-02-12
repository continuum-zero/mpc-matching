use argh::FromArgs;
use mpc::{
    circuits::{join_circuits_all, IntShare},
    executor::{self, MpcExecutionContext},
    fields::Mersenne127,
    spdz::{PrecomputedSpdzDealer, SpdzEngine},
    transport::{self, NetworkConfig},
    MpcEngine,
};
use ndarray::Array;

#[derive(FromArgs, Debug)]
/// Test app.
struct Options {
    /// path to configuration file
    #[argh(option)]
    config: String,

    /// current party ID
    #[argh(option)]
    id: usize,

    /// path to private TLS key
    #[argh(option)]
    private_key: String,

    /// path to precomputed data file
    #[argh(option)]
    precomp: String,
}

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

    let (left, _) = mpc_flow::min_cost_bipartite_matching(ctx, costs.view())
        .await
        .unwrap();

    ctx.ensure_integrity();
    join_circuits_all(left.into_iter().map(|x| x.open_unchecked(ctx))).await
}

#[tokio::main]
async fn main() {
    let options: Options = argh::from_env();

    let party_id = options.id;
    let config = NetworkConfig::load(options.config).unwrap();
    let private_key = transport::load_private_key(options.private_key).unwrap();
    let dealer = PrecomputedSpdzDealer::from_file(options.precomp).unwrap();

    let connection = transport::connect_multiparty(&config, private_key, party_id)
        .await
        .unwrap();

    let engine: SpdzEngine<Fp, _, _> = SpdzEngine::new(dealer, connection);

    let (result, stats) = executor::run_circuit_in_background(engine, Vec::new(), |ctx, _| {
        Box::pin(test_circuit(ctx))
    })
    .await
    .unwrap();

    dbg!(result, stats);
}
