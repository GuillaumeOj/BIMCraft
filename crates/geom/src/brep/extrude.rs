//! Solids made by extruding a profile (GEO-02).

use core::ops::Range;

use super::{BrepError, FaceInput, Solid};
use crate::{Edge, Point3, Profile, ProfileError, Vector3};

/// Why a profile cannot be extruded.
///
/// Like [`BrepError`] and [`ProfileError`], no variant carries coordinates (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum ExtrusionError {
    /// A component of the direction is NaN or infinite.
    #[error("the extrusion direction has a component that is not a finite number")]
    NonFiniteDirection,
    /// The direction does not point to the side of the profile plane where z increases.
    #[error("the extrusion direction does not point up out of the profile plane")]
    NotUpward,
    /// Replacing the profile's arcs by chords gives an invalid profile.
    #[error("the profile's arcs cannot be replaced by chords")]
    Faceting(#[from] ProfileError),
    /// The extruded faces do not bound a valid solid: too thin, or outside the model extent.
    #[error("the extruded solid is invalid")]
    Solid(#[from] BrepError),
}

impl Solid {
    /// Extrudes a profile lying in the plane z = 0 along `direction` into a prism: a bottom
    /// face, a top face moved by `direction`, and one side face per profile edge. Holes in the
    /// profile become holes through the solid.
    ///
    /// The direction may be oblique but must have a positive z component. Arcs are first
    /// replaced by chords within [`ARC_CHORD_TOLERANCE`](crate::ARC_CHORD_TOLERANCE), since a
    /// solid has planar faces only (`docs/dev/adr/0008-arc-faceting.md`). The volume is the
    /// faceted profile's area times the z component of `direction`.
    ///
    /// # Errors
    ///
    /// [`ExtrusionError`] when the direction is not finite or not upward, when faceting the arcs
    /// gives an invalid profile, or when the result is not a valid solid, for instance no thicker
    /// than [`LINEAR_TOLERANCE`](crate::LINEAR_TOLERANCE) or outside the model extent.
    pub fn extrude(profile: &Profile, direction: Vector3) -> Result<Self, ExtrusionError> {
        if ![direction.x, direction.y, direction.z]
            .iter()
            .all(|c| c.is_finite())
        {
            return Err(ExtrusionError::NonFiniteDirection);
        }
        if direction.z <= 0.0 {
            return Err(ExtrusionError::NotUpward);
        }
        let profile = profile.faceted()?;
        // Every loop vertex once at the bottom, then again at the top, `top` indices later.
        let loops: Vec<&[Edge]> = core::iter::once(profile.outer())
            .chain(profile.holes())
            .collect();
        let starts = || loops.iter().flat_map(|edges| edges.iter().map(Edge::start));
        let mut vertices: Vec<Point3> = starts().map(|p| Point3::new(p.x, p.y, 0.0)).collect();
        let top = vertices.len();
        vertices.extend(
            starts().map(|p| Point3::new(p.x + direction.x, p.y + direction.y, direction.z)),
        );
        let mut rings: Vec<Range<usize>> = Vec::with_capacity(loops.len());
        for edges in &loops {
            let first = rings.last().map_or(0, |ring| ring.end);
            rings.push(first..first + edges.len());
        }
        Ok(Self::from_faces(vertices, &prism_faces(&rings, top))?)
    }
}

/// The faces of a prism over `rings` (the outer ring counterclockwise from above, then the holes
/// clockwise), whose top copy of vertex `v` is `v + top`. Each face is counterclockwise seen
/// from outside, its holes clockwise.
fn prism_faces(rings: &[Range<usize>], top: usize) -> Vec<FaceInput> {
    let reversed = |ring: &Range<usize>| ring.clone().rev().collect();
    let raised = |ring: &Range<usize>| ring.clone().map(|v| v + top).collect();
    let mut faces = vec![
        FaceInput {
            outer: reversed(&rings[0]),
            holes: rings[1..].iter().map(reversed).collect(),
        },
        FaceInput {
            outer: raised(&rings[0]),
            holes: rings[1..].iter().map(raised).collect(),
        },
    ];
    // Along each edge a → b, material lies on the left, so a, b, b', a' turns counterclockwise
    // seen from the right, outside: ((b − a) × d) · right(b − a) = d.z |b − a|² > 0.
    for ring in rings {
        for a in ring.clone() {
            let b = if a + 1 == ring.end { ring.start } else { a + 1 };
            faces.push(FaceInput {
                outer: vec![a, b, b + top, a + top],
                holes: Vec::new(),
            });
        }
    }
    faces
}

#[cfg(test)]
mod tests {
    use core::f64::consts::PI;

    use super::*;
    use crate::profile::tests::{assert_close, circle, p, polygon, rectangle};
    use crate::{ArcDirection, MODEL_EXTENT};

    fn up(z: f64) -> Vector3 {
        Vector3::new(0.0, 0.0, z)
    }

    fn extrude(outer: Vec<Edge>, holes: Vec<Vec<Edge>>, direction: Vector3) -> Solid {
        Solid::extrude(&Profile::new(outer, holes).unwrap(), direction).unwrap()
    }

    /// Topology and volume against hand counts, and the rebuild from its faces.
    fn assert_prism(solid: &Solid, faces: usize, edges: usize, genus: u32, volume: f64) {
        assert_eq!(solid.face_count(), faces);
        assert_eq!(solid.edge_count(), edges);
        assert_eq!(solid.shells().len(), 1);
        assert_eq!(solid.shells()[0].genus(), genus);
        assert_close(solid.volume(), volume);
        let rebuilt = Solid::from_faces(solid.vertices().to_vec(), &solid.to_faces()).unwrap();
        assert_eq!(&rebuilt, solid);
    }

    #[test]
    fn a_rectangle_gives_a_box() {
        // 2 × 3 × 4 m: 6 faces, 12 edges, 8 vertices, 24 m³.
        let solid = extrude(rectangle(0.0, 0.0, 2.0, 3.0), vec![], up(4.0));
        assert_eq!(solid.vertices().len(), 8);
        assert_prism(&solid, 6, 12, 0, 24.0);
        let heights: Vec<f64> = solid.vertices().iter().map(|v| v.z).collect();
        assert_eq!(heights, [0.0, 0.0, 0.0, 0.0, 4.0, 4.0, 4.0, 4.0]);
    }

    #[test]
    fn an_l_shape_has_one_side_face_per_edge() {
        // 3 × 3 m minus a 2 × 2 m corner: 5 m², 6 edges, so 8 faces and 18 edges; 2.5 m high.
        let outer = polygon(&[
            (0.0, 0.0),
            (3.0, 0.0),
            (3.0, 1.0),
            (1.0, 1.0),
            (1.0, 3.0),
            (0.0, 3.0),
        ]);
        assert_prism(&extrude(outer, vec![], up(2.5)), 8, 18, 0, 12.5);
    }

    #[test]
    fn holes_go_through_the_solid() {
        // 3 × 3 m slab with a 1 × 1 m hole, 1 m thick: 8 m³, 2 + 4 + 4 faces, genus 1.
        let hole = rectangle(1.0, 1.0, 2.0, 2.0);
        let solid = extrude(rectangle(0.0, 0.0, 3.0, 3.0), vec![hole], up(1.0));
        assert_prism(&solid, 10, 24, 1, 8.0);
        assert_eq!(solid.to_faces()[0].holes.len(), 1);
        // Two holes: 9 − 2 = 7 m², 14 faces, genus 2.
        let holes = vec![
            rectangle(0.5, 0.5, 1.5, 1.5),
            rectangle(1.5 + 1e-3, 1.5 + 1e-3, 2.5, 2.5),
        ];
        let solid = extrude(rectangle(0.0, 0.0, 3.0, 3.0), holes, up(0.5));
        assert_prism(&solid, 14, 36, 2, 0.5 * (9.0 - 1.0 - 0.999 * 0.999));
    }

    #[test]
    fn oblique_directions_keep_area_times_height() {
        // A parallelepiped: unit square base, sheared by (1, 2) over 3 m: 1 × 3 = 3 m³.
        let solid = extrude(
            rectangle(0.0, 0.0, 1.0, 1.0),
            vec![],
            Vector3::new(1.0, 2.0, 3.0),
        );
        assert_prism(&solid, 6, 12, 0, 3.0);
        assert_eq!(solid.vertices()[4], Point3::new(1.0, 2.0, 3.0));
        let leaning = Vector3::new(-5.0, 0.0, 0.1);
        let solid = extrude(rectangle(0.0, 0.0, 2.0, 1.0), vec![], leaning);
        assert_close(solid.volume(), 0.2);
    }

    #[test]
    fn a_round_column_is_a_faceted_prism() {
        // r = 0.5 m: 50 chords (ADR 0008), so 52 faces; volume of the inscribed 50-gon × 3 m.
        let solid = extrude(circle(0.0, 0.0, 0.5), vec![], up(3.0));
        let polygon_area = 25.0 * 0.25 * libm::sin(2.0 * PI / 50.0);
        assert_prism(&solid, 52, 150, 0, polygon_area * 3.0);
        // Within the ADR bound of the true cylinder: arc length × tolerance × height.
        let cylinder = PI * 0.25 * 3.0;
        assert!(solid.volume() < cylinder);
        assert!(cylinder - solid.volume() <= PI * crate::ARC_CHORD_TOLERANCE * 3.0);
    }

    #[test]
    fn a_round_hole_is_faceted_too() {
        // r = 0.25 m: 36 chords; 2 + 4 + 36 faces.
        let solid = extrude(
            rectangle(-1.0, -1.0, 1.0, 1.0),
            vec![circle(0.0, 0.0, 0.25)],
            up(0.2),
        );
        let hole_area = 18.0 * 0.0625 * libm::sin(2.0 * PI / 36.0);
        assert_prism(&solid, 42, 120, 1, (4.0 - hole_area) * 0.2);
    }

    #[test]
    fn a_half_disc_mixes_lines_and_chords() {
        // r = 1 m half disc: one line and 36 chords, so 39 faces.
        let outer = vec![
            Edge::Arc {
                start: p(1.0, 0.0),
                end: p(-1.0, 0.0),
                centre: p(0.0, 0.0),
                direction: ArcDirection::CounterClockwise,
            },
            Edge::Line {
                start: p(-1.0, 0.0),
                end: p(1.0, 0.0),
            },
        ];
        let solid = extrude(outer, vec![], up(1.0));
        let area = 18.0 * libm::sin(PI / 36.0);
        assert_prism(&solid, 39, 111, 0, area);
    }

    #[test]
    fn directions_must_be_finite_and_upward() {
        let profile = Profile::new(rectangle(0.0, 0.0, 1.0, 1.0), vec![]).unwrap();
        let cases = [
            (
                Vector3::new(f64::NAN, 0.0, 1.0),
                ExtrusionError::NonFiniteDirection,
            ),
            (
                Vector3::new(0.0, f64::INFINITY, 1.0),
                ExtrusionError::NonFiniteDirection,
            ),
            (up(f64::INFINITY), ExtrusionError::NonFiniteDirection),
            (up(0.0), ExtrusionError::NotUpward),
            (up(-1.0), ExtrusionError::NotUpward),
            (Vector3::new(1.0, 0.0, 0.0), ExtrusionError::NotUpward),
        ];
        for (direction, error) in cases {
            assert_eq!(Solid::extrude(&profile, direction), Err(error));
        }
    }

    #[test]
    fn invalid_results_report_the_solid_error() {
        let profile = Profile::new(rectangle(0.0, 0.0, 1.0, 1.0), vec![]).unwrap();
        // A subnormal shift moves the top corners at x = 0 out of the predicates' range.
        assert_eq!(
            Solid::extrude(&profile, Vector3::new(1e-310, 0.0, 1.0)),
            Err(ExtrusionError::Solid(BrepError::OutsideModelExtent {
                vertex: 4
            }))
        );
        assert_eq!(
            Solid::extrude(&profile, up(2.0 * MODEL_EXTENT)),
            Err(ExtrusionError::Solid(BrepError::OutsideModelExtent {
                vertex: 4
            }))
        );
        assert!(matches!(
            Solid::extrude(&profile, up(1e-9)),
            Err(ExtrusionError::Solid(
                BrepError::ShortEdge { .. } | BrepError::DegenerateFace { .. }
            ))
        ));
    }

    #[test]
    fn faceting_errors_are_reported() {
        // Two 1 km circles need more chords than a profile may have.
        let profile = Profile::new(circle(0.0, 0.0, 1e3), vec![circle(0.0, 0.0, 999.0)]).unwrap();
        assert_eq!(
            Solid::extrude(&profile, up(1.0)),
            Err(ExtrusionError::Faceting(ProfileError::TooManyEdges))
        );
    }

    #[test]
    fn errors_describe_their_cause() {
        use std::error::Error as _;

        let error = ExtrusionError::Faceting(ProfileError::TooManyEdges);
        assert_eq!(
            error.to_string(),
            "the profile's arcs cannot be replaced by chords"
        );
        let source = error.source().map(ToString::to_string);
        assert_eq!(
            source.as_deref(),
            Some("the profile has more than 4096 edges")
        );
        let error = ExtrusionError::Solid(BrepError::Empty);
        assert_eq!(error.to_string(), "the extruded solid is invalid");
        assert_eq!(
            error.source().map(ToString::to_string).as_deref(),
            Some("the solid has no faces")
        );
    }
}
