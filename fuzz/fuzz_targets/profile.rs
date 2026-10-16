//! Feeds arbitrary loops of lines and arcs, with holes, to profile validation (GEO-01). Nothing
//! may panic; a rejected profile fails with a typed error, and an accepted one must have a
//! positive finite area, its centroid inside its bounds, and must be accepted again unchanged
//! from its own loops, or from the same loops turning the other way.

#![no_main]

use bimcraft_fuzz::{RawProfile, profile_loops, reversed};
use geom::Profile;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: RawProfile| {
    let (outer, holes) = profile_loops(&input);
    let Ok(profile) = Profile::new(outer.clone(), holes.clone()) else {
        return;
    };
    let area = profile.area();
    assert!(area.is_finite() && area > 0.0);
    let [min_x, min_y, max_x, max_y] = profile.bounds();
    let centroid = profile.centroid();
    assert!((min_x..=max_x).contains(&centroid.x) && (min_y..=max_y).contains(&centroid.y));

    let holes_again = profile.holes().map(<[_]>::to_vec).collect();
    let again = Profile::new(profile.outer().to_vec(), holes_again).expect("a valid profile");
    assert_eq!(again, profile);

    let flipped = Profile::new(
        reversed(&outer),
        holes.iter().map(|h| reversed(h)).collect(),
    )
    .expect("the same region, turning the other way");
    assert!((flipped.area() - area).abs() <= 1e-9 * area);
});
