//! Feeds arbitrary profiles, axes and angles to revolution (GEO-02). Nothing may panic; an angle
//! outside (0, 2π] is always rejected, and an accepted revolution must be a valid solid
//! with a positive volume, whose shells follow from the turn: one shell with a tunnel per hole
//! for a partial turn, and for a full turn at least a ring-shaped void per hole besides the outer
//! shell (a profile touching the axis in two places also encloses a cavity).

#![no_main]

use std::f64::consts::TAU;

use bimcraft_fuzz::{RawProfile, profile_loops};
use geom::{Point2, Profile, RevolutionAxis, RevolutionError, Solid, Vector2};
use libfuzzer_sys::fuzz_target;

/// A profile, an axis (origin and direction in quarter metres), and an angle in 1/32 of a full
/// turn, from −1 to 40 (see `angle` below).
type Input = (RawProfile, (i8, i8, i8, i8), i8);

fuzz_target!(|input: Input| {
    let (raw, (ox, oy, dx, dy), raw_angle) = input;
    let (outer, holes) = profile_loops(&raw);
    let Ok(profile) = Profile::new(outer, holes) else {
        return;
    };
    let quarter = |raw: i8| f64::from(raw) / 4.0;
    let axis = RevolutionAxis {
        origin: Point2::new(quarter(ox), quarter(oy)),
        direction: Vector2::new(quarter(dx), quarter(dy)),
    };
    // A multiple of 1/32 turn from −1 to 40, so that most inputs are valid angles; 32 is exactly
    // a full turn.
    let angle = f64::from(raw_angle.rem_euclid(42) - 1) * TAU / 32.0;
    let result = Solid::revolve(&profile, axis, angle);
    if !(angle > 0.0 && angle <= TAU) {
        // The axis is checked first, so a bad axis may be reported instead.
        assert!(matches!(
            result,
            Err(RevolutionError::AngleOutOfRange | RevolutionError::ZeroAxisDirection)
        ));
        return;
    }
    let Ok(solid) = result else {
        return;
    };
    assert_eq!(solid.validate(), Ok(()));
    let volume = solid.volume();
    assert!(volume.is_finite() && volume > 0.0);
    let holes = profile.holes().len();
    if angle < TAU {
        match solid.shells() {
            [shell] => assert_eq!(shell.genus(), u32::try_from(holes).expect("few holes")),
            shells => panic!("a partial turn has one shell, not {}", shells.len()),
        }
    } else {
        // A full turn: the outer shell, then one void per hole, and one more for each cavity
        // that a profile touching the axis in two places encloses.
        let voids = solid.shells().iter().filter(|shell| shell.is_void()).count();
        assert!(voids >= holes);
        assert_eq!(solid.shells().len(), 1 + voids);
    }
});
