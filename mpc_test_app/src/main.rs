mod circuits;

use argh::FromArgs;
use mpc::{
    fields::Mersenne127,
    spdz::{PrecomputedSpdzDealer, SpdzEngine},
    transport::{self, NetworkConfig},
};

/// Field for matching computation.
type Fp = Mersenne127;

/// Number of bits for field-embedded integers.
const NUM_BITS: usize = 32;

/// Maximum value of preference vector element. Minimum is 0.
const MAX_PREFERENCE_VALUE: u64 = 100;

/// Test app.
#[derive(FromArgs, Debug)]
struct Options {
    /// path to network configuration file
    #[argh(option)]
    config: String,

    /// current party ID
    #[argh(option)]
    id: usize,

    /// path to private TLS key
    #[argh(option)]
    private_key: String,

    /// path to precomputed data for SPDZ protocol
    #[argh(option)]
    precomp: String,

    /// preference vector
    #[argh(option)]
    preferences: String,
}

#[tokio::main]
async fn main() {
    let options: Options = argh::from_env();
    let party_id = options.id;

    let preferences: Vec<u64> = options
        .preferences
        .split(',')
        .map(|x| x.parse())
        .collect::<Result<_, _>>()
        .expect("Invalid preferences vector");

    let config = NetworkConfig::load(options.config).expect("Invalid config");

    let private_key =
        transport::load_private_key(options.private_key).expect("Invalid private key");

    let dealer =
        PrecomputedSpdzDealer::from_file(options.precomp).expect("Invalid precomputed SPDZ data");

    let connection = transport::connect_multiparty(&config, private_key, party_id)
        .await
        .expect("Multiparty connection failed");

    let engine: SpdzEngine<Fp, _, _> = SpdzEngine::new(dealer, connection);

    let our_match = circuits::compute_private_matching::<_, _, NUM_BITS>(
        engine,
        preferences,
        MAX_PREFERENCE_VALUE,
    )
    .await
    .expect("MPC computation failed");

    println!("You have been matched to {our_match}.");
}
