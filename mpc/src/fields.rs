mod mersenne_62 {
    use serde::{Deserialize, Serialize};

    /// Finite field mod 2^61-1.
    #[derive(ff::PrimeField, Serialize, Deserialize)]
    #[PrimeFieldModulus = "4611686018427387903"]
    #[PrimeFieldGenerator = "37"]
    #[PrimeFieldReprEndianness = "little"]
    pub struct Mersenne62([u64; 1]);
}

mod mersenne_127 {
    use serde::{Deserialize, Serialize};

    /// Finite field mod 2^127-1.
    #[derive(ff::PrimeField, Serialize, Deserialize)]
    #[PrimeFieldModulus = "170141183460469231731687303715884105727"]
    #[PrimeFieldGenerator = "43"]
    #[PrimeFieldReprEndianness = "little"]
    pub struct Mersenne127([u64; 2]);
}

pub use mersenne_127::Mersenne127;
pub use mersenne_62::Mersenne62;
