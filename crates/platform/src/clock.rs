/// A wall-clock instant, in milliseconds since the Unix epoch (UTC).
///
/// The core cannot use `std::time::Instant` (ARCH-02, and it panics on `wasm32-unknown-unknown`),
/// so time reaches it as plain values from a [`Clock`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(u64);

impl Timestamp {
    /// Creates a timestamp from milliseconds since the Unix epoch.
    #[must_use]
    pub const fn from_unix_millis(millis: u64) -> Self {
        Self(millis)
    }

    /// Returns the milliseconds since the Unix epoch.
    #[must_use]
    pub const fn as_unix_millis(self) -> u64 {
        self.0
    }

    /// The time `millis` later, stopping at the largest representable time.
    #[must_use]
    pub const fn saturating_add_millis(self, millis: u64) -> Self {
        Self(self.0.saturating_add(millis))
    }
}

/// Source of the current wall-clock time.
pub trait Clock {
    /// Returns the current time.
    fn now(&self) -> Timestamp;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_round_trips_millis() {
        assert_eq!(Timestamp::from_unix_millis(1_234).as_unix_millis(), 1_234);
    }

    #[test]
    fn adding_stops_at_the_largest_time() {
        assert_eq!(
            Timestamp::from_unix_millis(1).saturating_add_millis(2),
            Timestamp::from_unix_millis(3)
        );
        assert_eq!(
            Timestamp::from_unix_millis(u64::MAX - 1).saturating_add_millis(5),
            Timestamp::from_unix_millis(u64::MAX)
        );
    }
}
