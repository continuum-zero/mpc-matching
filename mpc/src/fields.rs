/// Convertion into small integer types by truncating.
pub trait IntoTruncated<T> {
    /// Truncate value and convert it.
    fn into_truncated(&self) -> T;
}

mod mersenne_61 {
    use ff::PrimeField;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::IntoTruncated;

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

    impl IntoTruncated<u64> for Mersenne61 {
        fn into_truncated(&self) -> u64 {
            // ff::PrimeField stores value multiplied by constant.
            // We can invert it by multiplying with element that has representation [1].
            const R2_INV: Mersenne61 = Mersenne61([1]);
            (*self * R2_INV).0[0]
        }
    }

    #[cfg(test)]
    mod tests {
        use crate::fields::IntoTruncated;

        use super::Mersenne61;

        #[test]
        fn serialization() {
            let value = Mersenne61::from(123456789012345678);
            let encoded = bincode::serialize(&value).unwrap();
            let decoded = bincode::deserialize(&encoded).unwrap();
            assert_eq!(value, decoded);
        }

        #[test]
        fn truncation() {
            let int_value = 123456789012345678;
            let field_value = Mersenne61::from(int_value);
            let trunc_value = field_value.into_truncated();
            assert_eq!(trunc_value, int_value);
        }
    }
}

mod mersenne_127 {
    use ff::PrimeField;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::IntoTruncated;

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

    impl IntoTruncated<u64> for Mersenne127 {
        fn into_truncated(&self) -> u64 {
            // ff::PrimeField stores value multiplied by constant.
            // We can invert it by multiplying with element that has representation [1, 0].
            // Lower limb represents the first 64 bits (little endian).
            const R2_INV: Mersenne127 = Mersenne127([1, 0]);
            (*self * R2_INV).0[0]
        }
    }

    #[cfg(test)]
    mod tests {
        use ff::PrimeField;

        use crate::fields::IntoTruncated;

        use super::Mersenne127;

        #[test]
        fn serialization() {
            let value = Mersenne127::from_str_vartime("1234567890123456789012345678901").unwrap();
            let encoded = bincode::serialize(&value).unwrap();
            let decoded = bincode::deserialize(&encoded).unwrap();
            assert_eq!(value, decoded);
        }

        #[test]
        fn truncation() {
            let value = Mersenne127::from_str_vartime("1234567890123456789012345678901").unwrap();
            let trunc_value = value.into_truncated();
            assert_eq!(trunc_value, 11711269222405794869
            );
        }
    }
}

pub use mersenne_127::{Mersenne127, Mersenne127Repr};
pub use mersenne_61::{Mersenne61, Mersenne61Repr};
