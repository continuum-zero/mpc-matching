use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use crate::fields::MpcField;
use crate::{MpcContext, MpcDealer};

use super::{SpdzDealer, SpdzShare};

/// Insecure dealer for SPDZ protocol that can be used for tests.
pub struct FakeSpdzDealer<T> {
    auth_key: FakeAuthKey<T>,
    beaver_triple_gen: FakeShareGenerator<T>,
    bits_gen: FakeShareGenerator<T>,
    input_masks_gen: Vec<FakeShareGenerator<T>>,
}

impl<T: MpcField> FakeSpdzDealer<T> {
    /// Create new instance.
    pub fn new(num_parties: usize, party_id: usize, seed: u8) -> Self {
        let mut rng = SmallRng::from_seed([seed; 32]);
        let auth_key = FakeAuthKey::random(&mut rng, party_id, num_parties);
        Self {
            auth_key,
            beaver_triple_gen: FakeShareGenerator::new(auth_key, rng.gen()),
            bits_gen: FakeShareGenerator::new(auth_key, rng.gen()),
            input_masks_gen: (0..num_parties)
                .map(|_| FakeShareGenerator::new(auth_key, rng.gen()))
                .collect(),
        }
    }
}

impl<T: MpcField> MpcContext for FakeSpdzDealer<T> {
    type Field = T;
    type Share = SpdzShare<T>;

    fn num_parties(&self) -> usize {
        self.auth_key.num_parties
    }

    fn party_id(&self) -> usize {
        self.auth_key.party_id
    }
}

impl<T: MpcField> MpcDealer for FakeSpdzDealer<T> {
    fn share_plain(&self, x: Self::Field) -> Self::Share {
        SpdzShare {
            value: if self.auth_key.party_id == 0 {
                x
            } else {
                T::zero()
            },
            mac: x * self.auth_key.share_value,
        }
    }

    fn next_beaver_triple(&mut self) -> (Self::Share, Self::Share, Self::Share) {
        let (a_share, a_plain) = self.beaver_triple_gen.gen_random_authenticated_share();
        let (b_share, b_plain) = self.beaver_triple_gen.gen_random_authenticated_share();
        let c_share = self
            .beaver_triple_gen
            .gen_authenticated_share(a_plain * b_plain);
        (a_share, b_share, c_share)
    }

    fn next_bit(&mut self) -> Self::Share {
        let value = if self.bits_gen.rng().gen() {
            Self::Field::one()
        } else {
            Self::Field::zero()
        };
        self.bits_gen.gen_authenticated_share(value)
    }
}

impl<T: MpcField> SpdzDealer for FakeSpdzDealer<T> {
    fn authentication_key_share(&self) -> Self::Field {
        self.auth_key.share_value
    }

    fn next_input_mask_own(&mut self) -> (Self::Share, Self::Field) {
        self.input_masks_gen[self.auth_key.party_id].gen_random_authenticated_share()
    }

    fn next_input_mask_for(&mut self, id: usize) -> Self::Share {
        self.input_masks_gen[id].gen_random_authenticated_share().0
    }
}

/// Authentication key in plain and its share.
#[derive(Copy, Clone)]
struct FakeAuthKey<T> {
    num_parties: usize,
    party_id: usize,
    share_value: T,
    plain_value: T,
}

impl<T: MpcField> FakeAuthKey<T> {
    /// Generate fake authentication key and its share.
    fn random(rng: &mut impl Rng, party_id: usize, num_parties: usize) -> Self {
        let (share_value, plain_value) = gen_random_raw_share(rng, party_id, num_parties);
        Self {
            num_parties,
            party_id,
            share_value,
            plain_value,
        }
    }
}

/// Insecure generator of SPDZ-shared values.
struct FakeShareGenerator<T> {
    auth_key: FakeAuthKey<T>,
    rng: SmallRng,
}

impl<T: MpcField> FakeShareGenerator<T> {
    /// Create new generator.
    fn new(auth_key: FakeAuthKey<T>, seed: [u8; 32]) -> Self {
        Self {
            rng: SmallRng::from_seed(seed),
            auth_key,
        }
    }

    /// Get underlying random number generator.
    fn rng(&mut self) -> &mut impl Rng {
        &mut self.rng
    }

    /// Generate local unauthenticated share of specified value.
    fn gen_raw_share(&mut self, value: T) -> T {
        gen_raw_share(
            &mut self.rng,
            self.auth_key.party_id,
            self.auth_key.num_parties,
            value,
        )
    }

    /// Generate local authenticated share of specified value.
    fn gen_authenticated_share(&mut self, value: T) -> SpdzShare<T> {
        SpdzShare {
            value: self.gen_raw_share(value),
            mac: self.gen_raw_share(value * self.auth_key.plain_value),
        }
    }

    /// Generate random value and its local authenticated share.
    fn gen_random_authenticated_share(&mut self) -> (SpdzShare<T>, T) {
        let value = T::random(&mut self.rng);
        (self.gen_authenticated_share(value), value)
    }
}

/// Generate local unauthenticated share of specified value.
fn gen_raw_share<T: MpcField>(
    mut rng: &mut impl Rng,
    party_id: usize,
    num_parties: usize,
    value: T,
) -> T {
    let start = T::random(&mut rng);
    let step = T::random(&mut rng);
    let share = arithmetic_progression(start, step, party_id as u64);
    let sum = arithmetic_progression_sum(start, step, num_parties as u64);
    if party_id == 0 {
        share + value - sum
    } else {
        share
    }
}

/// Generate random value and its local unauthenticated share.
fn gen_random_raw_share<T: MpcField>(
    mut rng: &mut impl Rng,
    party_id: usize,
    num_parties: usize,
) -> (T, T) {
    let value = T::random(&mut rng);
    (gen_raw_share(rng, party_id, num_parties, value), value)
}

/// Compute n-th term of linear progression.
fn arithmetic_progression<T: MpcField>(start: T, step: T, n: u64) -> T {
    start + step * T::from(n)
}

/// Compute sum of terms 0..n-1 of linear progression.
fn arithmetic_progression_sum<T: MpcField>(start: T, step: T, n: u64) -> T {
    let sum = if n % 2 == 0 {
        T::from(n / 2) * T::from(n - 1)
    } else {
        T::from(n) * T::from((n - 1) / 2)
    };
    start * T::from(n) + step * sum
}
