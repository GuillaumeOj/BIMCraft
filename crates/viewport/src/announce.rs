//! Throttling of the live-region announcements: at most one per [`INTERVAL`] frames (500 ms at
//! 60 frames a second), the newest waiting one replacing older ones, except that a snap kind
//! never replaces the outcome of an action; the snap kind only when it changes. Frames, not the
//! clock, so the tests are deterministic (AGENTS.md §5).

use render::SnapKind;

use crate::event::{Announcement, AnnouncementKey};

/// Frames between two announcements.
pub(crate) const INTERVAL: u32 = 30;

#[derive(Debug, Default)]
pub(crate) struct Announcer {
    /// Frames left before the next announcement may go out.
    wait: u32,
    /// The announcement waiting for its frame, and whether it is a snap kind.
    pending: Option<(Announcement, bool)>,
    last_snap: Option<SnapKind>,
}

impl Announcer {
    /// The outcome of an action: out now when the interval has passed, else when it does,
    /// unless a newer one replaces it.
    pub(crate) fn say(&mut self, announcement: Announcement) -> Option<Announcement> {
        self.queue(announcement, false)
    }

    /// Announces the snap kind if it changed; `None` (no snap) is silent but resets it.
    pub(crate) fn snap(&mut self, kind: Option<SnapKind>) -> Option<Announcement> {
        if kind == self.last_snap {
            return None;
        }
        self.last_snap = kind;
        let announcement = Announcement {
            key: AnnouncementKey::of_snap(kind?),
            values: Vec::new(),
        };
        self.queue(announcement, true)
    }

    /// One frame passed: the waiting announcement, when its frame has come.
    pub(crate) fn tick(&mut self) -> Option<Announcement> {
        self.wait = self.wait.saturating_sub(1);
        if self.wait > 0 {
            return None;
        }
        let (announcement, _) = self.pending.take()?;
        self.wait = INTERVAL;
        Some(announcement)
    }

    /// Whether an announcement waits for its frame.
    pub(crate) fn is_waiting(&self) -> bool {
        self.pending.is_some()
    }

    fn queue(&mut self, announcement: Announcement, snap: bool) -> Option<Announcement> {
        if self.wait == 0 {
            self.wait = INTERVAL;
            return Some(announcement);
        }
        // A snap kind never replaces a waiting outcome.
        if !(snap && self.pending.as_ref().is_some_and(|&(_, waiting)| !waiting)) {
            self.pending = Some((announcement, snap));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn say(key: AnnouncementKey) -> Announcement {
        Announcement {
            key,
            values: Vec::new(),
        }
    }

    #[test]
    fn announcements_are_throttled_by_frames_and_the_newest_wins() {
        let mut announcer = Announcer::default();
        let mut out = Vec::new();
        out.extend(announcer.say(say(AnnouncementKey::SelectionCount)));
        assert_eq!(out.len(), 1);
        out.extend(announcer.say(say(AnnouncementKey::ViewPreset)));
        out.extend(announcer.say(say(AnnouncementKey::ViewProjection)));
        assert!(announcer.is_waiting());
        for _ in 1..INTERVAL {
            out.extend(announcer.tick());
        }
        assert_eq!(out.len(), 1);
        out.extend(announcer.tick());
        assert_eq!(out[1].key, AnnouncementKey::ViewProjection);
        assert!(!announcer.is_waiting());
        // Quiet frames reopen the window.
        for _ in 0..INTERVAL {
            out.extend(announcer.tick());
        }
        out.extend(announcer.say(say(AnnouncementKey::SelectionCleared)));
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn the_snap_kind_is_announced_only_when_it_changes() {
        let mut announcer = Announcer::default();
        let mut out = Vec::new();
        out.extend(announcer.snap(Some(SnapKind::Endpoint)));
        out.extend(announcer.snap(Some(SnapKind::Endpoint)));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, AnnouncementKey::SnapEndpoint);
        out.extend(announcer.snap(None));
        assert!(!announcer.is_waiting());
        out.extend(announcer.snap(Some(SnapKind::Endpoint)));
        assert!(announcer.is_waiting());
        // An outcome replaces a waiting snap; a snap does not replace a waiting outcome.
        out.extend(announcer.say(say(AnnouncementKey::MoveDistance)));
        out.extend(announcer.snap(Some(SnapKind::Face)));
        for _ in 0..INTERVAL {
            out.extend(announcer.tick());
        }
        assert_eq!(
            out.last().map(|a| a.key),
            Some(AnnouncementKey::MoveDistance)
        );
    }
}
