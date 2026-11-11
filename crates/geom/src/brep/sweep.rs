//! Solids made by sweeping a profile along a path (GEO-02).
//!
//! Rationale and constants: `docs/dev/adr/0020-sweeps-and-revolutions.md`.

use core::f64::consts::{FRAC_PI_2, PI, TAU};
use core::ops::Range;

use super::extrude::{rings, swept_faces};
use super::{BrepError, MAX_SOLID_FACES, Solid};
use crate::path::{Arc3, Curve3};
use crate::profile::step_count;
use crate::tolerance::WITHIN_TOLERANCE;
use crate::vector::{Vector, across, add, coords, cross, dot, length, scale, sub};
use crate::{ARC_CHORD_TOLERANCE, Edge, Path, Point3, Profile, ProfileError, Vector3};

/// Largest number of straight pieces a sweep cuts its path into.
///
/// The self-intersection test compares pieces pairwise; the bound keeps it to about half a
/// million pairs. A path arc of 100 m radius still takes a full turn's worth of pieces.
pub const MAX_SWEEP_PIECES: usize = 1024;

/// Why a profile cannot be swept along a path.
///
/// Like [`BrepError`] and [`ProfileError`], no variant carries coordinates (SEC-03); path
/// segments are named by their index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum SweepError {
    /// A component of the reference vector is NaN or infinite.
    #[error("the sweep reference has a component that is not a finite number")]
    NonFiniteReference,
    /// The reference vector is zero.
    #[error("the sweep reference is the zero vector")]
    ZeroReference,
    /// Replacing the profile's arcs by chords gives an invalid profile.
    #[error("the profile's arcs cannot be replaced by chords")]
    Faceting(#[from] ProfileError),
    /// The path would be cut into more than [`MAX_SWEEP_PIECES`] straight pieces.
    #[error("the path would be cut into too many pieces")]
    TooManyPieces,
    /// The solid would have more than [`MAX_SOLID_FACES`] faces.
    #[error("the swept solid would have too many faces")]
    TooManyFaces,
    /// The reference vector is parallel to the path's start tangent: across the profile, its
    /// part normal to the tangent is within the linear tolerance.
    #[error("the sweep reference is parallel to the start of the path")]
    ReferenceAlongPath,
    /// The path turns back on itself where `segment` starts.
    #[error("the path turns back on itself at the start of segment {segment}")]
    DoublesBack {
        /// The segment after the turn.
        segment: usize,
    },
    /// On an arc, a point of the profile comes so near the arc's axis, or beyond it, that the
    /// inner side would fold over itself, or run less than the linear tolerance along a step.
    #[error("the profile reaches the axis of arc segment {segment}")]
    ArcTooTight {
        /// The arc segment.
        segment: usize,
    },
    /// On a straight piece of the path, the planes that end it at its two corners meet within
    /// the profile: a corner is too sharp for the piece's length.
    #[error("the corners of segment {segment} cut through each other")]
    MitresCross {
        /// The segment the piece belongs to.
        segment: usize,
    },
    /// Two parts of the path that are not neighbours come too close for the profile.
    #[error("the sweep along segments {first} and {second} may cross itself")]
    SelfIntersecting {
        /// The first segment.
        first: usize,
        /// The second segment, no smaller than `first`.
        second: usize,
    },
    /// The swept faces do not bound a valid solid: too thin, or outside the model extent.
    #[error("the swept solid is invalid")]
    Solid(#[from] BrepError),
}

impl Solid {
    /// Sweeps a profile along `path`.
    ///
    /// The profile's origin is placed at the path's start, in the plane normal to the start
    /// tangent t: its y axis points along the part of `reference` normal to t, and its x axis is
    /// y × t, so (x, y, t) is right-handed. For a beam, `reference` is usually the world z axis.
    /// The profile then moves without twisting (parallel transport): along a line it keeps its
    /// orientation, along an arc it turns with the tangent around the arc's axis, at a corner by
    /// the smallest rotation taking one direction to the next. At each corner the section is the
    /// profile projected onto the plane bisecting the corner (a mitre). The solid has an end
    /// face at each end of the path, and a hole through it for each profile hole.
    ///
    /// Arcs of the profile are first replaced by chords, and each path arc of radius R by its
    /// tangent polygon: n steps of α ≤ 90°, that is n − 1 pieces touching the arc plus two half
    /// pieces at its ends, so the path keeps its tangents where the arc starts and ends. n is
    /// computed for the farthest a profile point gets from the arc's axis, so the solid stays within
    /// [`ARC_CHORD_TOLERANCE`](crate::ARC_CHORD_TOLERANCE) of the exact shape
    /// (`docs/dev/adr/0020-sweeps-and-revolutions.md`). The volume is that of the faceted shape:
    /// a straight piece contributes the faceted profile's area `A_f` times the length its
    /// centroid travels between the piece's two end planes, and a path arc
    /// 2n · tan(α/2) · `A_f` · r̄, with r̄ the centroid's distance from the arc's axis. The
    /// tangent polygon lies outside the exact circle, so this is slightly *more* than Pappus'
    /// θ · `A_f` · r̄.
    ///
    /// # Errors
    ///
    /// [`SweepError`] when the reference is not finite, zero or parallel to the start tangent,
    /// when faceting the profile's arcs fails, when the sweep would have more than
    /// [`MAX_SWEEP_PIECES`] pieces or [`MAX_SOLID_FACES`] faces, when the path turns back on
    /// itself, when the profile reaches the axis of a path arc, when two corners cut through
    /// a piece, when parts of the sweep may cross each other (a conservative test: it may
    /// refuse a sweep whose distant parts come close without touching), or when the result is
    /// not a valid solid.
    pub fn sweep(profile: &Profile, path: &Path, reference: Vector3) -> Result<Self, SweepError> {
        let reference = unit_reference(reference)?;
        let profile = profile.faceted()?;
        let (offsets, rings, reach) = layout(&profile);
        let pieces = Pieces::new(path, reach, offsets.len())?;
        let start = pieces.start_section(reference, &offsets)?;
        pieces.check_turns()?;
        let sections = pieces.sections(path, start, offsets.len())?;
        pieces.check_apart(&sections, offsets.len())?;
        let points = sections
            .into_iter()
            .map(|[x, y, z]| Point3::new(x, y, z))
            .collect();
        let faces = swept_faces(&rings, pieces.count() + 1, offsets.len());
        Ok(Self::from_faces(points, &faces)?)
    }
}

/// The faceted profile's vertices as offsets from the path, in the profile's frame, the rings
/// they form, and the farthest any profile point gets from the path: edges are straight, so that
/// is a vertex.
fn layout(faceted: &Profile) -> (Vec<[f64; 2]>, Vec<Range<usize>>, f64) {
    let loops: Vec<&[Edge]> = core::iter::once(faceted.outer())
        .chain(faceted.holes())
        .collect();
    let offsets: Vec<[f64; 2]> = loops
        .iter()
        .flat_map(|edges| edges.iter().map(|e| [e.start().x, e.start().y]))
        .collect();
    let reach = offsets
        .iter()
        .fold(0.0, |r: f64, &[u, v]| r.max(length([u, v, 0.0])));
    (offsets, rings(&loops), reach)
}

/// The reference as a unit vector, scaled first so that a huge or subnormal one neither
/// overflows nor underflows.
fn unit_reference(reference: Vector3) -> Result<Vector, SweepError> {
    let r = [reference.x, reference.y, reference.z];
    if !r.iter().all(|c| c.is_finite()) {
        return Err(SweepError::NonFiniteReference);
    }
    let largest = r.iter().fold(0.0, |m: f64, c| m.max(c.abs()));
    if largest == 0.0 {
        return Err(SweepError::ZeroReference);
    }
    // Divided, not multiplied by the reciprocal, which overflows for a subnormal reference.
    let r = r.map(|c| c / largest);
    Ok(scale(r, 1.0 / length(r)))
}

/// The number of steps a path arc is cut into, as a whole-valued float.
fn arc_steps(arc: &Arc3, reach: f64) -> f64 {
    // A tangent polygon of radius r stays within s of its circle exactly when the chords of a
    // circle of radius r + s do: 1/cos(h) − 1 ≤ s/r ⇔ 1 − cos(h) ≤ s/(r + s).
    let radius = arc.radius + reach + ARC_CHORD_TOLERANCE;
    step_count(radius, arc.sweep, FRAC_PI_2, 1.0)
}

/// The points of the tangent polygon of `arc` after its start: at angle (k + ½)α and radius
/// R / cos(α/2) for k < `steps`, then the end. Computed from the start, so that a large radius
/// does not swamp a short arc: with h = α/2, `V_k` − start is
/// R / cos h · (−2 sin((k + 1)h) sin(kh) · outward + sin((2k + 1)h) · tangent).
fn tangent_polygon(arc: &Arc3, steps: u32, points: &mut Vec<Vector>) {
    let half = arc.sweep / (2.0 * f64::from(steps));
    let radius = arc.radius / libm::cos(half);
    for k in (0..steps).map(f64::from) {
        let out = -2.0 * libm::sin((k + 1.0) * half) * libm::sin(k * half);
        let along = libm::sin(2.0f64.mul_add(k, 1.0) * half);
        points.push(add(
            arc.start,
            add(
                scale(arc.outward, radius * out),
                scale(arc.tangent, radius * along),
            ),
        ));
    }
    points.push(arc.end);
}

/// The path cut into straight pieces.
struct Pieces {
    /// The path's start, then the end of each piece.
    points: Vec<Vector>,
    /// The unit direction of each piece.
    directions: Vec<Vector>,
    /// The segment of each piece.
    segments: Vec<usize>,
    /// The first piece of each segment, then the number of pieces.
    firsts: Vec<usize>,
    /// The farthest a profile point gets from the path.
    reach: f64,
    /// Whether each piece is bounded by radial planes of its arc at both ends (see
    /// [`is_radial`](Self::is_radial)).
    radial: Vec<bool>,
}

impl Pieces {
    /// Counts the pieces and the faces they give before building anything.
    fn new(path: &Path, reach: f64, edges: usize) -> Result<Self, SweepError> {
        let steps: Vec<f64> = path
            .curves()
            .iter()
            .map(|curve| match curve {
                Curve3::Line(..) => 0.0,
                Curve3::Arc(arc) => arc_steps(arc, reach),
            })
            .collect();
        // An arc of n steps has n + 1 pieces, a line one.
        let count: f64 = steps.iter().map(|n| n + 1.0).sum();
        #[expect(
            clippy::cast_precision_loss,
            reason = "the limits and edge counts are far below 2^53"
        )]
        {
            if count > MAX_SWEEP_PIECES as f64 {
                return Err(SweepError::TooManyPieces);
            }
            if count.mul_add(edges as f64, 2.0) > MAX_SOLID_FACES as f64 {
                return Err(SweepError::TooManyFaces);
            }
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a whole number up to MAX_SWEEP_PIECES"
        )]
        let mut points = Vec::with_capacity(count as usize + 1);
        points.push(coords(path.segments()[0].start()));
        let mut segments = Vec::new();
        let mut firsts = Vec::with_capacity(steps.len() + 1);
        for (segment, (curve, &n)) in path.curves().iter().zip(&steps).enumerate() {
            firsts.push(points.len() - 1);
            match curve {
                Curve3::Line(end) => points.push(*end),
                Curve3::Arc(arc) => {
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a whole number between 1 and MAX_SWEEP_PIECES"
                    )]
                    tangent_polygon(arc, n as u32, &mut points);
                }
            }
            segments.resize(points.len() - 1, segment);
        }
        firsts.push(points.len() - 1);
        // Pieces are never empty: lines are longer than the tolerance, and tangent polygon
        // sides are at least R · tan(θ / 2048) > 0.
        let directions = points
            .windows(2)
            .map(|pair| {
                let d = sub(pair[1], pair[0]);
                scale(d, 1.0 / length(d))
            })
            .collect();
        let mut pieces = Self {
            points,
            directions,
            segments,
            firsts,
            reach,
            radial: Vec::new(),
        };
        pieces.radial = (0..pieces.count()).map(|k| pieces.is_radial(k)).collect();
        Ok(pieces)
    }

    const fn count(&self) -> usize {
        self.segments.len()
    }

    /// The normal of the plane through joint `k` (not of unit length): normal to the path at
    /// its two ends, bisecting the corner between two pieces elsewhere.
    fn joint_normal(&self, k: usize) -> Vector {
        let d = &self.directions;
        if self.is_path_end(k) {
            d[k.min(d.len() - 1)]
        } else {
            add(d[k - 1], d[k])
        }
    }

    /// Whether joint `k` is the start or the end of the path.
    const fn is_path_end(&self, k: usize) -> bool {
        k == 0 || k == self.count()
    }

    /// The cosine of half the turn at joint `k`: 1 at the ends of the path.
    fn half_turn_cosine(&self, k: usize) -> f64 {
        if self.is_path_end(k) {
            1.0
        } else {
            length(self.joint_normal(k)) / 2.0
        }
    }

    /// The profile placed at the start of the path, in the plane normal to its first piece.
    fn start_section(
        &self,
        reference: Vector,
        offsets: &[[f64; 2]],
    ) -> Result<Vec<Vector>, SweepError> {
        let t = self.directions[0];
        let normal = across(reference, t);
        let size = length(normal);
        // ADR 0001 has no angular tolerance: the reference is parallel to t when its normal part
        // turns no profile point by more than the tolerance.
        if size * self.reach <= WITHIN_TOLERANCE {
            return Err(SweepError::ReferenceAlongPath);
        }
        let y = scale(normal, 1.0 / size);
        let x = cross(y, t);
        let origin = self.points[0];
        Ok(offsets
            .iter()
            .map(|&[u, v]| [0, 1, 2].map(|i| u.mul_add(x[i], v.mul_add(y[i], origin[i]))))
            .collect())
    }

    /// Rejects a turn back where a segment starts. Inside an arc the turns are at most 90°.
    fn check_turns(&self) -> Result<(), SweepError> {
        let segments = self.firsts.len() - 1;
        match (1..segments)
            .find(|&s| length(self.joint_normal(self.firsts[s])) * self.reach <= WITHIN_TOLERANCE)
        {
            Some(segment) => Err(SweepError::DoublesBack { segment }),
            None => Ok(()),
        }
    }

    /// Every section, from the start one: each profile vertex runs along piece k from its
    /// point in section k to the plane of joint k + 1. Parallel transport needs no frame: the
    /// projection along the incoming direction onto the plane bisecting a corner is the profile
    /// turned by the smallest rotation between the two directions (ADR 0020), and the side faces
    /// are planar by construction, their two long edges both along the piece.
    fn sections(
        &self,
        path: &Path,
        mut sections: Vec<Vector>,
        stride: usize,
    ) -> Result<Vec<Vector>, SweepError> {
        sections.reserve(stride * self.count());
        for (k, &t) in self.directions.iter().enumerate() {
            let segment = self.segments[k];
            if k == self.firsts[segment]
                && let Curve3::Arc(arc) = &path.curves()[segment]
            {
                self.check_arc(arc, &sections[k * stride..(k + 1) * stride], segment)?;
            }
            let (point, normal) = (self.points[k + 1], self.joint_normal(k + 1));
            let along = dot(t, normal);
            for v in 0..stride {
                let from = sections[k * stride + v];
                let run = dot(sub(point, from), normal) / along;
                if run.is_nan() || run <= WITHIN_TOLERANCE {
                    // Between two radial planes of an arc, a short run is the profile reaching
                    // too near the arc's axis: (inner) · tan(α/2) per half piece. Elsewhere a
                    // corner's mitre cuts it.
                    let on_arc = matches!(path.curves()[segment], Curve3::Arc(_));
                    return Err(if on_arc && self.radial[k] {
                        SweepError::ArcTooTight { segment }
                    } else {
                        SweepError::MitresCross { segment }
                    });
                }
                sections.push(add(from, scale(t, run)));
            }
        }
        Ok(sections)
    }

    /// The profile point nearest the arc's axis, at `inner` from it, must lie more than the
    /// tolerance from the axis; how far it then runs along each piece is the mitre check's. The
    /// offset towards the axis is the same in every section of the arc, so the section where the
    /// arc starts decides. When both ends of the arc are radial planes, nothing else tests its
    /// two end pieces against each other: the gap between them, at that point, must exceed the
    /// tolerance as well.
    fn check_arc(&self, arc: &Arc3, section: &[Vector], segment: usize) -> Result<(), SweepError> {
        let start = arc.start;
        let inward = section.iter().fold(f64::NEG_INFINITY, |m, &p| {
            m.max(-dot(sub(p, start), arc.outward))
        });
        let inner = arc.radius - inward;
        if inner <= WITHIN_TOLERANCE {
            return Err(SweepError::ArcTooTight { segment });
        }
        let (first, end) = (self.firsts[segment], self.firsts[segment + 1]);
        let gap = 2.0 * inner * libm::sin((TAU - arc.sweep) / 2.0);
        if arc.sweep > PI && gap <= WITHIN_TOLERANCE && self.radial[first] && self.radial[end - 1] {
            return Err(SweepError::SelfIntersecting {
                first: segment,
                second: segment,
            });
        }
        Ok(())
    }

    /// Whether piece `k` is bounded by radial planes of its arc at both ends: inside the arc, at
    /// an end of the path, or where the path runs straight on. A first or last piece meeting
    /// another segment at a corner is cut by the corner's mitre instead.
    fn is_radial(&self, k: usize) -> bool {
        let segment = self.segments[k];
        let (first, end) = (self.firsts[segment], self.firsts[segment + 1]);
        (k > first || self.is_straight(first)) && (k + 1 < end || self.is_straight(end))
    }

    /// Whether joint `k` is an end of the path, or turns so little that its plane is the plane
    /// normal to the path within the tolerance, across the profile.
    fn is_straight(&self, k: usize) -> bool {
        self.is_path_end(k)
            || length(sub(self.directions[k - 1], self.directions[k])) * self.reach
                <= WITHIN_TOLERANCE
    }

    /// No two pieces that are not neighbours may meet (ADR 0020, D3): every such pair must be
    /// [`clear`](Self::clear).
    fn check_apart(&self, sections: &[Vector], stride: usize) -> Result<(), SweepError> {
        let reaches = self.reaches();
        for i in 0..self.count() {
            for j in i + 2..self.count() {
                if !self.clear(i, j, sections, stride, &reaches) {
                    return Err(SweepError::SelfIntersecting {
                        first: self.segments[i],
                        second: self.segments[j],
                    });
                }
            }
        }
        Ok(())
    }

    /// How far each piece may reach from its axis segment: the profile's reach divided by
    /// cos(φ/2) for a corner of φ at either end, the mitre's tip.
    fn reaches(&self) -> Vec<f64> {
        (0..self.count())
            .map(|k| self.reach / self.half_turn_cosine(k).min(self.half_turn_cosine(k + 1)))
            .collect()
    }

    /// Whether pieces `i` and `j`, not neighbours, provably do not meet. Pieces of one arc
    /// bounded by its radial planes each lie in their own wedge of the arc. Any other pair must
    /// pass one of two sound tests. First, axis segments farther apart than both reaches cannot
    /// meet. Otherwise, each piece lies in the convex hull of its two sections, so a plane with
    /// every vertex of one piece on one side and every vertex of the other on the other side,
    /// more than the tolerance apart, separates them; the planes tried are those where the first
    /// piece's segment ends and where the second's starts.
    fn clear(
        &self,
        first: usize,
        second: usize,
        sections: &[Vector],
        stride: usize,
        reaches: &[f64],
    ) -> bool {
        let segments = (self.segments[first], self.segments[second]);
        if segments.0 == segments.1 && self.radial[first] && self.radial[second] {
            return true;
        }
        let points = &self.points;
        let axis = |k: usize| [points[k], points[k + 1]];
        if segment_distance(axis(first), axis(second))
            > reaches[first] + reaches[second] + WITHIN_TOLERANCE
        {
            return true;
        }
        let separates = |joint: usize| {
            let normal = self.joint_normal(joint);
            let normal = scale(normal, 1.0 / length(normal));
            let origin = points[joint];
            // The lowest and highest signed distances of a piece's vertices from the plane.
            let side = |k: usize| {
                sections[k * stride..(k + 2) * stride].iter().fold(
                    (f64::INFINITY, f64::NEG_INFINITY),
                    |(low, high), &vertex| {
                        let distance = dot(sub(vertex, origin), normal);
                        (low.min(distance), high.max(distance))
                    },
                )
            };
            let (one, two) = (side(first), side(second));
            one.1 < two.0 - WITHIN_TOLERANCE || two.1 < one.0 - WITHIN_TOLERANCE
        };
        separates(self.firsts[segments.0 + 1]) || separates(self.firsts[segments.1])
    }
}

/// The distance between two segments, from their closest points (Ericson, *Real-Time Collision
/// Detection*, 5.1.9). Both segments have a positive length.
fn segment_distance([start1, end1]: [Vector; 2], [start2, end2]: [Vector; 2]) -> f64 {
    let (d1, d2, between) = (sub(end1, start1), sub(end2, start2), sub(start1, start2));
    let (long1, long2) = (dot(d1, d1), dot(d2, d2));
    let (along1, along2, cosine) = (dot(d1, between), dot(d2, between), dot(d1, d2));
    let denominator = long1.mul_add(long2, -cosine * cosine);
    // The closest point's fraction along the first segment, then along the second; parallel
    // segments start from the first's start.
    let s = if denominator > 0.0 {
        (cosine.mul_add(along2, -along1 * long2) / denominator).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let t = cosine.mul_add(s, along2) / long2;
    let (s, t) = if t < 0.0 {
        ((-along1 / long1).clamp(0.0, 1.0), 0.0)
    } else if t > 1.0 {
        (((cosine - along1) / long1).clamp(0.0, 1.0), 1.0)
    } else {
        (s, t)
    };
    length(sub(add(start1, scale(d1, s)), add(start2, scale(d2, t))))
}

#[cfg(test)]
mod tests {
    use core::f64::consts::{PI, TAU};

    use super::*;
    use crate::path::tests::{arc, line, q};
    use crate::profile::tests::{circle, rectangle};
    use crate::{ARC_CHORD_TOLERANCE, MODEL_EXTENT, PathSegment};

    const Z: Vector3 = Vector3::new(0.0, 0.0, 1.0);

    fn profile(outer: Vec<Edge>, holes: Vec<Vec<Edge>>) -> Profile {
        Profile::new(outer, holes).unwrap()
    }

    /// 0.2 m wide (profile x), 0.1 m high (profile y), centred on the path: A = 0.02 m²,
    /// reaching hypot(0.1, 0.05) = 0.1118 m from it.
    fn bar() -> Profile {
        profile(rectangle(-0.1, -0.05, 0.1, 0.05), vec![])
    }

    fn sweep(profile: &Profile, segments: Vec<PathSegment>, reference: Vector3) -> Solid {
        Solid::sweep(profile, &Path::new(segments).unwrap(), reference).unwrap()
    }

    fn try_sweep(
        profile: &Profile,
        segments: Vec<PathSegment>,
        reference: Vector3,
    ) -> Result<Solid, SweepError> {
        Solid::sweep(profile, &Path::new(segments).unwrap(), reference)
    }

    /// The point at `angle` of the circle of `radius` around (cx, cy) in the plane z = 0.
    fn on_circle(cx: f64, cy: f64, radius: f64, angle: f64) -> Point3 {
        q(
            radius.mul_add(libm::cos(angle), cx),
            radius.mul_add(libm::sin(angle), cy),
            0.0,
        )
    }

    /// The arc of the circle around (cx, cy) in z = 0, counterclockwise from angle `from` to
    /// `to`.
    fn circle_arc(cx: f64, cy: f64, radius: f64, from: f64, to: f64) -> PathSegment {
        let at = |angle| on_circle(cx, cy, radius, angle);
        arc(at(from), at(f64::midpoint(from, to)), at(to))
    }

    /// Steps of a path arc of radius `r` for a profile reaching `d`, by ADR 0008 at r + d + s,
    /// at most 90° each.
    fn steps(r: f64, d: f64, angle: f64) -> f64 {
        let chord = 2.0 * libm::acos(1.0 - ARC_CHORD_TOLERANCE / (r + d + ARC_CHORD_TOLERANCE));
        (angle / chord).ceil().max((angle / FRAC_PI_2).ceil())
    }

    fn count(n: f64) -> usize {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a small whole number"
        )]
        let n = n as usize;
        n
    }

    /// V = 2n · tan(θ / 2n) · `A_f` · r̄: the centroid runs along the tangent polygon of radius
    /// r̄, 2n half sides of r̄ · tan(θ / 2n).
    fn arc_volume(n: f64, angle: f64, area: f64, distance: f64) -> f64 {
        2.0 * n * libm::tan(angle / (2.0 * n)) * area * distance
    }

    fn assert_near(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1e-9 * expected.abs(),
            "{actual} != {expected}"
        );
    }

    fn assert_point(actual: Point3, expected: Point3) {
        let gap = length(sub(coords(actual), coords(expected)));
        assert!(gap < 1e-9, "{actual:?} != {expected:?}");
    }

    fn one_shell(solid: &Solid, genus: u32) {
        assert_eq!(solid.shells().len(), 1);
        assert_eq!(solid.shells()[0].genus(), genus);
        assert!(solid.volume() > 0.0);
        let rebuilt = Solid::from_faces(solid.vertices().to_vec(), &solid.to_faces()).unwrap();
        assert_eq!(&rebuilt, solid);
    }

    /// The vertices of the end face: the last section.
    fn end_section(solid: &Solid, vertices: usize) -> &[Point3] {
        let all = solid.vertices();
        &all[all.len() - vertices..]
    }

    /// The profile axes at the end of the path, from the bar's corners 0 (−0.1, −0.05),
    /// 1 (0.1, −0.05) and 2 (0.1, 0.05) in the end section.
    fn end_axes(solid: &Solid) -> (Vector, Vector) {
        let end = end_section(solid, 4);
        let x = scale(sub(coords(end[1]), coords(end[0])), 1.0 / 0.2);
        let y = scale(sub(coords(end[2]), coords(end[1])), 1.0 / 0.1);
        (x, y)
    }

    fn assert_vector(actual: Vector, expected: Vector) {
        assert!(
            length(sub(actual, expected)) < 1e-9,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn a_straight_path_normal_to_the_profile_is_the_extrusion() {
        // Along +z with the profile's y axis along world y: x = y × t = world x, so the sweep
        // places the profile exactly where the extrusion leaves it, and builds the same faces.
        let section = bar();
        let swept = sweep(
            &section,
            vec![line(q(0.0, 0.0, 0.0), q(0.0, 0.0, 3.0))],
            Vector3::new(0.0, 5.0, 0.0),
        );
        let extruded = Solid::extrude(&section, Vector3::new(0.0, 0.0, 3.0)).unwrap();
        assert_eq!(swept, extruded);
        assert_near(swept.volume(), 0.06);
    }

    #[test]
    fn a_straight_path_anywhere_gives_area_times_length() {
        // From (1, 2, 3) by (3, 4, 0): 5 m long, so V = 0.02 · 5 = 0.1 m³, 6 faces.
        let solid = sweep(&bar(), vec![line(q(1.0, 2.0, 3.0), q(4.0, 6.0, 3.0))], Z);
        assert_eq!(solid.face_count(), 6);
        one_shell(&solid, 0);
        assert_near(solid.volume(), 0.1);
        // The profile stands upright: its y axis is world z, its x axis y × t = (−0.8, 0.6, 0).
        let (x, y) = end_axes(&solid);
        assert_vector(x, [-0.8, 0.6, 0.0]);
        assert_vector(y, [0.0, 0.0, 1.0]);
    }

    #[test]
    fn an_l_shaped_path_is_mitred_at_its_corner() {
        // 4 m along +x then 3 m along +y: one mitre at 45°, 2 + 2 · 4 faces. Centred profile:
        // its centroid runs the path, V = 0.02 · 7 = 0.14 m³.
        let corner = vec![
            line(q(0.0, 0.0, 0.0), q(4.0, 0.0, 0.0)),
            line(q(4.0, 0.0, 0.0), q(4.0, 3.0, 0.0)),
        ];
        let solid = sweep(&bar(), corner.clone(), Z);
        assert_eq!(solid.face_count(), 10);
        one_shell(&solid, 0);
        assert_near(solid.volume(), 0.14);
        // Profile x = z × x = +y, towards the inside of the corner. Moved 0.2 m that way (x from
        // 0.1 to 0.3), the centroid runs 0.2 · tan 45° = 0.2 m less on each piece:
        // V = 0.02 · (4 − 0.2 + 3 − 0.2) = 0.132 m³.
        let inside = profile(rectangle(0.1, -0.05, 0.3, 0.05), vec![]);
        assert_near(sweep(&inside, corner, Z).volume(), 0.132);
        // A 60° turn: 4 m then 2 m. Centred, V = 0.02 · 6; moved 0.2 m inside, each piece loses
        // 0.2 · tan 30°.
        let sixty = vec![
            line(q(0.0, 0.0, 0.0), q(4.0, 0.0, 0.0)),
            line(q(4.0, 0.0, 0.0), q(5.0, libm::sqrt(3.0), 0.0)),
        ];
        assert_near(sweep(&bar(), sixty.clone(), Z).volume(), 0.12);
        let loss = 2.0 * 0.2 * libm::tan(PI / 6.0);
        assert_near(sweep(&inside, sixty, Z).volume(), 0.02 * (6.0 - loss));
    }

    #[test]
    fn a_quarter_circle_follows_the_tangent_polygon() {
        // R = 2 m, the bar reaching d = 0.1118 m: n = ⌈(π/2) / (2 acos(1 − s / 2.1128))⌉
        // = ⌈1.5708 / 0.06154⌉ = 26 steps, so 27 pieces and 27 · 4 + 2 = 110 faces. The centroid
        // runs the tangent polygon of radius 2: V = 52 · tan(π/104) · 0.02 · 2 = 0.06290 m³.
        let quarter = vec![circle_arc(0.0, 0.0, 2.0, 0.0, FRAC_PI_2)];
        let solid = sweep(&bar(), quarter.clone(), Z);
        let n = steps(2.0, libm::hypot(0.1, 0.05), FRAC_PI_2);
        assert_eq!(count(n), 26);
        assert_eq!(solid.face_count(), 27 * 4 + 2);
        one_shell(&solid, 0);
        assert_near(solid.volume(), arc_volume(n, FRAC_PI_2, 0.02, 2.0));
        // Pappus gives θ · A · r̄ = (π/2) · 0.02 · 2 = 0.06283 m³. The tangent polygon lies
        // outside the circle, so the faceted volume is larger, by at most the tolerance times
        // the curved area it replaces, the profile's perimeter 0.6 m times the arc's 2.2 · π/2.
        let pappus = FRAC_PI_2 * 0.02 * 2.0;
        let excess = solid.volume() - pappus;
        assert!(excess > 0.0);
        assert!(excess <= ARC_CHORD_TOLERANCE * 0.6 * 2.2 * FRAC_PI_2);
        // Profile x = z × y = −x at the start: towards the centre. Moved 0.2 m that way, the
        // centroid runs at r̄ = 1.8 m, and the profile reaches hypot(0.3, 0.05) from the path.
        let inside = profile(rectangle(0.1, -0.05, 0.3, 0.05), vec![]);
        let solid = sweep(&inside, quarter, Z);
        let n = steps(2.0, libm::hypot(0.3, 0.05), FRAC_PI_2);
        assert_near(solid.volume(), arc_volume(n, FRAC_PI_2, 0.02, 1.8));
        assert!(solid.volume() > FRAC_PI_2 * 0.02 * 1.8);
    }

    #[test]
    fn a_tangent_arc_between_two_lines_adds_no_corner() {
        // 2 m along +x, a quarter circle of radius 1 m around (2, 1), 2 m along +y. The arc's
        // end pieces follow the lines, so the joints are straight: V = 0.02 · (2 + 2 + 2n ·
        // tan(π/4n) · 1), and n + 3 pieces.
        let bend = circle_arc(2.0, 1.0, 1.0, -FRAC_PI_2, 0.0);
        let path = vec![
            line(q(0.0, 0.0, 0.0), bend.start()),
            bend,
            line(bend.end(), q(3.0, 3.0, 0.0)),
        ];
        let solid = sweep(&bar(), path, Z);
        let n = steps(1.0, libm::hypot(0.1, 0.05), FRAC_PI_2);
        assert_eq!(solid.face_count(), (count(n) + 3) * 4 + 2);
        one_shell(&solid, 0);
        let centre_line = 4.0 + arc_volume(n, FRAC_PI_2, 1.0, 1.0);
        assert_near(solid.volume(), 0.02 * centre_line);
    }

    #[test]
    fn a_handrail_stays_upright_up_an_incline() {
        // 3 m level along +x, then 2 m up a 30° slope in the plane y = 0. Start: y = z,
        // x = z × x = +y. The corner turns around −y, the profile's x axis, which stays put;
        // y turns to (−sin 30°, 0, cos 30°), normal to the slope. V = 0.02 · 5 = 0.1 m³.
        let (c, s) = (libm::cos(PI / 6.0), libm::sin(PI / 6.0));
        let path = vec![
            line(q(0.0, 0.0, 0.0), q(3.0, 0.0, 0.0)),
            line(q(3.0, 0.0, 0.0), q(2.0f64.mul_add(c, 3.0), 0.0, 2.0 * s)),
        ];
        let solid = sweep(&bar(), path, Z);
        one_shell(&solid, 0);
        assert_near(solid.volume(), 0.1);
        let (x, y) = end_axes(&solid);
        assert_vector(x, [0.0, 1.0, 0.0]);
        assert_vector(y, [-s, 0.0, c]);
        // The end face is the profile itself, normal to the slope at the path's end.
        let end = end_section(&solid, 4);
        assert_point(
            end[2],
            q(
                2.0f64.mul_add(c, 3.0) - 0.05 * s,
                0.1,
                0.05f64.mul_add(c, 2.0 * s),
            ),
        );
    }

    #[test]
    fn the_profile_keeps_its_tilt_relative_to_a_planar_path() {
        // A quarter circle of radius 2 around the origin from +x to +y, then 3 m along −x. The
        // reference (1, 0, 1) is already normal to the start tangent +y: y = (1, 0, 1)/√2.
        // Parallel transport turns it with the tangent around z, by 90°: (0, 1, 1)/√2, still at
        // 45° to the path's plane.
        let turn = circle_arc(0.0, 0.0, 2.0, 0.0, FRAC_PI_2);
        let path = vec![turn, line(turn.end(), q(-3.0, 2.0, 0.0))];
        let solid = sweep(&bar(), path, Vector3::new(1.0, 0.0, 1.0));
        one_shell(&solid, 0);
        let (x, y) = end_axes(&solid);
        let h = libm::sqrt(0.5);
        assert_vector(y, [0.0, h, h]);
        assert_vector(x, cross(y, [-1.0, 0.0, 0.0]));
    }

    #[test]
    fn a_long_arc_keeps_its_end_face_flat_and_true() {
        // Three quarters of a circle of 200 m radius: n = ⌈4.712 / (2 acos(1 − s/200.11))⌉
        // = 746 steps, 747 pieces, so rounding accumulates over 747 sections. The end face must
        // still lie in the plane normal to the end tangent +x, through (0, −200, 0), and the
        // profile must be the right way up: y = z, x = z × x = +y.
        let solid = sweep(&bar(), vec![circle_arc(0.0, 0.0, 200.0, 0.0, 1.5 * PI)], Z);
        let n = steps(200.0, libm::hypot(0.1, 0.05), 1.5 * PI);
        assert_eq!(count(n), 746);
        assert_eq!(solid.face_count(), 747 * 4 + 2);
        let end = end_section(&solid, 4);
        for corner in end {
            assert!(corner.x.abs() < 1e-9, "{corner:?}");
        }
        let (x, y) = end_axes(&solid);
        assert_vector(x, [0.0, 1.0, 0.0]);
        assert_vector(y, [0.0, 0.0, 1.0]);
        assert_point(end[0], q(0.0, -200.0 - 0.1, -0.05));
        assert_near(solid.volume(), arc_volume(n, 1.5 * PI, 0.02, 200.0));
    }

    #[test]
    fn a_hole_in_the_profile_goes_through_the_solid() {
        // 0.4 m square with a 0.2 m square hole, A = 0.12 m², along the L path: genus 1, 2 + 2
        // · 8 faces, V = 0.12 · 7 = 0.84 m³.
        let tube = profile(
            rectangle(-0.2, -0.2, 0.2, 0.2),
            vec![rectangle(-0.1, -0.1, 0.1, 0.1)],
        );
        let corner = vec![
            line(q(0.0, 0.0, 0.0), q(4.0, 0.0, 0.0)),
            line(q(4.0, 0.0, 0.0), q(4.0, 3.0, 0.0)),
        ];
        let solid = sweep(&tube, corner, Z);
        assert_eq!(solid.face_count(), 18);
        one_shell(&solid, 1);
        assert_near(solid.volume(), 0.84);
        assert_eq!(solid.to_faces()[0].holes.len(), 1);
    }

    /// The pieces and sections of a sweep, before the self-intersection test.
    fn pieces(
        profile: &Profile,
        segments: Vec<PathSegment>,
        reference: Vector3,
    ) -> (Pieces, Vec<Vector>) {
        let path = Path::new(segments).unwrap();
        let (offsets, _, reach) = layout(&profile.faceted().unwrap());
        let pieces = Pieces::new(&path, reach, offsets.len()).unwrap();
        let reference = unit_reference(reference).unwrap();
        let start = pieces.start_section(reference, &offsets).unwrap();
        let sections = pieces.sections(&path, start, offsets.len()).unwrap();
        (pieces, sections)
    }

    #[test]
    fn a_corner_at_the_start_of_a_long_arc_is_tested_against_its_far_end() {
        // A unit circle around the origin in z = 0, from (1, 0, 0) to 0.02 rad short of a full
        // turn, so its end comes back to (0.9998, −0.02, 0). The profile sits 0.1 to 0.2 m above
        // the arc's plane, 0.05 m either side of it radially.
        let almost = TAU - 0.02;
        let ring = circle_arc(0.0, 0.0, 1.0, 0.0, almost);
        // Starting the path, the arc's first piece begins at the radial plane at angle 0: every
        // piece lies in its own wedge, the 0.02 rad (2 cm) gap keeps the ends apart, and the
        // sweep is accepted with no pair of its pieces tested. Profile y = z there.
        let raised = profile(rectangle(-0.05, 0.1, 0.05, 0.2), vec![]);
        one_shell(&sweep(&raised, vec![ring], Z), 0);
        let (alone, _) = pieces(&raised, vec![ring], Z);
        let last = alone.count() - 1;
        assert!(alone.is_radial(0) && alone.is_radial(last));
        // Coming up a line along +z into it, the corner turns +z into +y around −x. The line's
        // frame has y = world y and x = y × z = world x; the turn takes y to −z, so the profile
        // sits at y from −0.2 to −0.1 to be 0.1 to 0.2 m above the arc's plane again. The
        // corner's mitre is the plane y + z = 0: a profile point at height b above the arc's
        // plane starts the arc's first piece at y = −b, reaching 0.1 to 0.2 m back behind the
        // start, where the arc's last piece, at y between −0.06 and −0.02 m and the same
        // heights, already is. That pair really meets: no longer both radial, it is tested and
        // fails. The sweep is rejected (the line, whose mitred end also reaches back, is caught
        // with the arc first).
        let lowered = profile(rectangle(-0.05, -0.2, 0.05, -0.1), vec![]);
        let path = vec![line(q(1.0, 0.0, -1.0), q(1.0, 0.0, 0.0)), ring];
        let up = Vector3::new(0.0, 1.0, 0.0);
        let (cornered, sections) = pieces(&lowered, path.clone(), up);
        let last = cornered.count() - 1;
        assert!(!cornered.is_radial(1) && cornered.is_radial(2) && cornered.is_radial(last));
        let reaches = cornered.reaches();
        assert!(!cornered.clear(1, last, &sections, 4, &reaches));
        // Halfway round, the first piece and the far side of the circle are 2 m apart: clear.
        assert!(cornered.clear(1, last / 2, &sections, 4, &reaches));
        assert!(matches!(
            try_sweep(&lowered, path, up).err(),
            Some(SweepError::SelfIntersecting { .. })
        ));
        // The mirror case: the arc starts the path and a line leaves its end straight up. The
        // corner turns the end tangent, about +y, into +z, its mitre's tip reaching forward, by
        // the profile's depth below the arc's plane, into the first pieces of the arc. With the
        // profile 0.1 to 0.2 m below the plane (y = z at the start), the last arc piece meets
        // the first one: rejected, by that pair, which the end plane of the arc and the start of
        // the path both fail to separate.
        let below = profile(rectangle(-0.05, -0.2, 0.05, -0.1), vec![]);
        let end = ring.end();
        let path = vec![ring, line(end, q(end.x, end.y, 1.0))];
        let (ending, sections) = pieces(&below, path.clone(), Z);
        let last = ending.count() - 2;
        assert!(ending.is_radial(0) && !ending.is_radial(last));
        assert!(!ending.clear(0, last, &sections, 4, &ending.reaches()));
        assert_eq!(
            try_sweep(&below, path, Z).err(),
            Some(SweepError::SelfIntersecting {
                first: 0,
                second: 0
            })
        );
    }

    #[test]
    fn errors_describe_their_cause_without_coordinates() {
        use std::error::Error as _;

        let error = SweepError::Faceting(ProfileError::TooManyEdges);
        assert_eq!(
            error.to_string(),
            "the profile's arcs cannot be replaced by chords"
        );
        assert!(error.source().is_some());
        let error = SweepError::SelfIntersecting {
            first: 2,
            second: 5,
        };
        assert_eq!(
            error.to_string(),
            "the sweep along segments 2 and 5 may cross itself"
        );
        assert_eq!(
            SweepError::Solid(BrepError::Empty).to_string(),
            "the swept solid is invalid"
        );
    }

    #[test]
    fn every_error_is_reported() {
        let straight = || vec![line(q(0.0, 0.0, 0.0), q(1.0, 0.0, 0.0))];
        let run = |reference: Vector3| try_sweep(&bar(), straight(), reference).err();
        for bad in [f64::NAN, f64::INFINITY] {
            assert_eq!(
                run(Vector3::new(0.0, bad, 1.0)),
                Some(SweepError::NonFiniteReference)
            );
        }
        assert_eq!(
            run(Vector3::new(0.0, 0.0, 0.0)),
            Some(SweepError::ZeroReference)
        );
        // Along the path, or tilted from it by less than 1 µm over the bar's 0.11 m reach.
        assert_eq!(
            run(Vector3::new(-2.0, 0.0, 0.0)),
            Some(SweepError::ReferenceAlongPath)
        );
        assert_eq!(
            run(Vector3::new(1.0, 0.0, 5e-6)),
            Some(SweepError::ReferenceAlongPath)
        );
        // Huge and subnormal references are fine.
        for size in [1e300, 5e-324] {
            assert!(run(Vector3::new(0.0, 0.0, size)).is_none());
        }
        // 1 km circle chords exceed the profile's limits while faceting.
        let huge = profile(circle(0.0, 0.0, 9000.0), vec![]);
        assert_eq!(
            try_sweep(&huge, straight(), Z).err(),
            Some(SweepError::Faceting(ProfileError::TooManyEdges))
        );
        // A half circle of 4.5 km: n ≈ π / (2 √(2 s / 4500)) ≈ 2360 steps.
        let wide = vec![circle_arc(0.0, 0.0, 4500.0, 0.0, PI)];
        assert_eq!(
            try_sweep(&bar(), wide, Z).err(),
            Some(SweepError::TooManyPieces)
        );
        // A 2 m round section (100 chords) on three quarters of a 200 m circle: 748 pieces, so
        // 74 800 faces.
        let round = profile(circle(0.0, 0.0, 2.0), vec![]);
        let long = vec![circle_arc(0.0, 0.0, 200.0, 0.0, 1.5 * PI)];
        assert_eq!(
            try_sweep(&round, long, Z).err(),
            Some(SweepError::TooManyFaces)
        );
        // Out 1 m, back 0.5 m.
        let back = vec![
            line(q(0.0, 0.0, 0.0), q(1.0, 0.0, 0.0)),
            line(q(1.0, 0.0, 0.0), q(0.5, 0.0, 0.0)),
        ];
        assert_eq!(
            try_sweep(&bar(), back, Z).err(),
            Some(SweepError::DoublesBack { segment: 1 })
        );
        // A tight U, its 0.1 m middle leg between two left turns of 90°, and a 0.6 m wide
        // profile: each corner takes 0.3 m off the inner side of the leg, 0.1 − 2 · 0.3 < 0.
        // (A Z would not: its two mitres are parallel, and the leg keeps its length.)
        let wide = profile(rectangle(-0.3, -0.05, 0.3, 0.05), vec![]);
        let hairpin = vec![
            line(q(0.0, 0.0, 0.0), q(1.0, 0.0, 0.0)),
            line(q(1.0, 0.0, 0.0), q(1.0, 0.1, 0.0)),
            line(q(1.0, 0.1, 0.0), q(0.0, 0.1, 0.0)),
        ];
        assert_eq!(
            try_sweep(&wide, hairpin, Z).err(),
            Some(SweepError::MitresCross { segment: 1 })
        );
        // A path that crosses its first leg: 2 m along +x, 1 m along +y, 1 m along −x, then down
        // through the first leg at x = 1.
        let crossing = vec![
            line(q(0.0, 0.0, 0.0), q(2.0, 0.0, 0.0)),
            line(q(2.0, 0.0, 0.0), q(2.0, 1.0, 0.0)),
            line(q(2.0, 1.0, 0.0), q(1.0, 1.0, 0.0)),
            line(q(1.0, 1.0, 0.0), q(1.0, -1.0, 0.0)),
        ];
        assert_eq!(
            try_sweep(&bar(), crossing, Z).err(),
            Some(SweepError::SelfIntersecting {
                first: 0,
                second: 3
            })
        );
        // Swept 9.9 km out, the bar's edge passes the model extent.
        let far = vec![line(
            q(MODEL_EXTENT - 0.05, 0.0, 0.0),
            q(MODEL_EXTENT - 0.05, 1.0, 0.0),
        )];
        assert!(matches!(
            try_sweep(&bar(), far, Z).err(),
            Some(SweepError::Solid(BrepError::OutsideModelExtent { .. }))
        ));
    }

    #[test]
    fn arcs_too_tight_or_closing_on_themselves_are_refused() {
        // A 0.1 m radius arc around (−0.1, 0, 0), entered from a tangent line along +y. With
        // reference x, profile y = x points away from the centre and x = x × y = z across the
        // arc's plane: the bar reaches only its 0.05 m half height towards the centre, accepted.
        let tight_arc = circle_arc(-0.1, 0.0, 0.1, 0.0, FRAC_PI_2);
        let tight = vec![line(q(0.0, -1.0, 0.0), tight_arc.start()), tight_arc];
        let ok = try_sweep(&bar(), tight.clone(), Vector3::new(1.0, 0.0, 0.0));
        assert!(ok.is_ok(), "{ok:?}");
        // With reference z, profile x = z × y = −x points to the centre: the bar's 0.1 m half
        // width reaches R − tolerance, and the inner side would fold.
        assert_eq!(
            try_sweep(&bar(), tight, Z).err(),
            Some(SweepError::ArcTooTight { segment: 1 })
        );
        // A profile 5 µm from the same arc's axis: farther than the tolerance, but at
        // R + d + s = 0.213 m the arc takes 9 steps of h = 0.087 rad, and that point would run
        // only 5e-6 · tan h = 0.44 µm along each half piece. Too tight, not a corner.
        let tight = vec![line(q(0.0, -1.0, 0.0), tight_arc.start()), tight_arc];
        let close = profile(rectangle(0.0, -0.05, 0.1 - 5e-6, 0.05), vec![]);
        assert_eq!(
            try_sweep(&close, tight, Z).err(),
            Some(SweepError::ArcTooTight { segment: 1 })
        );
        // An arc 2 µrad short of a full turn of radius 1 m, its ends 2 µm apart, with the profile
        // reaching to 0.1 mm from the centre: there the end faces are 2e-10 m apart.
        let almost = vec![circle_arc(0.0, 0.0, 1.0, 0.0, TAU - 2e-6)];
        let inner = profile(rectangle(0.5, -0.05, 0.9999, 0.05), vec![]);
        assert_eq!(
            try_sweep(&inner, almost, Z).err(),
            Some(SweepError::SelfIntersecting {
                first: 0,
                second: 0
            })
        );
    }

    #[test]
    fn a_nearly_straight_arc_takes_one_step() {
        // A 15 km chord with a 2 µm sagitta: R = c² / 8s = 1.4e13 m, where 1 − s/r rounds to 1.
        // One step does: two half pieces, nearly along the chord. V = 0.02 · 15 000 m³, to well
        // within the 4e-10 relative length the polygon adds.
        let flat = vec![arc(
            q(-7500.0, 0.0, 0.0),
            q(0.0, 2e-6, 0.0),
            q(7500.0, 0.0, 0.0),
        )];
        let solid = sweep(&bar(), flat, Z);
        assert_eq!(solid.face_count(), 2 * 4 + 2);
        one_shell(&solid, 0);
        assert_near(solid.volume(), 300.0);
    }

    #[test]
    fn a_u_turn_with_legs_apart_is_accepted() {
        // 2 m along +x, 1 m along +y, 2 m back along −x: the legs are 1 m apart, the bar 0.2 m
        // wide; they are farther apart than both reaches (0.1118 / cos 45° each), so the
        // distance test passes. V = 0.02 · 5 = 0.1 m³.
        let u = vec![
            line(q(0.0, 0.0, 0.0), q(2.0, 0.0, 0.0)),
            line(q(2.0, 0.0, 0.0), q(2.0, 1.0, 0.0)),
            line(q(2.0, 1.0, 0.0), q(0.0, 1.0, 0.0)),
        ];
        let solid = sweep(&bar(), u, Z);
        one_shell(&solid, 0);
        assert_near(solid.volume(), 0.1);
    }

    #[test]
    fn segment_distance_handles_every_configuration() {
        let d = |a: [Vector; 2], b: [Vector; 2]| segment_distance(a, b);
        let x = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        // Crossing above: 1 apart.
        assert_near(d(x, [[1.0, -1.0, 1.0], [1.0, 1.0, 1.0]]), 1.0);
        // Parallel, overlapping.
        assert_near(d(x, [[1.0, 3.0, 0.0], [5.0, 3.0, 0.0]]), 3.0);
        // The closest point of the second beyond its start, then beyond its end.
        assert_near(d(x, [[-1.0, 1.0, 0.0], [-1.0, 2.0, 0.0]]), libm::sqrt(2.0));
        assert_near(d(x, [[3.0, 2.0, 0.0], [3.0, 1.0, 0.0]]), libm::sqrt(2.0));
    }
}
