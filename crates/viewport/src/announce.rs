//! Throttling of the live-region announcements: at most one per [`INTERVAL`] frames (500 ms at
//! 60 frames a second), the newest waiting one replacing older ones, except that a snap kind
//! never replaces the outcome of an action; the snap kind only when it changes. Frames, not the clock, so the tests are deterministic (AGENTS.md §5).

use render::SnapKind;

use crate::event::{Announcement, AnnouncementKey};

/// Frames between two announcements.
pub(crate) const INTERVAL: u32 = 30;

#[derive(Debug, Default)]
pub(crate) struct Announcer {
    /// Frames left before the next announcement may go out.
    wait: u32,
    pending: Option<Announcement>,
    last_snap: Option<SnapKind>,
}

impl Announcer {
    /// Queues `announcement`: out now when the interval has passed, else when it does, unless a
    /// newer one replaces it.
    pub(crate) fn say(&mut self, announcement: Announcement, out: &mut Vec<Announcement>) {
        if self.wait == 0 {
            out.push(announcement);
            self.wait = INTERVAL;
        } else if !(is_snap(&announcement) && self.pending.as_ref().is_some_and(|p| !is_snap(p))) {
            self.pending = Some(announcement);
        }
    }

    /// Announces the snap kind if it changed; `None` (no snap) is silent but resets it.
    pub(crate) fn snap(&mut self, kind: Option<SnapKind>, out: &mut Vec<Announcement>) {
        if kind == self.last_snap {
            return;
        }
        self.last_snap = kind;
        if let Some(kind) = kind {
            self.say(
                Announcement {
                    key: AnnouncementKey::of_snap(kind),
                    values: Vec::new(),
                },
                out,
            );
        }
    }

    /// One frame passed.
    pub(crate) fn tick(&mut self, out: &mut Vec<Announcement>) {
        self.wait = self.wait.saturating_sub(1);
        if self.wait == 0
            && let Some(announcement) = self.pending.take()
        {
            out.push(announcement);
            self.wait = INTERVAL;
        }
    }

    /// Whether an announcement waits for its frame.
    pub(crate) fn is_waiting(&self) -> bool {
        self.pending.is_some()
    }
}

fn is_snap(announcement: &Announcement) -> bool {
    matches!(
        announcement.key,
        AnnouncementKey::SnapEndpoint
            | AnnouncementKey::SnapMidpoint
            | AnnouncementKey::SnapEdge
            | AnnouncementKey::SnapFace
            | AnnouncementKey::SnapGround
    )
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
        announcer.say(say(AnnouncementKey::SelectionCount), &mut out);
        assert_eq!(out.len(), 1);
        announcer.say(say(AnnouncementKey::ViewPreset), &mut out);
        announcer.say(say(AnnouncementKey::ViewProjection), &mut out);
        assert!(announcer.is_waiting());
        for _ in 1..INTERVAL {
            announcer.tick(&mut out);
        }
        assert_eq!(out.len(), 1);
        announcer.tick(&mut out);
        assert_eq!(out[1].key, AnnouncementKey::ViewProjection);
        assert!(!announcer.is_waiting());
        // Quiet frames reopen the window.
        for _ in 0..INTERVAL {
            announcer.tick(&mut out);
        }
        announcer.say(say(AnnouncementKey::SelectionCleared), &mut out);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn the_snap_kind_is_announced_only_when_it_changes() {
        let mut announcer = Announcer::default();
        let mut out = Vec::new();
        announcer.snap(Some(SnapKind::Endpoint), &mut out);
        announcer.snap(Some(SnapKind::Endpoint), &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, AnnouncementKey::SnapEndpoint);
        announcer.snap(None, &mut out);
        assert!(!announcer.is_waiting());
        announcer.snap(Some(SnapKind::Endpoint), &mut out);
        assert!(announcer.is_waiting());
        // An outcome replaces a waiting snap; a snap does not replace a waiting outcome.
        announcer.say(say(AnnouncementKey::MoveDistance), &mut out);
        announcer.snap(Some(SnapKind::Face), &mut out);
        for _ in 0..INTERVAL {
            announcer.tick(&mut out);
        }
        assert_eq!(
            out.last().map(|a| a.key),
            Some(AnnouncementKey::MoveDistance)
        );
    }
}
