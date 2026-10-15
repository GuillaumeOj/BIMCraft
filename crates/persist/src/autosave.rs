//! Autosave (PERS-02, NFR-06): when to write a recovery copy of the model, and writing it.
//!
//! The recovery copy is a project file of its own, next to the project: autosave never touches
//! the project file, so it only changes, or moves to a newer format, when the user saves. The
//! host owns the timer and the files. It reports every change to an [`Autosave`], runs it when
//! [`Autosave::due_at`] comes, writes the copy with [`autosave_model`], and deletes the copy
//! after a manual save. A copy still there when the project is opened means the last session
//! ended with unsaved work: the host offers to restore it, reading it with
//! [`open_model`](crate::open_model).

use core_model::Store;
use platform::Timestamp;

use crate::model::save_model_and;
use crate::{Connection, PersistError, Schema, property, write_property};

/// Project property holding the time a recovery copy was written, in milliseconds since the
/// Unix epoch.
const AUTOSAVED_AT: &str = "autosaved_at";

/// How long after the first unsaved change the recovery copy is written: whole minutes from
/// [`MIN`](Self::MIN) to [`MAX`](Self::MAX). Autosave cannot be turned off, so a crash loses at
/// most one interval of work (NFR-06).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutosaveInterval(u32);

impl AutosaveInterval {
    /// The shortest interval: one minute.
    pub const MIN: Self = Self(1);
    /// The longest interval: one hour.
    pub const MAX: Self = Self(60);
    /// The interval of a new installation: five minutes.
    pub const DEFAULT: Self = Self(5);

    /// An interval of `minutes`.
    ///
    /// # Errors
    ///
    /// [`IntervalOutOfRange`] outside [`MIN`](Self::MIN)..=[`MAX`](Self::MAX).
    pub const fn from_minutes(minutes: u32) -> Result<Self, IntervalOutOfRange> {
        if minutes >= Self::MIN.0 && minutes <= Self::MAX.0 {
            Ok(Self(minutes))
        } else {
            Err(IntervalOutOfRange)
        }
    }

    /// The interval in milliseconds, the unit of [`Timestamp`].
    #[must_use]
    pub const fn as_millis(self) -> u64 {
        // Lossless: at most 60 minutes.
        self.0 as u64 * 60_000
    }
}

/// An autosave interval outside the accepted range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "the autosave interval must be between {} and {} minutes",
    AutosaveInterval::MIN.0,
    AutosaveInterval::MAX.0
)]
pub struct IntervalOutOfRange;

/// Decides when the recovery copy is due.
///
/// A plain state machine: it never reads a clock, sleeps or spawns. The host passes the current
/// time from its [`Clock`](platform::Clock), so tests drive it with a fake clock.
///
/// Not `Copy`: a copy made by accident would record changes the real scheduler never sees.
#[derive(Clone, Debug)]
pub struct Autosave {
    interval: AutosaveInterval,
    /// Start of the countdown: the first change since the last copy or manual save, or the last
    /// failed attempt. `None` when nothing is waiting to be saved.
    since: Option<Timestamp>,
}

impl Autosave {
    /// A scheduler with nothing to save.
    #[must_use]
    pub const fn new(interval: AutosaveInterval) -> Self {
        Self {
            interval,
            since: None,
        }
    }

    /// Changes the interval; a pending copy is now due one new interval after its countdown
    /// started.
    pub const fn set_interval(&mut self, interval: AutosaveInterval) {
        self.interval = interval;
    }

    /// Records a change to the model at `now`.
    ///
    /// Only the first change since the last copy starts the countdown: later ones never push it
    /// back, so no change waits more than one interval.
    pub fn changed(&mut self, now: Timestamp) {
        self.restart_if_clock_went_back(now);
        self.since.get_or_insert(now);
    }

    /// When the next copy is due, or `None` when there is nothing to save. A host can set one
    /// timer for this time instead of polling.
    #[must_use]
    pub fn due_at(&self) -> Option<Timestamp> {
        self.since
            .map(|since| since.saturating_add_millis(self.interval.as_millis()))
    }

    /// Calls `save` if a copy is due at `now`, and tells whether it did.
    ///
    /// After a successful save nothing is pending. After a failed one the changes stay pending
    /// and the next attempt comes one interval later, so a full disk is not retried in a loop.
    ///
    /// # Errors
    ///
    /// The error of `save`.
    pub fn run<E>(
        &mut self,
        now: Timestamp,
        save: impl FnOnce() -> Result<(), E>,
    ) -> Result<bool, E> {
        self.restart_if_clock_went_back(now);
        if self.due_at().is_none_or(|due| now < due) {
            return Ok(false);
        }
        match save() {
            Ok(()) => {
                self.since = None;
                Ok(true)
            }
            Err(error) => {
                self.since = Some(now);
                Err(error)
            }
        }
    }

    /// Records a manual save: nothing is pending any more. The host deletes the recovery copy.
    pub const fn saved(&mut self) {
        self.since = None;
    }

    /// A clock set back would otherwise delay the copy by as much as it moved: the countdown
    /// starts again from `now` instead.
    fn restart_if_clock_went_back(&mut self, now: Timestamp) {
        self.since = self.since.map(|since| since.min(now));
    }
}

/// Writes `store` into the recovery copy `conn`, with the time `saved_at`, in one transaction:
/// an interrupted autosave leaves the previous copy whole (PERS-02).
///
/// `conn` is a project file of its own, prepared by [`open_project`](crate::open_project); the
/// project file is never written.
///
/// # Errors
///
/// The errors of [`save_model`](crate::save_model).
pub fn autosave_model(
    conn: &mut Connection,
    schema: &Schema<'_>,
    store: &Store,
    saved_at: Timestamp,
) -> Result<(), PersistError> {
    save_model_and(conn, schema, store, |tx| {
        write_property(tx, AUTOSAVED_AT, &saved_at.as_unix_millis().to_string())
    })
}

/// When the recovery copy `conn` was written, or `None` for a file [`autosave_model`] never
/// wrote, such as a project file.
///
/// # Errors
///
/// [`PersistError::Corrupt`] when the stored time is not a number, and the errors of
/// [`property`](crate::property).
pub fn autosaved_at(conn: &Connection) -> Result<Option<Timestamp>, PersistError> {
    property(conn, AUTOSAVED_AT)?
        .map(|value| {
            value
                .parse()
                .map(Timestamp::from_unix_millis)
                .map_err(|_| PersistError::Corrupt)
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use platform::Clock;
    use platform::testing::ManualClock;
    use proptest::prelude::*;

    use super::*;
    use crate::test_support::proptest_config;

    const MINUTE: u64 = 60_000;

    fn at(millis: u64) -> Timestamp {
        Timestamp::from_unix_millis(millis)
    }

    fn minutes(minutes: u32) -> AutosaveInterval {
        AutosaveInterval::from_minutes(minutes).unwrap()
    }

    /// Runs `autosave` with a save that succeeds, and tells whether it saved.
    fn run(autosave: &mut Autosave, clock: &ManualClock) -> bool {
        autosave.run(clock.now(), || Ok::<_, ()>(())).unwrap()
    }

    #[test]
    fn intervals_are_whole_minutes_from_one_to_sixty() {
        assert_eq!(AutosaveInterval::from_minutes(0), Err(IntervalOutOfRange));
        assert_eq!(AutosaveInterval::from_minutes(61), Err(IntervalOutOfRange));
        assert_eq!(minutes(1), AutosaveInterval::MIN);
        assert_eq!(minutes(60), AutosaveInterval::MAX);
        assert_eq!(AutosaveInterval::DEFAULT, minutes(5));
        assert_eq!(AutosaveInterval::MAX.as_millis(), 60 * MINUTE);
        assert_eq!(
            IntervalOutOfRange.to_string(),
            "the autosave interval must be between 1 and 60 minutes"
        );
    }

    #[test]
    fn nothing_is_due_before_the_first_change() {
        let clock = ManualClock::new(at(0));
        let mut autosave = Autosave::new(AutosaveInterval::DEFAULT);
        assert_eq!(autosave.due_at(), None);
        clock.advance(24 * 60 * MINUTE);
        assert!(!run(&mut autosave, &clock));
    }

    #[test]
    fn a_copy_is_due_one_interval_after_the_first_change() {
        let clock = ManualClock::new(at(1_000));
        let mut autosave = Autosave::new(minutes(2));
        autosave.changed(clock.now());
        clock.advance(MINUTE);
        // Later changes do not push the copy back.
        autosave.changed(clock.now());
        assert_eq!(autosave.due_at(), Some(at(1_000 + 2 * MINUTE)));

        clock.set(at(1_000 + 2 * MINUTE - 1));
        assert!(!run(&mut autosave, &clock));
        clock.advance(1);
        assert!(run(&mut autosave, &clock));
        assert_eq!(autosave.due_at(), None);
        assert!(!run(&mut autosave, &clock));
    }

    #[test]
    fn a_failed_copy_is_retried_one_interval_later() {
        let clock = ManualClock::new(at(0));
        let mut autosave = Autosave::new(minutes(1));
        autosave.changed(clock.now());
        clock.advance(MINUTE);
        assert_eq!(
            autosave.run(clock.now(), || Err("disk full")),
            Err("disk full")
        );
        assert_eq!(autosave.due_at(), Some(at(2 * MINUTE)));
        clock.advance(MINUTE - 1);
        assert!(!run(&mut autosave, &clock));
        clock.advance(1);
        assert!(run(&mut autosave, &clock));
    }

    #[test]
    fn a_manual_save_cancels_the_pending_copy() {
        let clock = ManualClock::new(at(0));
        let mut autosave = Autosave::new(minutes(1));
        autosave.changed(clock.now());
        autosave.saved();
        assert_eq!(autosave.due_at(), None);
        clock.advance(MINUTE);
        assert!(!run(&mut autosave, &clock));
    }

    #[test]
    fn a_new_interval_reschedules_the_pending_copy() {
        let mut autosave = Autosave::new(minutes(10));
        autosave.changed(at(0));
        autosave.set_interval(minutes(1));
        assert_eq!(autosave.due_at(), Some(at(MINUTE)));
    }

    #[test]
    fn a_clock_set_back_restarts_the_countdown() {
        let clock = ManualClock::new(at(10 * 60 * MINUTE));
        let mut autosave = Autosave::new(minutes(1));
        autosave.changed(clock.now());
        clock.set(at(0));
        assert!(!run(&mut autosave, &clock));
        assert_eq!(autosave.due_at(), Some(at(MINUTE)));
        clock.advance(MINUTE);
        assert!(run(&mut autosave, &clock));

        clock.set(at(10 * 60 * MINUTE));
        autosave.changed(clock.now());
        clock.set(at(0));
        autosave.changed(clock.now());
        assert_eq!(autosave.due_at(), Some(at(MINUTE)));
    }

    /// Clock moves are in thousandths of the interval, so saves come due whatever its length.
    #[derive(Clone, Debug)]
    enum Step {
        Change,
        Wait(u64),
        SetBack(u64),
        Run { succeeds: bool },
    }

    fn step() -> impl Strategy<Value = Step> {
        prop_oneof![
            Just(Step::Change),
            (0..2_000_u64).prop_map(Step::Wait),
            (0..2_000_u64).prop_map(Step::SetBack),
            any::<bool>().prop_map(|succeeds| Step::Run { succeeds }),
        ]
    }

    proptest! {
        #![proptest_config(proptest_config())]

        /// NFR-06: after every run nothing is overdue, so no change waits longer than one
        /// interval from the start of its countdown, and a copy is only written when something
        /// changed.
        #[test]
        fn no_change_waits_longer_than_one_interval(
            interval in 1..=60_u32,
            steps in proptest::collection::vec(step(), 0..64),
        ) {
            let interval = minutes(interval);
            let scaled = |thousandths: u64| interval.as_millis() * thousandths / 1_000;
            let clock = ManualClock::new(at(24 * 60 * MINUTE));
            let mut autosave = Autosave::new(interval);
            let mut dirty = false;
            for step in steps {
                match step {
                    Step::Change => {
                        autosave.changed(clock.now());
                        dirty = true;
                    }
                    Step::Wait(thousandths) => clock.advance(scaled(thousandths)),
                    Step::SetBack(thousandths) => {
                        let back = scaled(thousandths);
                        clock.set(at(clock.now().as_unix_millis().saturating_sub(back)));
                    }
                    Step::Run { succeeds } => {
                        let mut attempted = false;
                        let result = autosave.run(clock.now(), || {
                            attempted = true;
                            if succeeds { Ok(()) } else { Err(()) }
                        });
                        prop_assert!(!attempted || dirty);
                        prop_assert_eq!(result, match (attempted, succeeds) {
                            (false, _) => Ok(false),
                            (true, true) => Ok(true),
                            (true, false) => Err(()),
                        });
                        dirty &= !(attempted && succeeds);
                        prop_assert!(autosave.due_at().is_none_or(|due| clock.now() < due));
                        prop_assert_eq!(autosave.due_at().is_some(), dirty);
                    }
                }
            }
        }
    }
}
