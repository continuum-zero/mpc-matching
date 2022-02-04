mod mersenne_61 {
    use ff::PrimeField;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Finite field mod 2^61-1.
    #[derive(PrimeField)]
    #[PrimeFieldModulus = "4611686018427387903"]
    #[PrimeFieldGenerator = "37"]
    #[PrimeFieldReprEndianness = "little"]
    pub struct Mersenne61([u64; 1]);

    impl Serialize for Mersenne61 {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            self.to_repr().0.serialize(serializer)
        }
    }

    impl<'de> Deserialize<'de> for Mersenne61 {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let repr = Mersenne61Repr(Deserialize::deserialize(deserializer)?);
            Self::from_repr_vartime(repr)
                .ok_or_else(|| serde::de::Error::custom("Invalid field element"))
        }
    }
}

mod mersenne_127 {
    use ff::PrimeField;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Finite field mod 2^127-1.
    #[derive(PrimeField)]
    #[PrimeFieldModulus = "170141183460469231731687303715884105727"]
    #[PrimeFieldGenerator = "43"]
    #[PrimeFieldReprEndianness = "little"]
    pub struct Mersenne127([u64; 2]);

    impl Serialize for Mersenne127 {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            self.to_repr().0.serialize(serializer)
        }
    }

    impl<'de> Deserialize<'de> for Mersenne127 {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let repr = Mersenne127Repr(Deserialize::deserialize(deserializer)?);
            Self::from_repr_vartime(repr)
                .ok_or_else(|| serde::de::Error::custom("Invalid field element"))
        }
    }
}

pub use mersenne_127::{Mersenne127, Mersenne127Repr};
pub use mersenne_61::{Mersenne61, Mersenne61Repr};
