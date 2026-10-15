//! Properties of the shear centre (STR-14, ENG-02): symmetric sections shear at their centroid,
//! monosymmetric ones on their axis, channels agree with thin-walled theory, and the result
//! moves with the profile. Each case meshes and solves a section, so the case count is small.

use geom::{Edge, Point2, Profile, ProfileError};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

fn polygon(points: &[(f64, f64)], dx: f64, dy: f64) -> Result<Profile, ProfileError> {
    let points: Vec<Point2> = points
        .iter()
        .map(|&(x, y)| Point2::new(x + dx, y + dy))
        .collect();
    let edges = (0..points.len())
        .map(|i| Edge::Line {
            start: points[i],
            end: points[(i + 1) % points.len()],
        })
        .collect();
    Profile::new(edges, vec![])
}

/// An I-section of width `b`, depth `h`, flange thickness `tf` and web thickness `tw`, centred
/// on the origin.
fn i_section(b: f64, h: f64, tf: f64, tw: f64) -> Vec<(f64, f64)> {
    let (half_b, half_h, half_w) = (b / 2.0, h / 2.0, tw / 2.0);
    let (outer, inner) = (half_h, half_h - tf);
    vec![
        (-half_b, -outer),
        (half_b, -outer),
        (half_b, -inner),
        (half_w, -inner),
        (half_w, inner),
        (half_b, inner),
        (half_b, outer),
        (-half_b, outer),
        (-half_b, inner),
        (-half_w, inner),
        (-half_w, -inner),
        (-half_b, -inner),
    ]
}

/// A channel of depth `h`, flange width `b` and uniform thickness `t`, web along x = 0.
fn channel(h: f64, b: f64, t: f64) -> Vec<(f64, f64)> {
    vec![
        (0.0, 0.0),
        (b, 0.0),
        (b, t),
        (t, t),
        (t, h - t),
        (b, h - t),
        (b, h),
        (0.0, h),
    ]
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 8,
        rng_seed: RngSeed::Fixed(11),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn doubly_symmetric_sections_shear_at_their_centroid(
        b in 0.1..0.4f64,
        h in 0.1..0.8f64,
        flange in 0.03..0.15f64,
        web in 0.1..0.9f64,
        (dx, dy) in (-50.0..50.0f64, -50.0..50.0f64),
    ) {
        let tf = flange * h;
        let tw = web * b;
        let profile = polygon(&i_section(b, h, tf, tw), dx, dy).expect("valid profile");
        let shear = profile.shear_centre().expect("solves");
        let tolerance = 0.005 * h.max(b);
        prop_assert!((shear.x - dx).abs() <= tolerance && (shear.y - dy).abs() <= tolerance);
    }

    #[test]
    fn channels_agree_with_thin_walled_theory_and_move_with_the_profile(
        h in 0.1..0.5f64,
        width in 0.25..0.6f64,
        thinness in 0.02..0.05f64,
        (dx, dy) in (-50.0..50.0f64, -50.0..50.0f64),
    ) {
        let b = width * h;
        let t = thinness * h;
        let shape = channel(h, b, t);
        let shear = polygon(&shape, 0.0, 0.0).expect("valid profile").shear_centre().expect("solves");
        // Along the midlines: e = 3 b'² / (6 b' + h') left of the web's midline.
        let (bm, hm) = (b - t / 2.0, h - t);
        let e = 3.0 * bm * bm / (6.0 * bm + hm);
        let tolerance = 0.01 * h;
        prop_assert!((shear.x - (t / 2.0 - e)).abs() <= tolerance, "{shear:?}");
        prop_assert!((shear.y - h / 2.0).abs() <= tolerance, "{shear:?}");
        // The shear centre of a channel lies outside it, beyond the web.
        prop_assert!(shear.x < 0.0);

        let moved = polygon(&shape, dx, dy).expect("valid profile").shear_centre().expect("solves");
        prop_assert!((moved.x - dx - shear.x).abs() <= 1e-6 * h);
        prop_assert!((moved.y - dy - shear.y).abs() <= 1e-6 * h);
    }
}
