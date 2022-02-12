mod circuits;

use argh::FromArgs;
use mpc::{
    fields::Mersenne127,
    spdz::{PrecomputedSpdzDealer, SpdzEngine},
    transport::{self, NetworkConfig},
};

type Fp = Mersenne127;

const NUM_BITS: usize = 64;

const MAX_PREFERENCE_VALUE: u64 = 100;

/// Test app.
#[derive(FromArgs, Debug)]
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

    let preferences = vec![party_id as u64; 1];

    let best_match = circuits::compute_private_matching::<_, _, NUM_BITS>(
        engine,
        preferences,
        MAX_PREFERENCE_VALUE,
    )
    .await
    .unwrap();

    println!("You have been matched to {best_match}.");
}
