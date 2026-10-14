use std::time::{SystemTime, UNIX_EPOCH};

use platform::{Clock, Timestamp};

/// A [`Clock`] that reads the system's wall clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        timestamp_at(SystemTime::now())
    }
}

/// Converts `time` to a [`Timestamp`]: times before the epoch become 0, and times too far ahead
/// for milliseconds in a `u64` become the largest timestamp.
fn timestamp_at(time: SystemTime) -> Timestamp {
    let millis = time.duration_since(UNIX_EPOCH).map_or(0, |since| {
        u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
    });
    Timestamp::from_unix_millis(millis)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn converts_times_after_the_epoch_to_milliseconds() {
        assert_eq!(timestamp_at(UNIX_EPOCH), Timestamp::from_unix_millis(0));
        let later = UNIX_EPOCH + Duration::from_micros(1_500_900);
        assert_eq!(timestamp_at(later), Timestamp::from_unix_millis(1_500));
    }

    #[test]
    fn clamps_times_before_the_epoch_to_zero() {
        let earlier = UNIX_EPOCH - Duration::from_secs(1);
        assert_eq!(timestamp_at(earlier), Timestamp::from_unix_millis(0));
    }

    /// Only checks that the reading is plausible: the actual value depends on the machine.
    #[test]
    fn system_clock_reads_a_time_after_2020() {
        let start_of_2020 = Timestamp::from_unix_millis(1_577_836_800_000);
        assert!(SystemClock.now() > start_of_2020);
    }
}
