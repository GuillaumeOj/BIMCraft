//! Feeds pairs of extruded profiles to the booleans of the geometry kernel (GEO-03). Unlike the
//! boxes of the `boolean` target, the operands are prisms over any profile: non-convex, with
//! holes, with arcs turned into chords, and slanted. Their vertices lie on a quarter-metre grid,
//! so faces of the two operands coincide, touch and cross along shared lines often.
//!
//! Every operation must give a valid solid, or refuse the operands with the one typed error
//! valid operands may get, `FeaturesTooClose` (ADR 0001, merge pass); any other error fails.
//! A result's volume must lie within its bounds give or take the tolerance times the operands'
//! area, what the merge pass may move: a union holds at least the larger operand and at most
//! both, a difference at most the first operand and at least what the second cannot remove from
//! it, and an intersection at most the smaller operand. So that a wrong solid of about the right
//! volume fails too, vertices must lie where the operation puts them: every operand vertex
//! inside or on the union, every vertex of the difference inside or on the first operand and not
//! inside the second, every vertex of the intersection inside or on both. "On" is within the
//! tolerance, or within the reach of a vertex merge, which may move a vertex across a group up to
//! ten tolerances wide along each axis.

#![no_main]

use bimcraft_fuzz::{
    RawProfile, all_located, direction, profile_loops, unless_too_close, volume_slack,
};
use geom::{Containment, Profile, Solid};
use libfuzzer_sys::fuzz_target;

/// Most faces per operand. Arcs hundreds of metres wide become thousands of chords, and a
/// boolean of such prisms takes seconds, which would starve the fuzzer and brush the time limit
/// of each input; the `extrusion` target covers them.
const MAX_FACES: usize = 200;

/// The prism over a raw profile, or `None` if the profile or the extrusion is rejected or the
/// prism has more than [`MAX_FACES`] faces.
fn prism(raw: &RawProfile, raw_direction: (i8, i8, i8)) -> Option<Solid> {
    let (outer, holes) = profile_loops(raw);
    let profile = Profile::new(outer, holes).ok()?;
    let solid = Solid::extrude(&profile, direction(raw_direction)).ok()?;
    (solid.face_count() <= MAX_FACES).then_some(solid)
}

/// Two profiles and their extrusion directions in quarter metres.
type Input = (RawProfile, (i8, i8, i8), RawProfile, (i8, i8, i8));

fuzz_target!(|input: Input| {
    let (first, first_direction, second, second_direction) = input;
    let (Some(a), Some(b)) = (
        prism(&first, first_direction),
        prism(&second, second_direction),
    ) else {
        return;
    };
    let (va, vb) = (a.volume(), b.volume());
    let slack = volume_slack(&a, &b);
    let not_outside = |c| c != Containment::Outside;
    if let Some(union) = unless_too_close(a.union(&b), "union") {
        let union = union.expect("a union is never empty");
        assert_eq!(union.validate(), Ok(()));
        let volume = union.volume();
        assert!(volume <= va + vb + slack && volume >= va.max(vb) - slack);
        assert!(all_located(a.vertices(), &union, not_outside));
        assert!(all_located(b.vertices(), &union, not_outside));
    }
    if let Some(cut) = unless_too_close(a.difference(&b), "difference") {
        let volume = cut.map_or(0.0, |cut| {
            assert_eq!(cut.validate(), Ok(()));
            assert!(all_located(cut.vertices(), &a, not_outside));
            assert!(all_located(cut.vertices(), &b, |c| c != Containment::Inside));
            cut.volume()
        });
        assert!(volume <= va + slack && volume >= va - vb - slack);
    }
    if let Some(shared) = unless_too_close(a.intersection(&b), "intersection") {
        let volume = shared.map_or(0.0, |shared| {
            assert_eq!(shared.validate(), Ok(()));
            assert!(all_located(shared.vertices(), &a, not_outside));
            assert!(all_located(shared.vertices(), &b, not_outside));
            shared.volume()
        });
        assert!(volume <= va.min(vb) + slack);
    }
});
