use std::fmt;

/// Number of characters (all ASCII, so also bytes) in an IFC `GlobalId`.
const GLOBAL_ID_LEN: usize = 22;

/// The IFC base-64 alphabet. It differs from RFC 4648 base 64, so no standard codec applies.
const ALPHABET: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

/// The stable 128-bit GUID every object carries (CORE-01).
///
/// It converts losslessly to and from the 22-character IFC `GlobalId` with
/// [`to_global_id`](Self::to_global_id) and [`from_global_id`](Self::from_global_id).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(u128);

impl EntityId {
    /// Wraps a raw identifier.
    #[must_use]
    pub const fn new(raw: u128) -> Self {
        Self(raw)
    }

    /// Builds an identifier from 128 random bits, setting the RFC 4122 version-4 and variant
    /// bits so it looks like the random UUIDs other IFC tools produce. Every [`IdSource`] that
    /// draws random bits goes through here.
    #[must_use]
    pub const fn from_random_bits(bits: u128) -> Self {
        const VERSION_AND_VARIANT: u128 = (0xF << 76) | (0x3 << 62);
        const VERSION_4_RFC_4122: u128 = (0x4 << 76) | (0x2 << 62);
        Self((bits & !VERSION_AND_VARIANT) | VERSION_4_RFC_4122)
    }

    /// Returns the raw identifier.
    #[must_use]
    pub const fn get(self) -> u128 {
        self.0
    }

    /// Encodes the identifier as an IFC `GlobalId`.
    ///
    /// The first character holds the top 2 bits and each of the other 21 holds 6 bits, most
    /// significant first, which is how IFC fits 128 bits into 22 characters.
    #[must_use]
    pub fn to_global_id(self) -> String {
        (0..GLOBAL_ID_LEN)
            .rev()
            .map(|position| char::from(ALPHABET[((self.0 >> (6 * position)) & 0x3F) as usize]))
            .collect()
    }

    /// Decodes an IFC `GlobalId`.
    ///
    /// # Errors
    ///
    /// Returns a [`GlobalIdError`] when `global_id` is not 22 bytes long, contains a character
    /// outside the IFC alphabet, or encodes a value wider than 128 bits.
    pub fn from_global_id(global_id: &str) -> Result<Self, GlobalIdError> {
        // Every valid GlobalId is 22 ASCII bytes, so the length check reads none of the input.
        let [first, rest @ ..]: &[u8; GLOBAL_ID_LEN] =
            global_id
                .as_bytes()
                .try_into()
                .map_err(|_| GlobalIdError::WrongLength {
                    len: global_id.len(),
                })?;
        // The first character carries only 2 bits; anything larger would overflow 128 bits.
        let first = digit_at(0, *first)?;
        if first > 3 {
            return Err(GlobalIdError::OutOfRange);
        }
        rest.iter()
            .zip(1..)
            .try_fold(u128::from(first), |value, (&byte, index)| {
                Ok((value << 6) | u128::from(digit_at(index, byte)?))
            })
            .map(Self)
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

/// Why a string is not a valid IFC `GlobalId`.
///
/// The messages never repeat the input, which may come from a user's model (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GlobalIdError {
    /// The string is not 22 bytes long. Valid characters are ASCII, so any other byte length
    /// cannot be a `GlobalId`.
    #[error("a GlobalId has 22 ASCII characters, got {len} bytes")]
    WrongLength {
        /// Number of bytes received.
        len: usize,
    },
    /// A character is not in the IFC base-64 alphabet.
    #[error("invalid GlobalId character at byte {index}")]
    InvalidCharacter {
        /// Zero-based byte position of the first invalid character.
        index: usize,
    },
    /// The first character is above `3`, so the value does not fit in 128 bits.
    #[error("GlobalId value does not fit in 128 bits")]
    OutOfRange,
}

/// Creates new identifiers.
///
/// Every new [`EntityId`] comes from a source, so tests can use a seeded source (the
/// `test-support` feature) and stay deterministic while hosts supply real randomness.
pub trait IdSource {
    /// Returns a new identifier.
    fn next_id(&mut self) -> EntityId;
}

/// A deterministic [`IdSource`] for tests: the same seed always yields the same identifiers.
///
/// Identifiers carry the RFC 4122 version-4 and variant bits, like the random UUIDs other IFC
/// tools produce. Not suitable for production: the sequence is predictable, so it only exists in
/// this crate's tests and behind the `test-support` feature, which other crates enable as a
/// dev-dependency.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Debug)]
pub struct SeededIdSource {
    state: u64,
}

#[cfg(any(test, feature = "test-support"))]
impl SeededIdSource {
    /// Creates a source whose sequence is fully determined by `seed`.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// `SplitMix64`: small, dependency-free and well distributed for test data.
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[cfg(any(test, feature = "test-support"))]
impl IdSource for SeededIdSource {
    fn next_id(&mut self) -> EntityId {
        EntityId::from_random_bits(
            (u128::from(self.next_u64()) << 64) | u128::from(self.next_u64()),
        )
    }
}

/// Reverse lookup of [`ALPHABET`] for every byte value, so the alphabet is written only once
/// and a lookup can never go out of bounds.
const DIGITS: [Option<u8>; 256] = {
    let mut digits = [None; 256];
    let mut digit = 0;
    while digit < ALPHABET.len() {
        #[expect(clippy::cast_possible_truncation, reason = "the digit is below 64")]
        {
            digits[ALPHABET[digit] as usize] = Some(digit as u8);
        }
        digit += 1;
    }
    digits
};

fn digit_at(index: usize, byte: u8) -> Result<u8, GlobalIdError> {
    DIGITS[usize::from(byte)].ok_or(GlobalIdError::InvalidCharacter { index })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use proptest::prelude::*;

    use super::*;

    /// UUID and `GlobalId` pairs:
    /// 1. the EXAMPLE in the IFC 4.3 documentation of `IfcGloballyUniqueId` (buildingSMART);
    /// 2. the `IfcProject` of buildingSMART's `Sample-Test-Files`, IFC 4.3.2.0
    ///    `Simple-Scene/Building-Architecture.ifc` (CC BY 4.0), UUID expanded with
    ///    `IfcOpenShell`'s reference `legacy_expand`;
    /// 3. the sample UUID from `IfcOpenShell`'s `test_guid.py`, and an arbitrary pattern, both
    ///    encoded with `IfcOpenShell`'s reference `legacy_compress`;
    /// 4. the extremes.
    const KNOWN_VECTORS: [(u128, &str); 6] = [
        (
            0xf70d_d363_bfe3_495d_84a0_2c02_dcb7_d4d2,
            "3t3TDZl_D9NOIWB0BSjzJI",
        ),
        (
            0x979f_c9ff_61c8_47d8_9280_1319_84bf_cf28,
            "2Ndyd$OSX7s9A04nc4lyye",
        ),
        (
            0x656d_1ed0_85c5_4879_b990_26c1_be57_e3be,
            "1bRHxGXSL8URcG9i6_L_E_",
        ),
        (
            0x0123_4567_89ab_cdef_0123_4567_89ab_cdef,
            "018qLdYQlDxm4ZHMU9gytl",
        ),
        (0, "0000000000000000000000"),
        (u128::MAX, "3$$$$$$$$$$$$$$$$$$$$$"),
    ];

    #[test]
    fn id_round_trips_raw_value() {
        assert_eq!(EntityId::new(7).get(), 7);
    }

    #[test]
    fn id_displays_as_32_hex_digits() {
        assert_eq!(
            EntityId::new(0x2a).to_string(),
            "0000000000000000000000000000002a"
        );
    }

    #[test]
    fn known_vectors_round_trip() {
        for (raw, global_id) in KNOWN_VECTORS {
            assert_eq!(EntityId::new(raw).to_global_id(), global_id);
            assert_eq!(EntityId::from_global_id(global_id), Ok(EntityId::new(raw)));
        }
    }

    #[test]
    fn rejects_wrong_lengths() {
        let huge = "0".repeat(1_000_000);
        for input in [
            "",
            "1bRHxGXSL8URcG9i6_L_E",
            "1bRHxGXSL8URcG9i6_L_E_0",
            huge.as_str(),
        ] {
            assert_eq!(
                EntityId::from_global_id(input),
                Err(GlobalIdError::WrongLength { len: input.len() })
            );
        }
    }

    #[test]
    fn rejects_non_ascii_characters() {
        // 20 ASCII characters plus one 2-byte character: 22 bytes.
        assert_eq!(
            EntityId::from_global_id("1bRHxGXSL8URcG9i6_L_\u{e9}"),
            Err(GlobalIdError::InvalidCharacter { index: 20 })
        );
    }

    #[test]
    fn rejects_characters_outside_the_alphabet() {
        assert_eq!(
            EntityId::from_global_id("1bRHxGXSL8-RcG9i6_L_E_"),
            Err(GlobalIdError::InvalidCharacter { index: 10 })
        );
    }

    #[test]
    fn rejects_values_wider_than_128_bits() {
        for first in ['4', 'A', '$'] {
            let input = format!("{first}{}", "0".repeat(21));
            assert_eq!(
                EntityId::from_global_id(&input),
                Err(GlobalIdError::OutOfRange)
            );
        }
    }

    #[test]
    fn errors_describe_themselves() {
        assert_eq!(
            GlobalIdError::WrongLength { len: 3 }.to_string(),
            "a GlobalId has 22 ASCII characters, got 3 bytes"
        );
        assert_eq!(
            GlobalIdError::InvalidCharacter { index: 4 }.to_string(),
            "invalid GlobalId character at byte 4"
        );
        assert_eq!(
            GlobalIdError::OutOfRange.to_string(),
            "GlobalId value does not fit in 128 bits"
        );
    }

    #[test]
    fn seeded_source_is_deterministic() {
        let first: Vec<_> = ids(&mut SeededIdSource::new(42), 8);
        let second: Vec<_> = ids(&mut SeededIdSource::new(42), 8);
        assert_eq!(first, second);
        assert_ne!(first, ids(&mut SeededIdSource::new(43), 8));
    }

    fn is_version_4(id: EntityId) -> bool {
        (id.get() >> 76) & 0xF == 0x4 && (id.get() >> 62) & 0x3 == 0x2
    }

    #[test]
    fn random_bits_get_version_4_and_variant_bits() {
        assert!(is_version_4(EntityId::from_random_bits(0)));
        assert!(is_version_4(EntityId::from_random_bits(u128::MAX)));
    }

    #[test]
    fn seeded_source_yields_distinct_version_4_ids() {
        let generated = ids(&mut SeededIdSource::new(7), 1_000);
        assert_eq!(generated.iter().collect::<HashSet<_>>().len(), 1_000);
        assert!(generated.into_iter().all(is_version_4));
    }

    fn ids(source: &mut impl IdSource, count: usize) -> Vec<EntityId> {
        (0..count).map(|_| source.next_id()).collect()
    }

    proptest! {
        // A fixed seed keeps runs reproducible; with no persistence, no regression files are
        // written next to the sources.
        #![proptest_config(ProptestConfig {
            rng_seed: prop::test_runner::RngSeed::Fixed(0x00C0_FFEE),
            failure_persistence: None,
            ..ProptestConfig::default()
        })]

        #[test]
        fn decode_inverts_encode(raw in any::<u128>()) {
            let id = EntityId::new(raw);
            prop_assert_eq!(EntityId::from_global_id(&id.to_global_id()), Ok(id));
        }

        #[test]
        fn encode_inverts_decode(global_id in "[0-3][0-9A-Za-z_$]{21}") {
            let id = EntityId::from_global_id(&global_id);
            prop_assert_eq!(id.map(EntityId::to_global_id), Ok(global_id));
        }

        #[test]
        fn decoding_any_string_never_panics(input in "\\PC{0,30}") {
            let _ = EntityId::from_global_id(&input);
        }
    }
}
