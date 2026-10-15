//! Feeds pairs of extruded profiles to the booleans of the geometry kernel (GEO-03). Unlike the
//! boxes of the `boolean` target, the operands are prisms over any profile: non-convex, with
//! holes, with arcs turned into chords, and slanted. Their vertices lie on a quarter-metre grid,
//! so faces of the two operands coincide, touch and cross along shared lines often. Every
//! operation must succeed, and any solid it returns must be valid: a union holds at least the
//! larger operand and at most both, a difference at most the first operand and at least what the
//! second cannot remove from it, and an intersection at most the smaller operand.

#![no_main]

use bimcraft_fuzz::{RawProfile, direction, profile_loops};
use geom::{Profile, Solid};
use libfuzzer_sys::fuzz_target;

/// The volume a boolean result may differ from its bounds by: rounding of the cut vertices.
const SLACK: f64 = 1e-6;

/// Most faces per operand. Arcs hundreds of metres wide become thousands of chords, and a
/// boolean of such prisms takes seconds, which would starve the fuzzer and brush the time limit
/// of each input; the `extrusion` target covers them.
const MAX_FACES: usize = 200;

/// The prism over a raw profile, or `None` if the profile or the extrusion is rejected or the
/// prism has more than [`MAX_FACES`] faces.
fn prism(raw: &RawProfile, raw_direction: (i8, i8, i8)) -> Option<(Solid, f64)> {
    let (outer, holes) = profile_loops(raw);
    let profile = Profile::new(outer, holes).ok()?;
    let solid = Solid::extrude(&profile, direction(raw_direction)).ok()?;
    if solid.face_count() > MAX_FACES {
        return None;
    }
    let volume = solid.volume();
    Some((solid, volume))
}

/// Two profiles and their extrusion directions in quarter metres.
type Input = (RawProfile, (i8, i8, i8), RawProfile, (i8, i8, i8));

fuzz_target!(|input: Input| {
    let (first, first_direction, second, second_direction) = input;
    let (Some((a, va)), Some((b, vb))) = (
        prism(&first, first_direction),
        prism(&second, second_direction),
    ) else {
        return;
    };
    let slack = SLACK * (va + vb).max(1.0);
    let union = a.union(&b).expect("union").expect("a union is never empty");
    assert_eq!(union.validate(), Ok(()));
    let volume = union.volume();
    assert!(volume <= va + vb + slack && volume >= va.max(vb) - slack);
    let volume = a.difference(&b).expect("difference").map_or(0.0, |cut| {
        assert_eq!(cut.validate(), Ok(()));
        cut.volume()
    });
    assert!(volume <= va + slack && volume >= va - vb - slack);
    let volume = a
        .intersection(&b)
        .expect("intersection")
        .map_or(0.0, |shared| {
            assert_eq!(shared.validate(), Ok(()));
            shared.volume()
        });
    assert!(volume <= va.min(vb) + slack);
});
