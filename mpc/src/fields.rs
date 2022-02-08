/// Prime field that can be used in MPC computation.
pub trait MpcField: ff::PrimeField + IntoTruncated<u64> {
    /// Largest k such that 2^(k+1)-2 doesn't overflow.
    const SAFE_BITS: usize;

    /// Returns preprocessed integer 2^k embedded in field. Panics if k > SAFE_BITS.
    fn power_of_two(k: usize) -> Self;

    /// Returns preprocessed inverse of 2^k. Panics if k > SAFE_BITS.
    fn power_of_two_inverse(k: usize) -> Self;
}

/// Conversion into smaller integer type with truncation.
pub trait IntoTruncated<T> {
    /// Convert with truncation.
    fn into_truncated(&self) -> T;
}

/// Precomputed powers of two and their inverses.
struct PowersOfTwo<T, const N: usize> {
    pub powers: [T; N],
    pub inverses: [T; N],
}

impl<T: ff::Field, const N: usize> PowersOfTwo<T, N> {
    /// Precompute powers of two and their inverses.
    fn precompute() -> Self {
        let mut powers = [T::one(); N];
        let mut inverses = [T::one(); N];
        for i in 1..N {
            powers[i] = powers[i - 1].double();
            inverses[i] = powers[i].invert().unwrap();
        }
        Self { powers, inverses }
    }
}

mod mersenne_61 {
    use ff::PrimeField;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::{IntoTruncated, MpcField, PowersOfTwo};

    /// Finite field mod 2^61-1.
    #[derive(PrimeField)]
    #[PrimeFieldModulus = "4611686018427387903"]
    #[PrimeFieldGenerator = "37"]
    #[PrimeFieldReprEndianness = "little"]
    pub struct Mersenne61([u64; 1]);

    #[static_init::dynamic]
    static POWERS_OF_TWO: PowersOfTwo<Mersenne61, { Mersenne61::SAFE_BITS + 1 }> =
        PowersOfTwo::precompute();

    impl MpcField for Mersenne61 {
        const SAFE_BITS: usize = 60;

        fn power_of_two(k: usize) -> Self {
            POWERS_OF_TWO.powers[k]
        }

        fn power_of_two_inverse(k: usize) -> Self {
            POWERS_OF_TWO.inverses[k]
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

    #[cfg(test)]
    mod tests {
        use crate::fields::IntoTruncated;

        use super::Mersenne61;

        #[test]
        fn test_serialization() {
            let value = Mersenne61::from(123456789012345678);
            let encoded = bincode::serialize(&value).unwrap();
            let decoded = bincode::deserialize(&encoded).unwrap();
            assert_eq!(value, decoded);
        }

        #[test]
        fn test_truncation() {
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

    use super::{IntoTruncated, MpcField, PowersOfTwo};

    /// Finite field mod 2^127-1.
    #[derive(PrimeField)]
    #[PrimeFieldModulus = "170141183460469231731687303715884105727"]
    #[PrimeFieldGenerator = "43"]
    #[PrimeFieldReprEndianness = "little"]
    pub struct Mersenne127([u64; 2]);

    #[static_init::dynamic]
    static POWERS_OF_TWO: PowersOfTwo<Mersenne127, { Mersenne127::SAFE_BITS + 1 }> =
        PowersOfTwo::precompute();

    impl MpcField for Mersenne127 {
        const SAFE_BITS: usize = 126;

        fn power_of_two(k: usize) -> Self {
            POWERS_OF_TWO.powers[k]
        }

        fn power_of_two_inverse(k: usize) -> Self {
            POWERS_OF_TWO.inverses[k]
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

    #[cfg(test)]
    mod tests {
        use ff::PrimeField;

        use crate::fields::IntoTruncated;

        use super::Mersenne127;

        #[test]
        fn test_serialization() {
            let value = Mersenne127::from_str_vartime("1234567890123456789012345678901").unwrap();
            let encoded = bincode::serialize(&value).unwrap();
            let decoded = bincode::deserialize(&encoded).unwrap();
            assert_eq!(value, decoded);
        }

        #[test]
        fn test_truncation() {
            let value = Mersenne127::from_str_vartime("1234567890123456789012345678901").unwrap();
            let trunc_value = value.into_truncated();
            assert_eq!(trunc_value, 11711269222405794869);
        }
    }
}

pub use mersenne_127::{Mersenne127, Mersenne127Repr};
pub use mersenne_61::{Mersenne61, Mersenne61Repr};
