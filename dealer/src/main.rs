use std::str::FromStr;

use argh::FromArgs;
use mpc::{
    fields::{Mersenne127, Mersenne61},
    spdz::{PrecomputedSpdzData, SpdzShare},
    MpcField, MpcShare,
};
use rand::{prelude::StdRng, Rng, SeedableRng};
use serde::{Deserialize, Serialize};

/// Field type for preprocessing.
enum FieldType {
    Mersenne61,
    Mersenne127,
}

impl FromStr for FieldType {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "61" => Ok(FieldType::Mersenne61),
            "127" => Ok(FieldType::Mersenne127),
            _ => Err("Unsupported field type. Available options: 61, 127"),
        }
    }
}

#[derive(FromArgs)]
/// SPDZ offline preprocessing tool.
struct Options {
    /// number of parties participating in protocol
    #[argh(option)]
    parties: usize,

    /// output path pattern ('#' is replaced with party ID)
    #[argh(option)]
    output: String,

    /// target field
    #[argh(option, default = "FieldType::Mersenne127")]
    field: FieldType,

    /// number of beaver triples to be generated
    #[argh(option)]
    beaver_triples: usize,

    /// number of random bits to be generated
    #[argh(option)]
    random_bits: usize,

    /// number of input masks to be generated for each party
    #[argh(option)]
    input_masks: usize,
}

/// Generator of random SPDZ sharings.
struct ShareGenerator<T, R> {
    num_parties: usize,
    auth_key: T,
    rng: R,
}

impl<T, R> ShareGenerator<T, R>
where
    T: MpcField,
    R: Rng,
{
    /// Generate random sharing of given value.
    fn share(&mut self, value: T) -> Vec<SpdzShare<T>> {
        let mut shares: Vec<_> = (1..self.num_parties)
            .map(|_| SpdzShare {
                value: T::random(&mut self.rng),
                mac: T::random(&mut self.rng),
            })
            .collect();
        let sum = shares.iter().fold(SpdzShare::zero(), |acc, &x| acc + x);
        shares.push(SpdzShare {
            value: value - sum.value,
            mac: value * self.auth_key - sum.mac,
        });
        shares
    }

    /// Generate random sharing of random value.
    fn share_random(&mut self) -> (Vec<SpdzShare<T>>, T) {
        let value = T::random(&mut self.rng);
        (self.share(value), value)
    }

    /// Generate random sharing of random bit.
    fn share_random_bit(&mut self) -> (Vec<SpdzShare<T>>, T) {
        let value = T::from(self.rng.gen_range(0..=1));
        (self.share(value), value)
    }

    /// Generate beaver triples and add them to precomputed data table.
    fn fill_beaver_triples(&mut self, data: &mut [PrecomputedSpdzData<T>], count: usize) {
        for _ in 0..count {
            let (shares_a, a) = self.share_random();
            let (shares_b, b) = self.share_random();
            let shares_ab = self.share(a * b);
            for (i, party_data) in data.iter_mut().enumerate() {
                party_data
                    .beaver_triples
                    .push((shares_a[i], shares_b[i], shares_ab[i]));
            }
        }
    }

    /// Generate random bits and add them to precomputed data table.
    fn fill_random_bits(&mut self, data: &mut [PrecomputedSpdzData<T>], count: usize) {
        for _ in 0..count {
            let (shares, _) = self.share_random_bit();
            for (i, party_data) in data.iter_mut().enumerate() {
                party_data.random_bits.push(shares[i]);
            }
        }
    }

    /// Generate input masks for given party and add them to precomputed data table.
    fn fill_input_masks_for(
        &mut self,
        data: &mut [PrecomputedSpdzData<T>],
        party_id: usize,
        count: usize,
    ) {
        for _ in 0..count {
            let (shares, plain) = self.share_random();
            for (i, party_data) in data.iter_mut().enumerate() {
                party_data.input_masks[party_id].push(shares[i]);
            }
            data[party_id].input_masks_plain.push(plain);
        }
    }
}

/// Generate authorizaton key and sharings.
fn generate<T, R>(mut rng: R, options: &Options) -> Vec<PrecomputedSpdzData<T>>
where
    T: MpcField + Serialize + for<'a> Deserialize<'a>,
    R: Rng,
{
    let mut data: Vec<PrecomputedSpdzData<T>> = (0..options.parties)
        .map(|id| PrecomputedSpdzData {
            num_parties: options.parties,
            party_id: id,
            auth_key: T::random(&mut rng),
            input_masks: (0..options.parties).map(|_| Vec::new()).collect(),
            ..Default::default()
        })
        .collect();

    let auth_key = data.iter().fold(T::zero(), |acc, x| acc + x.auth_key);

    let mut share_gen = ShareGenerator {
        num_parties: options.parties,
        auth_key,
        rng,
    };

    println!("Generating {} beaver triples...", options.beaver_triples);
    share_gen.fill_beaver_triples(&mut data, options.beaver_triples);

    println!("Generating {} random bits...", options.random_bits);
    share_gen.fill_random_bits(&mut data, options.random_bits);

    println!("Generating {} input masks...", options.input_masks);
    for party_id in 0..options.parties {
        share_gen.fill_input_masks_for(&mut data, party_id, options.input_masks);
    }

    data
}

fn run<T>(options: Options)
where
    T: MpcField + Serialize + for<'a> Deserialize<'a>,
{
    println!("Generating data for {} parties", options.parties);
    let rng = StdRng::from_entropy();
    let data = generate::<T, _>(rng, &options);

    println!("Saving...");
    for (id, party_data) in data.into_iter().enumerate() {
        let output_path = options.output.replace("#", &format!("{id}"));
        party_data.save_file(output_path).unwrap();
    }
}

fn main() {
    let options: Options = argh::from_env();
    match options.field {
        FieldType::Mersenne61 => run::<Mersenne61>(options),
        FieldType::Mersenne127 => run::<Mersenne127>(options),
    }
}
