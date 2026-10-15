//! Where a point of one operand's face lies relative to the other operand.

use std::f64::consts::PI;

use super::face::{PlanarFace, Pool};
use crate::LINEAR_TOLERANCE;
use crate::vector::{Vector, cross, dot, length, sub};

/// Where a piece of face lies relative to the other operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Class {
    Inside,
    Outside,
    /// On a face of the other operand whose outside is on the same side.
    OnSame,
    /// On a face of the other operand whose outside is on the opposite side.
    OnOpposite,
}

/// Classifies a point of `face` against the faces of a solid.
///
/// A point inside a face of the solid coplanar with `face`, within the tolerance, lies on it.
/// Being near a face is not enough: a thin piece of face may run along an edge of the solid
/// without lying on either face there. Otherwise
/// the winding number of the solid's boundary around the point decides: about 1 inside, about 0
/// outside, and far from ambiguous once the point is more than the tolerance from every face.
pub(super) fn classify(p: Vector, face: &PlanarFace, solid: &[PlanarFace], pool: &Pool) -> Class {
    let on = solid.iter().find(|other| {
        other.reaches(p)
            && other.distance(p).abs() <= LINEAR_TOLERANCE
            && other.contains(pool, other.projection.apply(p))
            && face.coplanar(other, pool)
    });
    if let Some(other) = on {
        return if dot(other.normal, face.normal) > 0.0 {
            Class::OnSame
        } else {
            Class::OnOpposite
        };
    }
    if winding(p, solid, pool) > 0.5 {
        Class::Inside
    } else {
        Class::Outside
    }
}

/// The winding number of the faces around a point: their total solid angle over 4π. Each ring
/// is fanned into triangles from its first vertex; the signed angles of a fan add up to the
/// ring's, whatever its shape.
///
/// Faces whose plane passes within the tolerance of the point are left out. Seen from its own
/// plane a face subtends no angle, but the formula's arc tangent would then turn on the sign of
/// a zero and give ±2π per triangle. A point inside such a face was already found on it.
fn winding(p: Vector, solid: &[PlanarFace], pool: &Pool) -> f64 {
    let mut total = 0.0;
    let seen = solid
        .iter()
        .filter(|face| face.distance(p).abs() > LINEAR_TOLERANCE);
    for ring in seen.flat_map(|face| &face.rings) {
        let first = sub(pool.get(ring[0]), p);
        for pair in ring[1..].windows(2) {
            total += solid_angle(first, sub(pool.get(pair[0]), p), sub(pool.get(pair[1]), p));
        }
    }
    total / (4.0 * PI)
}

/// The signed solid angle of a triangle seen from the origin (Van Oosterom and Strackee, 1983).
fn solid_angle(a: Vector, b: Vector, c: Vector) -> f64 {
    let (la, lb, lc) = (length(a), length(b), length(c));
    let numerator = dot(a, cross(b, c));
    let denominator = la * lb * lc + dot(a, b) * lc + dot(a, c) * lb + dot(b, c) * la;
    2.0 * libm::atan2(numerator, denominator)
}
