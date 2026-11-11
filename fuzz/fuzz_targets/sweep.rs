//! Feeds arbitrary profiles, paths and references to sweeps (GEO-02). Nothing may panic; an
//! accepted sweep must be a valid solid with a positive volume and a single shell with a tunnel
//! per profile hole.

#![no_main]

use bimcraft_fuzz::{RawProfile, direction, profile_loops};
use geom::{Path, PathSegment, Point3, Profile, Solid};
use libfuzzer_sys::fuzz_target;

/// Most path segments per input: enough for corners, arcs and crossings, few enough to keep each
/// input fast.
const MAX_SEGMENTS: usize = 8;

/// A segment: its kind (a line when even, an arc otherwise), then its last point, and an arc's
/// middle point, on the quarter-metre grid.
type RawSegment = (u8, (i8, i8, i8), (i8, i8, i8));

/// A profile, the path's start, its segments and the reference, in quarter metres.
type Input = (RawProfile, (i8, i8, i8), Vec<RawSegment>, (i8, i8, i8));

fn point(raw: (i8, i8, i8)) -> Point3 {
    let v = direction(raw);
    Point3::new(v.x, v.y, v.z)
}

fuzz_target!(|input: Input| {
    let (raw, start, raw_segments, reference) = input;
    let (outer, holes) = profile_loops(&raw);
    let Ok(profile) = Profile::new(outer, holes) else {
        return;
    };
    // Each segment starts where the previous one ends, so most paths are continuous.
    let mut at = point(start);
    let segments = raw_segments
        .iter()
        .take(MAX_SEGMENTS)
        .map(|&(kind, end, through)| {
            let (start, end) = (at, point(end));
            at = end;
            if kind % 2 == 0 {
                PathSegment::Line { start, end }
            } else {
                PathSegment::Arc {
                    start,
                    through: point(through),
                    end,
                }
            }
        })
        .collect();
    let Ok(path) = Path::new(segments) else {
        return;
    };
    let Ok(solid) = Solid::sweep(&profile, &path, direction(reference)) else {
        return;
    };
    assert_eq!(solid.validate(), Ok(()));
    let volume = solid.volume();
    assert!(volume.is_finite() && volume > 0.0);
    match solid.shells() {
        [shell] => assert_eq!(
            shell.genus(),
            u32::try_from(profile.holes().len()).expect("few holes")
        ),
        shells => panic!("a sweep has one shell, not {}", shells.len()),
    }
});
