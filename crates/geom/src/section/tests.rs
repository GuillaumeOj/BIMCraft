//! Shear centres against thin-walled theory and symmetry (ENG-03). Thin-walled values are exact
//! only as the thickness tends to zero, so they are checked to 1 % of the section's depth, the
//! accuracy `docs/dev/adr/0011-shear-centre.md` promises.

use super::mesh::{TARGET_ELEMENTS, mesh};
use super::warping::{self, solve_with};
use super::*;
use crate::Edge;
use crate::profile::tests::mm_profile;

const MM: f64 = 1e-3;

fn circle(radius_mm: f64) -> Vec<Edge> {
    crate::profile::tests::circle(0.0, 0.0, radius_mm * MM)
}

fn assert_near(got: Point2, x_mm: f64, y_mm: f64, tolerance_mm: f64) {
    let (dx, dy) = (got.x / MM - x_mm, got.y / MM - y_mm);
    assert!(
        dx.abs() <= tolerance_mm && dy.abs() <= tolerance_mm,
        "({}, {}) mm is not within {tolerance_mm} mm of ({x_mm}, {y_mm})",
        got.x / MM,
        got.y / MM
    );
}

#[test]
fn channel_matches_thin_walled_theory() {
    // 200 × 80 channel, 5 mm thick, web on the left. Along the midlines: h = 195, b = 77.5, so
    // e = 3 b² / (6 b + h) = 18 018.75 / 660 = 27.30 mm left of the web's midline (x = 2.5).
    let channel = mm_profile(&[
        (0.0, 0.0),
        (80.0, 0.0),
        (80.0, 5.0),
        (5.0, 5.0),
        (5.0, 195.0),
        (80.0, 195.0),
        (80.0, 200.0),
        (0.0, 200.0),
    ]);
    let e = 3.0 * 77.5 * 77.5 / (6.0 * 77.5 + 195.0);
    assert_near(channel.shear_centre().expect("solves"), 2.5 - e, 100.0, 2.0);
}

#[test]
fn angle_shear_centre_is_where_the_legs_meet() {
    // Equal angle 100 × 100 × 5: the legs' midlines meet at (2.5, 2.5).
    let angle = mm_profile(&[
        (0.0, 0.0),
        (100.0, 0.0),
        (100.0, 5.0),
        (5.0, 5.0),
        (5.0, 100.0),
        (0.0, 100.0),
    ]);
    assert_near(angle.shear_centre().expect("solves"), 2.5, 2.5, 1.0);
}

#[test]
fn tee_shear_centre_is_on_the_flange_midline() {
    // Flange 200 × 20 on a 10 × 180 web: the midlines meet at (0, 190).
    let tee = mm_profile(&[
        (-5.0, 0.0),
        (5.0, 0.0),
        (5.0, 180.0),
        (100.0, 180.0),
        (100.0, 200.0),
        (-100.0, 200.0),
        (-100.0, 180.0),
        (-5.0, 180.0),
    ]);
    assert_near(tee.shear_centre().expect("solves"), 0.0, 190.0, 2.0);
}

#[test]
fn a_rectangle_shears_at_its_centroid() {
    let rectangle = mm_profile(&[(0.0, 0.0), (300.0, 0.0), (300.0, 500.0), (0.0, 500.0)]);
    assert_near(
        rectangle.shear_centre().expect("solves"),
        150.0,
        250.0,
        0.05,
    );
}

#[test]
fn a_tube_is_meshed_around_its_hole_and_shears_at_its_centre() {
    // Arcs are faceted, the hole is meshed around, and the mesh covers the faceted profile.
    let tube = Profile::new(circle(100.0), vec![circle(90.0)]).expect("valid profile");
    let faceted = tube.faceted().expect("facets");
    let mesh = mesh(&faceted).expect("meshes");
    let area = mesh.covered_area();
    assert!(((area - faceted.area()) / faceted.area()).abs() < 1e-12);
    #[expect(clippy::cast_precision_loss, reason = "a few thousand triangles")]
    let count = mesh.triangles.len() as f64;
    assert!((0.5 * TARGET_ELEMENTS..2.0 * TARGET_ELEMENTS).contains(&count));
    let shear = warping::shear_centre(&mesh, faceted.centroid()).expect("solves");
    assert_near(shear, 0.0, 0.0, 0.5);
}

#[test]
fn thin_parts_next_to_large_ones_are_meshed_conformingly() {
    // A 500 mm square with a tapered fin 0.5 mm thick: the fin is far thinner than the element
    // size and its faces are split at different places, so pieces must be split again until the
    // triangulation contains them.
    let finned = mm_profile(&[
        (0.0, 0.0),
        (500.0, 0.0),
        (500.0, 250.0),
        (800.0, 250.0),
        (797.0, 250.5),
        (500.0, 250.5),
        (500.0, 500.0),
        (0.0, 500.0),
    ]);
    let mesh = mesh(&finned).expect("meshes");
    assert!(((mesh.covered_area() - finned.area()) / finned.area()).abs() < 1e-12);
    // The fin barely changes the square's shear centre at its centre.
    let shear = warping::shear_centre(&mesh, finned.centroid()).expect("solves");
    assert_near(shear, 250.0, 250.0, 5.0);
}

#[test]
fn failures_are_typed() {
    // A 7 m diagonal strip 0.5 mm thick fills so little of its bounding box that the interior
    // lattice would be too large.
    let strip = mm_profile(&[
        (0.0, 0.0),
        (5_000.0, 5_000.0),
        (5_000.0, 5_000.7),
        (0.0, 0.7),
    ]);
    assert_eq!(strip.shear_centre(), Err(ShearCentreError::MeshTooLarge));
    // A 10 m strip 1 µm thick would need far more nodes than allowed.
    let sliver = mm_profile(&[(0.0, 0.0), (10_000.0, 0.0), (10_000.0, 0.001), (0.0, 0.001)]);
    assert_eq!(sliver.shear_centre(), Err(ShearCentreError::MeshTooLarge));
    // A 10 km radius disc needs more chords than a profile may have.
    let huge = Profile::new(circle(1e7), vec![]).expect("valid profile");
    assert_eq!(
        huge.shear_centre(),
        Err(ShearCentreError::Faceting(ProfileError::TooManyEdges))
    );
    assert_eq!(
        ShearCentreError::MeshTooLarge.to_string(),
        format!("the profile needs a mesh of more than {MAX_MESH_NODES} nodes")
    );
}

#[test]
fn a_solve_cut_short_does_not_converge() {
    let rectangle = mm_profile(&[(0.0, 0.0), (300.0, 0.0), (300.0, 20.0), (0.0, 20.0)]);
    let mesh = mesh(&rectangle).expect("meshes");
    assert_eq!(
        solve_with(&mesh, rectangle.centroid(), 1).map(|_| ()),
        Err(ShearCentreError::NotConverged)
    );
}
