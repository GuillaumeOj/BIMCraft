//! The `BIMCraft` model kernel: entities, attribute tables, relationships, IDs and units (spec §5).
//!
//! This crate builds for native targets and `wasm32-unknown-unknown` (ARCH-01) and never touches
//! the platform directly (ARCH-02).

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

use std::fmt;

/// The stable 128-bit GUID every object carries (CORE-01).
///
/// Conversion to and from IFC `GlobalId` comes with the IFC interop crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(u128);

impl EntityId {
    /// Wraps a raw identifier.
    #[must_use]
    pub const fn new(raw: u128) -> Self {
        Self(raw)
    }

    /// Returns the raw identifier.
    #[must_use]
    pub const fn get(self) -> u128 {
        self.0
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
