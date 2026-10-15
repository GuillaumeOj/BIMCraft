//! Feeds arbitrary profiles and directions to extrusion (GEO-02). Nothing may panic; a downward
//! direction is always rejected as such, and an accepted extrusion must be a valid solid of one
//! shell with a through hole per profile hole, whose volume is positive and, for a profile of
//! lines only, its area times the height, with one side face per edge.

#![no_main]

use bimcraft_fuzz::{RawProfile, direction, profile_loops};
use geom::{Edge, ExtrusionError, Profile, Solid};
use libfuzzer_sys::fuzz_target;

/// A profile and an extrusion direction in quarter metres.
type Input = (RawProfile, (i8, i8, i8));

fuzz_target!(|input: Input| {
    let (raw, raw_direction) = input;
    let (outer, holes) = profile_loops(&raw);
    let Ok(profile) = Profile::new(outer, holes) else {
        return;
    };
    let direction = direction(raw_direction);
    let result = Solid::extrude(&profile, direction);
    if direction.z <= 0.0 {
        assert_eq!(result.err(), Some(ExtrusionError::NotUpward));
        return;
    }
    let Ok(solid) = result else {
        return;
    };
    assert_eq!(solid.validate(), Ok(()));
    let holes = u32::try_from(profile.holes().len()).expect("at most a few holes");
    match solid.shells() {
        [shell] => assert_eq!(shell.genus(), holes),
        shells => panic!("an extrusion has one shell, not {}", shells.len()),
    }
    let volume = solid.volume();
    assert!(volume.is_finite() && volume > 0.0);
    let edges: Vec<&Edge> = profile
        .outer()
        .iter()
        .chain(profile.holes().flatten())
        .collect();
    if edges.iter().all(|edge| matches!(edge, Edge::Line { .. })) {
        assert_eq!(solid.face_count(), edges.len() + 2);
        let expected = profile.area() * direction.z;
        assert!((volume - expected).abs() <= 1e-9 * expected);
    }
});
