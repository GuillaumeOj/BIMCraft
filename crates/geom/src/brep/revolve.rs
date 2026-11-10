//! Solids made by revolving a profile around an axis (GEO-02).
//!
//! Rationale and constants: `docs/dev/adr/0020-sweeps-and-revolutions.md`.

use core::f64::consts::{FRAC_PI_2, PI, TAU};
use core::ops::Range;

use super::{BrepError, FaceInput, Solid};
use crate::profile::step_count;
use crate::tolerance::WITHIN_TOLERANCE;
use crate::{Edge, MODEL_EXTENT, Point2, Point3, Profile, ProfileError, Vector2};

/// Largest number of faces a revolved solid may have.
///
/// Counting happens before any face is built, so hostile input cannot make a revolution allocate
/// millions of faces. A profile of at most 64 edges still turns a full circle of radius 10 km.
pub const MAX_SOLID_FACES: usize = 65_536;

/// The axis of a revolution: a line of the profile plane z = 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RevolutionAxis {
    /// A point of the axis.
    pub origin: Point2,
    /// The direction of the axis; it need not be a unit vector but must not be zero. A positive
    /// angle turns by the right-hand rule around it, seen from the direction's tip.
    pub direction: Vector2,
}

/// Why a profile cannot be revolved.
///
/// Like [`BrepError`] and [`ProfileError`], no variant carries coordinates (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum RevolutionError {
    /// A coordinate of the axis is NaN or infinite.
    #[error("the revolution axis has a coordinate that is not a finite number")]
    NonFiniteAxis,
    /// The axis origin is outside the model extent.
    #[error("the revolution axis starts outside the model extent")]
    AxisOutsideExtent,
    /// The axis direction is the zero vector.
    #[error("the revolution axis has no direction")]
    ZeroAxisDirection,
    /// The angle is NaN, not positive, or more than a full turn.
    #[error("the revolution angle is not in the range from 0 (excluded) to a full turn")]
    AngleOutOfRange,
    /// The profile has points on both sides of the axis.
    #[error("the profile crosses the revolution axis")]
    ProfileCrossesAxis,
    /// In a full turn, a vertex on the axis whose two edges both leave it: the solid would be two
    /// cones touching at their tips.
    #[error("a full turn would pinch the solid to a point on the axis")]
    PinchedOnAxis,
    /// The angle leaves the two end faces closer than the linear tolerance.
    #[error("the angle is so close to a full turn that the end faces would almost touch")]
    NearlyFullTurn,
    /// The solid would have more than [`MAX_SOLID_FACES`] faces.
    #[error("the revolved solid would have too many faces")]
    TooManyFaces,
    /// Replacing the profile's arcs by chords gives an invalid profile.
    #[error("the profile's arcs cannot be replaced by chords")]
    Faceting(#[from] ProfileError),
    /// The revolved faces do not bound a valid solid: too thin, or outside the model extent.
    #[error("the revolved solid is invalid")]
    Solid(#[from] BrepError),
}

/// A profile vertex seen from the axis.
#[derive(Clone, Copy)]
struct Vertex {
    point: Point2,
    /// Distance along the axis from its origin.
    along: f64,
    /// Signed distance from the axis, positive on its left; exactly 0 for a vertex on the axis.
    across: f64,
}

impl Vertex {
    const fn on_axis(&self) -> bool {
        self.across == 0.0
    }
}

/// The axis as an origin and a unit direction.
struct Frame {
    origin: Point2,
    unit: Vector2,
}

impl Frame {
    fn new(axis: RevolutionAxis) -> Result<Self, RevolutionError> {
        let RevolutionAxis { origin, direction } = axis;
        if ![origin.x, origin.y, direction.x, direction.y]
            .iter()
            .all(|c| c.is_finite())
        {
            return Err(RevolutionError::NonFiniteAxis);
        }
        if origin.x.abs().max(origin.y.abs()) > MODEL_EXTENT {
            return Err(RevolutionError::AxisOutsideExtent);
        }
        // Scaled first, so that the length of a huge or subnormal direction neither overflows
        // nor underflows.
        let scale = direction.x.abs().max(direction.y.abs());
        if scale == 0.0 {
            return Err(RevolutionError::ZeroAxisDirection);
        }
        let (x, y) = (direction.x / scale, direction.y / scale);
        let length = libm::hypot(x, y);
        Ok(Self {
            origin,
            unit: Vector2::new(x / length, y / length),
        })
    }

    fn vertex(&self, point: Point2) -> Vertex {
        let (dx, dy) = (point.x - self.origin.x, point.y - self.origin.y);
        let across = self.unit.x.mul_add(dy, -self.unit.y * dx);
        Vertex {
            point,
            along: self.unit.x.mul_add(dx, self.unit.y * dy),
            // Within the tolerance of the axis, a vertex is on it (ADR 0001).
            across: if across.abs() <= WITHIN_TOLERANCE {
                0.0
            } else {
                across
            },
        }
    }

    /// The vertex turned by `phi` radians around the axis, right-handed. The unit vector to the
    /// left of the axis, (−uy, ux, 0), goes to cos φ (−uy, ux, 0) + sin φ (0, 0, 1).
    fn turned(&self, vertex: &Vertex, phi: f64) -> Point3 {
        if phi == 0.0 && !vertex.on_axis() {
            // The start section is the input profile, bit for bit.
            return Point3::new(vertex.point.x, vertex.point.y, 0.0);
        }
        let radial = vertex.across * libm::cos(phi);
        let (u, o) = (self.unit, self.origin);
        Point3::new(
            u.x.mul_add(vertex.along, o.x) - u.y * radial,
            u.y.mul_add(vertex.along, o.y) + u.x * radial,
            vertex.across * libm::sin(phi),
        )
    }
}

impl Solid {
    /// Revolves a profile lying in the plane z = 0 around `axis`, also in that plane, by `angle`
    /// radians, 0 < `angle` ≤ 2π, turning by the right-hand rule around the axis direction. At
    /// angle 0 the solid's start face is the profile itself.
    ///
    /// The profile must lie on one side of the axis, touching it allowed: a vertex within
    /// [`LINEAR_TOLERANCE`](crate::LINEAR_TOLERANCE) of the axis is one vertex of the solid, and
    /// an edge along the axis gives no face (cones, solid cylinders, spheres). A partial turn has
    /// two end faces and one shell, with a hole through it per profile hole. A full turn
    /// (`angle` exactly 2π) has no end faces: a profile away from the axis gives a ring, and each
    /// profile hole a void inside it.
    ///
    /// Arcs of the profile are first replaced by chords, and the circle each point travels on by
    /// steps of at most 90°, each within [`ARC_CHORD_TOLERANCE`](crate::ARC_CHORD_TOLERANCE) of
    /// the exact shape for the farthest point from the axis, since a solid has planar faces only
    /// (`docs/dev/adr/0020-sweeps-and-revolutions.md`). The volume is that of the faceted shape,
    /// n · sin(θ/n) · A · r̄ for n steps, with A and r̄ the area and the centroid's distance to
    /// the axis of the faceted profile.
    ///
    /// # Errors
    ///
    /// [`RevolutionError`] when the axis or the angle is invalid, when the profile crosses the
    /// axis, when a full turn would pinch the solid on the axis, when the angle leaves the end
    /// faces nearly touching, when the solid would have more than [`MAX_SOLID_FACES`] faces,
    /// when faceting the arcs gives an invalid profile, or when the result is not a valid solid.
    pub fn revolve(
        profile: &Profile,
        axis: RevolutionAxis,
        angle: f64,
    ) -> Result<Self, RevolutionError> {
        let frame = Frame::new(axis)?;
        // Written so that NaN fails.
        if !(angle > 0.0 && angle <= TAU) {
            return Err(RevolutionError::AngleOutOfRange);
        }
        // At most 2π here, so "at least" is "exactly".
        let full = angle >= TAU;
        let profile = profile.faceted()?;
        let loops: Vec<&[Edge]> = core::iter::once(profile.outer())
            .chain(profile.holes())
            .collect();
        let vertices: Vec<Vertex> = loops
            .iter()
            .flat_map(|edges| edges.iter().map(|edge| frame.vertex(edge.start())))
            .collect();
        let mut rings: Vec<Range<usize>> = Vec::with_capacity(loops.len());
        for edges in &loops {
            let first = rings.last().map_or(0, |ring| ring.end);
            rings.push(first..first + edges.len());
        }
        let flip = side_of_axis(&vertices)?;
        let radius = vertices.iter().fold(0.0, |r: f64, v| r.max(v.across.abs()));
        if full {
            check_not_pinched(&vertices, &rings)?;
        } else if angle > PI && 2.0 * radius * libm::sin((TAU - angle) / 2.0) <= WITHIN_TOLERANCE {
            return Err(RevolutionError::NearlyFullTurn);
        }
        let steps = step_count(radius, angle, FRAC_PI_2, 1.0);
        let steps = bounded_steps(steps, &vertices, &rings, full)?;
        let sections = if full { steps } else { steps + 1 };
        let layout = Layout::new(&vertices, &rings, sections, full);
        let points = layout.points(&frame, &vertices, steps, angle);
        let faces = layout.faces(&vertices, &rings, steps, flip, full);
        Ok(Self::from_faces(points, &faces)?)
    }
}

/// Whether the profile lies to the right of the axis, which reverses every face; an error when
/// it has vertices on both sides.
fn side_of_axis(vertices: &[Vertex]) -> Result<bool, RevolutionError> {
    let mut right = None;
    for vertex in vertices.iter().filter(|v| !v.on_axis()) {
        let is_right = vertex.across < 0.0;
        if *right.get_or_insert(is_right) != is_right {
            return Err(RevolutionError::ProfileCrossesAxis);
        }
    }
    Ok(right == Some(true))
}

/// The vertex before and after `v` in its ring.
fn neighbours(ring: &Range<usize>, v: usize) -> (usize, usize) {
    let before = if v == ring.start { ring.end - 1 } else { v - 1 };
    let after = if v + 1 == ring.end { ring.start } else { v + 1 };
    (before, after)
}

/// In a full turn, a vertex on the axis needs an edge along the axis next to it: with two edges
/// leaving the axis the faces around it form two circles, not one fan.
fn check_not_pinched(vertices: &[Vertex], rings: &[Range<usize>]) -> Result<(), RevolutionError> {
    for ring in rings {
        for v in ring.clone().filter(|&v| vertices[v].on_axis()) {
            let (before, after) = neighbours(ring, v);
            if !vertices[before].on_axis() && !vertices[after].on_axis() {
                return Err(RevolutionError::PinchedOnAxis);
            }
        }
    }
    Ok(())
}

/// The whole number of steps, once the faces it gives are known to fit in [`MAX_SOLID_FACES`].
fn bounded_steps(
    steps: f64,
    vertices: &[Vertex],
    rings: &[Range<usize>],
    full: bool,
) -> Result<usize, RevolutionError> {
    let side_edges = rings
        .iter()
        .flat_map(|ring| ring.clone().map(|v| (v, neighbours(ring, v).1)))
        .filter(|&(a, b)| !(vertices[a].on_axis() && vertices[b].on_axis()))
        .count();
    let caps = if full { 0 } else { 2 };
    #[expect(
        clippy::cast_precision_loss,
        reason = "counts are at most MAX_PROFILE_EDGES"
    )]
    let faces = steps.mul_add(side_edges as f64, f64::from(caps));
    #[expect(
        clippy::cast_precision_loss,
        reason = "MAX_SOLID_FACES is far below 2^53"
    )]
    if faces > MAX_SOLID_FACES as f64 {
        return Err(RevolutionError::TooManyFaces);
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a whole number between 1 and MAX_SOLID_FACES"
    )]
    Ok(steps as usize)
}

/// Where each profile vertex lives in the solid's vertex list.
struct Layout {
    /// Index of the first copy of each profile vertex; `None` for one no face uses.
    first: Vec<Option<usize>>,
    sections: usize,
    count: usize,
}

impl Layout {
    /// A vertex on the axis exists once, any other once per section. In a full turn, a vertex
    /// between two edges along the axis is in no face, so it is left out.
    fn new(vertices: &[Vertex], rings: &[Range<usize>], sections: usize, full: bool) -> Self {
        let mut first = vec![None; vertices.len()];
        let mut count = 0;
        for ring in rings {
            for v in ring.clone() {
                let (before, after) = neighbours(ring, v);
                let on_axis = vertices[v].on_axis();
                if full && on_axis && vertices[before].on_axis() && vertices[after].on_axis() {
                    continue;
                }
                first[v] = Some(count);
                count += if on_axis { 1 } else { sections };
            }
        }
        Self {
            first,
            sections,
            count,
        }
    }

    /// The index of profile vertex `v` in section `k`; the last section of a full turn is the
    /// first.
    fn index(&self, vertices: &[Vertex], v: usize, k: usize) -> usize {
        let first = self.first[v].unwrap_or_default();
        if vertices[v].on_axis() {
            first
        } else {
            first + k % self.sections
        }
    }

    fn points(&self, frame: &Frame, vertices: &[Vertex], steps: usize, angle: f64) -> Vec<Point3> {
        let mut points = Vec::with_capacity(self.count);
        for (v, vertex) in vertices.iter().enumerate() {
            if self.first[v].is_none() {
                continue;
            }
            let copies = if vertex.on_axis() { 1 } else { self.sections };
            for k in 0..copies {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "section counts are at most MAX_SOLID_FACES"
                )]
                let phi = if k == steps {
                    // The last section of a partial turn is at the angle itself, exactly.
                    angle
                } else {
                    angle * k as f64 / steps as f64
                };
                points.push(frame.turned(vertex, phi));
            }
        }
        points
    }

    /// The side faces, then the two end faces of a partial turn. Along an edge a → b, material
    /// lies on the left, so a, b, b', a' (primes: the next section) turns counterclockwise seen
    /// from outside when the profile is left of the axis, which moves it towards +z; to the
    /// right of the axis the faces are reversed.
    fn faces(
        &self,
        vertices: &[Vertex],
        rings: &[Range<usize>],
        steps: usize,
        flip: bool,
        full: bool,
    ) -> Vec<FaceInput> {
        let oriented = |mut ring: Vec<usize>, reversed: bool| {
            if reversed {
                ring.reverse();
            }
            ring
        };
        let mut faces = Vec::new();
        for ring in rings {
            for a in ring.clone() {
                let b = neighbours(ring, a).1;
                let (a_axis, b_axis) = (vertices[a].on_axis(), vertices[b].on_axis());
                if a_axis && b_axis {
                    continue;
                }
                for k in 0..steps {
                    // On the axis, a vertex's two copies coincide: the quadrilateral becomes a
                    // triangle.
                    let mut outer = vec![self.index(vertices, a, k), self.index(vertices, b, k)];
                    if !b_axis {
                        outer.push(self.index(vertices, b, k + 1));
                    }
                    if !a_axis {
                        outer.push(self.index(vertices, a, k + 1));
                    }
                    faces.push(FaceInput {
                        outer: oriented(outer, flip),
                        holes: Vec::new(),
                    });
                }
            }
        }
        if !full {
            // Seen from outside, the start face looks against the motion: the profile reversed
            // (left of the axis), and the end face looks along it.
            let section = |k: usize, reversed: bool| {
                let ring_at = |ring: &Range<usize>| {
                    oriented(
                        ring.clone().map(|v| self.index(vertices, v, k)).collect(),
                        reversed,
                    )
                };
                FaceInput {
                    outer: ring_at(&rings[0]),
                    holes: rings[1..].iter().map(ring_at).collect(),
                }
            };
            faces.push(section(0, !flip));
            faces.push(section(steps, flip));
        }
        faces
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::tests::{ccw, circle, line, p, polygon, rectangle};
    use crate::{ARC_CHORD_TOLERANCE, Shell};

    /// The y axis, pointing up.
    const Y_AXIS: RevolutionAxis = RevolutionAxis {
        origin: Point2::new(0.0, 0.0),
        direction: Vector2::new(0.0, 1.0),
    };

    fn profile(outer: Vec<Edge>, holes: Vec<Vec<Edge>>) -> Profile {
        Profile::new(outer, holes).unwrap()
    }

    fn revolve(
        outer: Vec<Edge>,
        holes: Vec<Vec<Edge>>,
        axis: RevolutionAxis,
        angle: f64,
    ) -> Result<Solid, RevolutionError> {
        Solid::revolve(&profile(outer, holes), axis, angle)
    }

    /// Steps of at most 90° whose chords stay within the tolerance at radius `r`, by ADR 0008.
    fn steps(r: f64, angle: f64) -> usize {
        let chord = 2.0 * libm::acos(1.0 - ARC_CHORD_TOLERANCE / r);
        let n = (angle / chord).ceil().max((angle / FRAC_PI_2).ceil());
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a small whole number"
        )]
        let n = n as usize;
        n
    }

    fn real(n: usize) -> f64 {
        f64::from(u32::try_from(n).unwrap())
    }

    /// V = n · sin α · `A_f` · `r̄_f` for n steps of α = θ/n: each step is a wedge of the faceted
    /// profile, a trapezoid prism whose volume is sin α times the profile's first moment about
    /// the axis. `distance` is the faceted centroid's distance to the axis.
    fn faceted_volume(n: usize, angle: f64, profile: &Profile, distance: f64) -> f64 {
        let faceted = profile.faceted().unwrap();
        real(n) * libm::sin(angle / real(n)) * faceted.area() * distance
    }

    fn assert_near(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1e-9 * expected.abs(),
            "{actual} != {expected}"
        );
    }

    /// Pappus: V = A · θ · r̄ for the exact shape. The faceted solid differs from it by at most
    /// twice the tolerance (profile chords and revolution steps) times the area of the exact
    /// surface the arcs and steps replace (ADR 0020).
    fn assert_pappus(solid: &Solid, exact: f64, lateral_area: f64) {
        let gap = exact - solid.volume();
        assert!(
            gap.abs() <= 2.0 * ARC_CHORD_TOLERANCE * lateral_area,
            "{} vs Pappus {exact}",
            solid.volume()
        );
    }

    fn assert_rebuilds(solid: &Solid) {
        let rebuilt = Solid::from_faces(solid.vertices().to_vec(), &solid.to_faces()).unwrap();
        assert_eq!(&rebuilt, solid);
    }

    fn one_shell(solid: &Solid, genus: u32) {
        assert_eq!(solid.shells().len(), 1);
        assert_eq!(solid.shells()[0].genus(), genus);
        assert!(solid.volume() > 0.0);
    }

    #[test]
    fn a_full_turn_of_a_rectangle_off_the_axis_is_a_tube() {
        // 1 m × 1 m section from x = 1 to 2, around the y axis: a tube of inner radius 1 m and
        // outer radius 2 m, 1 m high. r_max = 2 m, so by ADR 0008 α_chord = 2 acos(1 − 1e-3/2)
        // = 0.0632 rad and n = ⌈2π / 0.0632⌉ = 100 steps. A = 1 m², r̄ = 1.5 m.
        // Faceted: V = 100 · sin(2π/100) · 1 · 1.5 = 9.4194 m³; Pappus: 2π · 1.5 = 9.4248 m³.
        let section = profile(rectangle(1.0, 0.0, 2.0, 1.0), vec![]);
        let solid = Solid::revolve(&section, Y_AXIS, TAU).unwrap();
        assert_eq!(steps(2.0, TAU), 100);
        assert_eq!(solid.face_count(), 4 * 100);
        assert_eq!(solid.vertices().len(), 4 * 100);
        // A ring: V − E + F = 0, so E = 800.
        assert_eq!(solid.edge_count(), 800);
        one_shell(&solid, 1);
        assert_near(solid.volume(), faceted_volume(100, TAU, &section, 1.5));
        assert_pappus(&solid, TAU * 1.5, 6.0 * PI);
        assert_rebuilds(&solid);
        // The profile to the right of the axis is the one that needed reversing: the start
        // section is the input profile.
        assert_eq!(solid.vertices()[0], Point3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn the_profile_on_either_side_of_the_axis_gives_the_same_solid() {
        let right = revolve(rectangle(1.0, 0.0, 2.0, 1.0), vec![], Y_AXIS, TAU).unwrap();
        let left = revolve(rectangle(-2.0, 0.0, -1.0, 1.0), vec![], Y_AXIS, TAU).unwrap();
        assert_eq!(left.face_count(), right.face_count());
        assert_near(left.volume(), right.volume());
        // The same turn around the axis pointing down is the mirror image of the first.
        let down = RevolutionAxis {
            origin: Point2::new(0.0, 0.0),
            direction: Vector2::new(0.0, -3.0),
        };
        let mirrored = revolve(rectangle(1.0, 0.0, 2.0, 1.0), vec![], down, PI / 3.0).unwrap();
        let direct = revolve(rectangle(1.0, 0.0, 2.0, 1.0), vec![], Y_AXIS, PI / 3.0).unwrap();
        assert_near(mirrored.volume(), direct.volume());
        // Right-hand rule: around +y the profile at x > 0 moves towards −z; around −y, towards
        // +z.
        let top = |solid: &Solid| solid.vertices().iter().map(|v| v.z).fold(0.0, f64::max);
        let bottom = |solid: &Solid| solid.vertices().iter().map(|v| v.z).fold(0.0, f64::min);
        assert!(top(&direct) < 1e-12 && bottom(&direct) < -1.0);
        assert!(top(&mirrored) > 1.0 && bottom(&mirrored) > -1e-12);
    }

    #[test]
    fn a_rectangle_with_a_side_on_the_axis_is_a_solid_cylinder() {
        // 1 m × 2 m against the y axis: radius 1 m, 2 m high. n = ⌈2π / 0.0894⌉ = 71. The side
        // on the axis gives no face; the bottom and top give a fan of 71 triangles each, the
        // outer side 71 quadrilaterals: 213 faces, 2 + 2·71 vertices; genus 0 gives
        // E = V + F − 2 = 355. A = 2 m², r̄ = 0.5 m: V = 71 · sin(2π/71) · 1 = 6.2766 m³ (Pappus
        // 2π = 6.2832).
        let section = profile(rectangle(0.0, 0.0, 1.0, 2.0), vec![]);
        let solid = Solid::revolve(&section, Y_AXIS, TAU).unwrap();
        assert_eq!(steps(1.0, TAU), 71);
        assert_eq!(solid.face_count(), 3 * 71);
        assert_eq!(solid.vertices().len(), 2 + 2 * 71);
        assert_eq!(solid.edge_count(), 355);
        one_shell(&solid, 0);
        assert_near(solid.volume(), faceted_volume(71, TAU, &section, 0.5));
        assert_pappus(&solid, TAU, 4.0 * PI);
        // The axis vertices are single: (0, 0, 0) and (0, 2, 0).
        let on_axis = solid.vertices().iter().filter(|v| v.x == 0.0 && v.z == 0.0);
        assert_eq!(on_axis.count(), 2);
        assert_rebuilds(&solid);
    }

    #[test]
    fn a_right_triangle_with_a_vertex_on_the_axis_is_a_cone() {
        // Radius 1 m, height 2 m: V = π r² h / 3 = 2π/3 = 2.0944 m³. Two edges leave the axis,
        // the third lies along it: 2 · 71 faces, all triangles; A = 1 m², r̄ = 1/3 m.
        let outer = polygon(&[(0.0, 0.0), (1.0, 0.0), (0.0, 2.0)]);
        let section = profile(outer.clone(), vec![]);
        let solid = Solid::revolve(&section, Y_AXIS, TAU).unwrap();
        assert_eq!(solid.face_count(), 2 * 71);
        one_shell(&solid, 0);
        assert_near(solid.volume(), faceted_volume(71, TAU, &section, 1.0 / 3.0));
        assert_pappus(&solid, 2.0 * PI / 3.0, PI * libm::sqrt(5.0));
        assert!(solid.to_faces().iter().all(|f| f.outer.len() == 3));
    }

    #[test]
    fn a_half_disc_on_the_axis_is_a_sphere() {
        // Unit half disc, its diameter on the y axis: V = 4π/3 = 4.18879 m³. The arc is first
        // replaced by ⌈π / 0.0894⌉ = 36 chords, so 37 vertices, two of them on the axis, and
        // 36 · 71 faces, none for the diameter. Pappus: A = π/2, r̄ = 4/(3π): V = 4π/3.
        let outer = vec![
            ccw(p(0.0, -1.0), p(0.0, 1.0), p(0.0, 0.0)),
            line(p(0.0, 1.0), p(0.0, -1.0)),
        ];
        let section = profile(outer, vec![]);
        let solid = Solid::revolve(&section, Y_AXIS, TAU).unwrap();
        assert_eq!(solid.face_count(), 36 * 71);
        assert_eq!(solid.vertices().len(), 2 + 35 * 71);
        one_shell(&solid, 0);
        let faceted = section.faceted().unwrap();
        let distance = faceted.centroid().x;
        assert_near(solid.volume(), faceted_volume(71, TAU, &section, distance));
        assert_pappus(&solid, 4.0 * PI / 3.0, 4.0 * PI);
    }

    #[test]
    fn a_partial_turn_has_two_end_faces() {
        // The tube's section turned by 90° only: n = ⌈(π/2) / 0.0632⌉ = 25 steps, 4 · 25 + 2
        // faces, a wedge of genus 0. V = 25 · sin(π/50) · 1 · 1.5 = 2.3558 m³ (Pappus
        // (π/2) · 1.5 = 2.3562).
        let angle = PI / 2.0;
        let section = profile(rectangle(1.0, 0.0, 2.0, 1.0), vec![]);
        let solid = Solid::revolve(&section, Y_AXIS, angle).unwrap();
        assert_eq!(steps(2.0, angle), 25);
        assert_eq!(solid.face_count(), 4 * 25 + 2);
        one_shell(&solid, 0);
        assert_near(solid.volume(), faceted_volume(25, angle, &section, 1.5));
        assert_pappus(&solid, angle * 1.5, angle * 3.0);
        // The end face is the profile in the plane x = 0, turned by 90° around +y: towards −z.
        let end = &solid.to_faces()[solid.face_count() - 1];
        for &v in &end.outer {
            let corner = solid.vertices()[v];
            assert!(corner.x.abs() < 1e-12 && (-2.0..=-1.0).contains(&corner.z));
        }
        assert_rebuilds(&solid);
    }

    #[test]
    fn a_hole_in_a_partial_turn_is_a_tunnel() {
        // Section x 1 to 5, y 0 to 4, with a hole x 2 to 4, y 1 to 3: A = 16 − 4 = 12 m²,
        // r̄ = 3 m, r_max = 5 m, α_chord = 2 acos(1 − 2e-4) = 0.04 rad, so a quarter turn takes
        // ⌈1.5708 / 0.04⌉ = 40 steps; 8 · 40 + 2 faces, genus 1. Pappus: 12 · (π/2) · 3.
        let angle = PI / 2.0;
        let hole = rectangle(2.0, 1.0, 4.0, 3.0);
        let section = profile(rectangle(1.0, 0.0, 5.0, 4.0), vec![hole]);
        let solid = Solid::revolve(&section, Y_AXIS, angle).unwrap();
        let n = steps(5.0, angle);
        assert_eq!(n, 40);
        assert_eq!(solid.face_count(), 8 * n + 2);
        one_shell(&solid, 1);
        assert_eq!(solid.to_faces()[solid.face_count() - 2].holes.len(), 1);
        assert_near(solid.volume(), faceted_volume(n, angle, &section, 3.0));
        assert_pappus(&solid, 12.0 * angle * 3.0, angle * 36.0);
        assert_rebuilds(&solid);
    }

    #[test]
    fn a_hole_in_a_full_turn_is_a_void() {
        // The same section turned fully: n = ⌈2π / 0.04⌉ = 158 steps. The outer ring has genus
        // 1 and so has the cavity, a closed ring-shaped void with a negative volume. Outer shell
        // (A = 16, r̄ = 3): V_outer = n sin α · 4 · 3 · ... the faceted formula is linear in the
        // profile's moment, so the whole solid has n sin α · (16 · 3 − 4 · 3) = 12 · 3 · n sin α.
        let hole = rectangle(2.0, 1.0, 4.0, 3.0);
        let section = profile(rectangle(1.0, 0.0, 5.0, 4.0), vec![hole]);
        let solid = Solid::revolve(&section, Y_AXIS, TAU).unwrap();
        let n = steps(5.0, TAU);
        assert_eq!(n, 158);
        assert_eq!(solid.face_count(), 8 * n);
        let shells: &[Shell] = solid.shells();
        assert_eq!(shells.len(), 2);
        assert!(shells.iter().all(|s| s.genus() == 1));
        assert_eq!(shells.iter().filter(|s| s.is_void()).count(), 1);
        assert_near(solid.volume(), faceted_volume(n, TAU, &section, 3.0));
        assert!(solid.volume() > 0.0);
        assert_rebuilds(&solid);
    }

    #[test]
    fn the_axis_may_be_anywhere_in_the_plane() {
        // The tube's section and axis both moved by (3, 4): the same solid, moved.
        let moved = RevolutionAxis {
            origin: Point2::new(3.0, 4.0),
            direction: Vector2::new(0.0, 2.0),
        };
        let solid = revolve(rectangle(4.0, 4.0, 5.0, 5.0), vec![], moved, TAU).unwrap();
        let tube = revolve(rectangle(1.0, 0.0, 2.0, 1.0), vec![], Y_AXIS, TAU).unwrap();
        assert_eq!(solid.face_count(), tube.face_count());
        assert_near(solid.volume(), tube.volume());
        // The line y = x, direction (1, 1): the unit square from (2, 0) to (3, 1) lies to its
        // right. Its centre (2.5, 0.5) is |0.5 − 2.5| / √2 = 1.4142 m from the line, so Pappus
        // gives V = 2π · 1.4142 = 8.8858 m³. The farthest corner (3, 0) is 3/√2 = 2.1213 m away.
        let oblique = RevolutionAxis {
            origin: Point2::new(0.0, 0.0),
            direction: Vector2::new(1.0, 1.0),
        };
        let solid = revolve(rectangle(2.0, 0.0, 3.0, 1.0), vec![], oblique, TAU).unwrap();
        let radius = 3.0 / libm::sqrt(2.0);
        assert_pappus(&solid, TAU * libm::sqrt(2.0), TAU * 4.0 * radius);
        one_shell(&solid, 1);
    }

    #[test]
    fn a_vertex_touching_the_axis_pinches_a_full_turn_only() {
        // A triangle whose corner (0, 0) alone touches the axis: two cones meet at their tip in
        // a full turn, a legal wedge in a partial one, where the end faces close the fan.
        let outer = polygon(&[(0.0, 0.0), (1.0, -1.0), (1.0, 1.0)]);
        let full = revolve(outer.clone(), vec![], Y_AXIS, TAU);
        assert_eq!(full.err(), Some(RevolutionError::PinchedOnAxis));
        let partial = revolve(outer, vec![], Y_AXIS, PI).unwrap();
        one_shell(&partial, 0);
        // A diamond touching the axis at one corner.
        let diamond = polygon(&[(0.0, 0.0), (1.0, -1.0), (2.0, 0.0), (1.0, 1.0)]);
        assert_eq!(
            revolve(diamond.clone(), vec![], Y_AXIS, TAU).err(),
            Some(RevolutionError::PinchedOnAxis)
        );
        one_shell(&revolve(diamond, vec![], Y_AXIS, 1.0).unwrap(), 0);
    }

    #[test]
    fn a_vertex_between_two_edges_on_the_axis_is_in_no_face_of_a_full_turn() {
        // The cylinder's axis side has a middle vertex (0, 1): a full turn leaves it out, so
        // the solid is the same 144 vertices; a partial turn keeps it in the end faces.
        let outer = polygon(&[(0.0, 0.0), (1.0, 0.0), (1.0, 2.0), (0.0, 2.0), (0.0, 1.0)]);
        let full = revolve(outer.clone(), vec![], Y_AXIS, TAU).unwrap();
        assert_eq!(full.vertices().len(), 2 + 2 * 71);
        assert_eq!(full.face_count(), 3 * 71);
        let cylinder = revolve(rectangle(0.0, 0.0, 1.0, 2.0), vec![], Y_AXIS, TAU).unwrap();
        assert_near(full.volume(), cylinder.volume());
        let partial = revolve(outer, vec![], Y_AXIS, PI).unwrap();
        assert_eq!(partial.vertices().len(), 3 + 2 * 37);
        one_shell(&partial, 0);
    }

    #[test]
    fn an_angle_just_below_a_full_turn_still_takes_steps_of_at_most_90_degrees() {
        // 2π − 0.01 rad at r_max = 2 m leaves a 2 · 2 · sin(0.005) = 0.02 m gap between the
        // end faces: accepted, with the full turn's 100 steps, so 4 · 100 + 2 faces.
        let angle = TAU - 0.01;
        let solid = revolve(rectangle(1.0, 0.0, 2.0, 1.0), vec![], Y_AXIS, angle).unwrap();
        let n = (solid.face_count() - 2) / 4;
        assert_eq!(n, 100);
        assert!(angle / real(n) <= FRAC_PI_2);
        one_shell(&solid, 0);
    }

    #[test]
    #[expect(clippy::float_cmp, reason = "whole numbers")]
    fn small_radii_still_turn_by_steps_of_at_most_90_degrees() {
        // Below half the tolerance the chord clamp allows a half turn per chord; the 90° cap
        // then gives a full turn 4 steps.
        assert_eq!(step_count(0.0004, TAU, FRAC_PI_2, 1.0), 4.0);
        let section = profile(rectangle(0.0, 0.0, 0.0004, 0.01), vec![]);
        let solid = Solid::revolve(&section, Y_AXIS, TAU).unwrap();
        // A pyramid-like core: 3 side faces per step.
        assert_eq!(solid.face_count(), 3 * 4);
        one_shell(&solid, 0);
        // The square pyramid-prism has volume n sin α A r̄ = 4 · 1 · (0.0004 · 0.01) · 0.0002.
        assert_near(solid.volume(), 4.0 * 0.0004 * 0.01 * 0.0002);
    }

    #[test]
    fn every_error_is_reported() {
        let rect = rectangle(1.0, 0.0, 2.0, 1.0);
        let run = |axis: RevolutionAxis, angle: f64| revolve(rect.clone(), vec![], axis, angle);
        let axis = |ox: f64, oy: f64, dx: f64, dy: f64| RevolutionAxis {
            origin: Point2::new(ox, oy),
            direction: Vector2::new(dx, dy),
        };
        for bad in [f64::NAN, f64::INFINITY] {
            assert_eq!(
                run(axis(bad, 0.0, 0.0, 1.0), 1.0).err(),
                Some(RevolutionError::NonFiniteAxis)
            );
            assert_eq!(
                run(axis(0.0, 0.0, 0.0, bad), 1.0).err(),
                Some(RevolutionError::NonFiniteAxis)
            );
        }
        assert_eq!(
            run(axis(2e4, 0.0, 0.0, 1.0), 1.0).err(),
            Some(RevolutionError::AxisOutsideExtent)
        );
        assert_eq!(
            run(axis(0.0, 0.0, 0.0, 0.0), 1.0).err(),
            Some(RevolutionError::ZeroAxisDirection)
        );
        for angle in [0.0, -1.0, TAU + 1e-9, 7.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                run(Y_AXIS, angle).err(),
                Some(RevolutionError::AngleOutOfRange)
            );
        }
        let crossing = revolve(rectangle(-1.0, 0.0, 1.0, 1.0), vec![], Y_AXIS, 1.0);
        assert_eq!(crossing.err(), Some(RevolutionError::ProfileCrossesAxis));
        // Gap between the end faces at r_max = 2 m: 4 · sin(5e-8) = 2e-7 m < 1 µm.
        assert_eq!(
            run(Y_AXIS, TAU - 1e-7).err(),
            Some(RevolutionError::NearlyFullTurn)
        );
        // A circle of radius 1 km 1.5 km from the axis: 2221 chords, about 3500 steps.
        let wide = revolve(circle(1500.0, 0.0, 1000.0), vec![], Y_AXIS, TAU);
        assert_eq!(wide.err(), Some(RevolutionError::TooManyFaces));
        // 6600 chords exceed the profile's edge limit while faceting.
        let huge = revolve(circle(0.0, 0.0, 9000.0), vec![], Y_AXIS, 1.0);
        assert_eq!(
            huge.err(),
            Some(RevolutionError::Faceting(ProfileError::TooManyEdges))
        );
        // Turned 9 km further out than the extent allows.
        let far = axis(-9000.0, 0.0, 0.0, 1.0);
        let outside = revolve(rectangle(9000.0, 0.0, 9001.0, 1.0), vec![], far, 1.0);
        assert!(matches!(
            outside.err(),
            Some(RevolutionError::Solid(BrepError::OutsideModelExtent { .. }))
        ));
        // A sliver of a turn is too thin to be a solid.
        let thin = run(Y_AXIS, 1e-9);
        assert!(matches!(thin, Err(RevolutionError::Solid(_))), "{thin:?}");
    }

    #[test]
    fn a_direction_of_any_magnitude_is_accepted() {
        for scale in [1e-300, 5e-324, 1.0, 1e300] {
            let axis = RevolutionAxis {
                origin: Point2::new(0.0, 0.0),
                direction: Vector2::new(0.0, scale),
            };
            let solid = revolve(rectangle(1.0, 0.0, 2.0, 1.0), vec![], axis, 1.0).unwrap();
            assert_eq!(solid.face_count(), 4 * steps(2.0, 1.0) + 2);
        }
    }

    #[test]
    fn a_vertex_within_the_tolerance_of_the_axis_counts_as_on_it() {
        // The section's side is 0.5 µm off the axis: snapped onto it, as in a cylinder.
        let section = rectangle(5e-7, 0.0, 1.0, 2.0);
        let solid = revolve(section, vec![], Y_AXIS, TAU).unwrap();
        assert_eq!(solid.face_count(), 3 * 71);
        one_shell(&solid, 0);
    }
}
