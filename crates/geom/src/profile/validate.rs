//! Validity checks of profile loops: closure, degenerate edges, crossings and nesting.
//!
//! Following the tolerance policy, decisions on two straight edges are exact (orient2d). A pair
//! involving an arc cannot be decided by the predicates, so it is decided in floating point and
//! rejected when the edges come within [`LINEAR_TOLERANCE`](crate::LINEAR_TOLERANCE), read as
//! the solids' checks read it (at most the tolerance, up to rounding): near-touching profiles
//! fail instead of producing slivers. Rationale: `docs/dev/adr/0005-profile-validation-with-arcs.md`.

use core::f64::consts::{PI, TAU};

use crate::profile::edge::{Arc, Curve, Edge, cross, distance, dot, length, sub};
use crate::profile::{LoopId, ProfileError};
use crate::tolerance::{WITHIN_TOLERANCE, is_model_coordinate};
use crate::{Orientation, Point2, orient2d};

/// Checks one loop on its own (coordinates, closure and degenerate edges) and returns the
/// geometry of its edges for the checks between edges.
pub(crate) fn check_loop(id: LoopId, edges: &[Edge]) -> Result<Vec<Curve>, ProfileError> {
    if edges.is_empty() {
        return Err(ProfileError::EmptyLoop(id));
    }
    for (edge, e) in edges.iter().enumerate() {
        for value in e.points().flat_map(|p| [p.x, p.y]) {
            if !value.is_finite() {
                return Err(ProfileError::NonFinite { loop_id: id, edge });
            }
            if !is_model_coordinate(value) {
                return Err(ProfileError::OutOfRange { loop_id: id, edge });
            }
        }
    }
    edges
        .iter()
        .enumerate()
        .map(|(edge, e)| {
            if e.end() != edges[(edge + 1) % edges.len()].start() {
                return Err(ProfileError::Open { loop_id: id, edge });
            }
            let curve = e.curve();
            check_curve(&curve, id, edge)?;
            Ok(curve)
        })
        .collect()
}

fn check_curve(curve: &Curve, loop_id: LoopId, edge: usize) -> Result<(), ProfileError> {
    match curve {
        Curve::Segment(a, b) if a == b => Err(ProfileError::ZeroLengthLine { loop_id, edge }),
        Curve::Segment(..) => Ok(()),
        Curve::Arc(arc) => {
            // Distinct ends at the same angle sweep nothing counterclockwise and a full turn
            // clockwise; either way the arc is not what its ends describe.
            let same_angle = !arc.is_full_circle() && (arc.sweep == 0.0 || arc.sweep.abs() >= TAU);
            // The checks between consecutive edges ignore points within the tolerance of their
            // common end, so an arc must reach beyond it: its ends must not merge under the
            // tolerance policy, and its middle, the point farthest from the nearer end, must lie
            // outside the tolerance of them. Otherwise they could miss the arc retracing or
            // crossing its neighbour.
            let within_ends = !arc.is_full_circle()
                && (distance(arc.start, arc.end) <= WITHIN_TOLERANCE
                    || distance(arc.middle, arc.start) <= WITHIN_TOLERANCE);
            if (distance(arc.end, arc.centre) - arc.radius).abs() > WITHIN_TOLERANCE {
                Err(ProfileError::ArcRadiusMismatch { loop_id, edge })
            } else if arc.radius <= WITHIN_TOLERANCE || same_angle || within_ends {
                Err(ProfileError::DegenerateArc { loop_id, edge })
            } else {
                Ok(())
            }
        }
    }
}

/// Checks that no two edges of the same loop meet, except consecutive edges at their common end.
pub(crate) fn check_simple(id: LoopId, curves: &[Curve]) -> Result<(), ProfileError> {
    let n = curves.len();
    let intersecting = |i: usize, j: usize| ProfileError::SelfIntersecting {
        loop_id: id,
        first: i.min(j),
        second: i.max(j),
    };
    if n > 1 {
        // A full circle returns to its start, where the next edge also starts: the loop pinches.
        if let Some(i) = curves
            .iter()
            .position(|c| matches!(c, Curve::Arc(arc) if arc.is_full_circle()))
        {
            return Err(intersecting(i, (i + 1) % n));
        }
    }
    for i in 0..n {
        for j in i + 1..n {
            let ends = [curves[i].end(), curves[0].start()];
            let shared: &[Point2] = match (j == i + 1, i == 0 && j == n - 1) {
                (true, true) => &ends,
                (true, false) => &ends[..1],
                (false, true) => &ends[1..],
                (false, false) => &[],
            };
            if curves_meet(&curves[i], &curves[j], shared) {
                return Err(intersecting(i, j));
            }
        }
    }
    Ok(())
}

/// Whether two different loops meet, or come within the tolerance where arcs are involved.
pub(crate) fn loops_meet(a: &[Curve], b: &[Curve]) -> bool {
    a.iter()
        .any(|ca| b.iter().any(|cb| curves_meet(ca, cb, &[])))
}

/// Whether the region enclosed by a closed, non-self-intersecting loop contains `p`.
///
/// `p` must not lie on the loop. The angles `p` sees each edge under add up to ±2π inside and to
/// 0 outside; only their sum's distance from 0 matters, so rounding cannot change the answer.
pub(crate) fn encloses(curves: &[Curve], p: Point2) -> bool {
    let total: f64 = curves
        .iter()
        .map(|curve| match *curve {
            Curve::Segment(a, b) => chord_angle(a, b, p, orient(a, b, p)),
            Curve::Arc(ref arc) => arc_angle(arc, p, true),
        })
        .sum();
    total.abs() > PI
}

/// Exact orientation; coordinates were range-checked by [`check_loop`], so the predicate cannot
/// refuse them.
fn orient(a: Point2, b: Point2, c: Point2) -> Orientation {
    orient2d(a, b, c).unwrap_or(Orientation::Zero)
}

fn sign(o: Orientation) -> f64 {
    match o {
        Orientation::Positive => 1.0,
        Orientation::Negative => -1.0,
        Orientation::Zero => 0.0,
    }
}

/// Signed angle under which `p` sees the segment `a → b`; `side` is the exact orientation of
/// `a, b, p`, so the sign is right even when `p` is very close to the segment's line.
fn chord_angle(a: Point2, b: Point2, p: Point2, side: Orientation) -> f64 {
    let (pa, pb) = (sub(a, p), sub(b, p));
    sign(side) * libm::atan2(cross(pa, pb).abs(), dot(pa, pb))
}

/// Signed angle under which `p` sees an arc: that of its chord, plus a full turn when `p` lies in
/// the circular segment between chord and arc.
fn arc_angle(arc: &Arc, p: Point2, may_split: bool) -> f64 {
    let inside_circle = distance(p, arc.centre) < arc.radius;
    if arc.is_full_circle() {
        return if inside_circle { arc.sweep } else { 0.0 };
    }
    let side = orient(arc.start, arc.end, p);
    if side == Orientation::Zero && may_split {
        // On the chord's line the chord angle is ±π or 0 and its sign is undecided; the chords of
        // the two halves do not pass through `p`.
        let (first, second) = arc.split();
        return arc_angle(&first, p, false) + arc_angle(&second, p, false);
    }
    // A counterclockwise arc bulges to the right of its chord.
    let bulge = if arc.sweep > 0.0 {
        Orientation::Negative
    } else {
        Orientation::Positive
    };
    let segment_turn = if inside_circle && side == bulge {
        TAU.copysign(arc.sweep)
    } else {
        0.0
    };
    chord_angle(arc.start, arc.end, p, side) + segment_turn
}

/// Whether two edges meet or, when an arc is involved, come within the tolerance. Points
/// within the tolerance of a `shared` end of consecutive edges are not counted.
fn curves_meet(a: &Curve, b: &Curve, shared: &[Point2]) -> bool {
    if boxes_apart(a.bounds(), b.bounds()) {
        return false;
    }
    match (a, b, shared) {
        (Curve::Segment(a0, a1), Curve::Segment(b0, b1), []) => segments_touch(*a0, *a1, *b0, *b1),
        (Curve::Segment(a0, a1), Curve::Segment(b0, b1), [v, ..]) => {
            let other_a = if a0 == v { *a1 } else { *a0 };
            let other_b = if b0 == v { *b1 } else { *b0 };
            folds_back(other_a, *v, other_b)
        }
        _ => {
            shared.iter().any(|&v| leave_together(a, b, v))
                || clearance(a, b, shared) <= WITHIN_TOLERANCE
        }
    }
}

/// Whether consecutive edges leave their common end `v` in the same direction, so closely that
/// the turn alone keeps them within the tolerance along the shorter one: a cusp. Its sliver
/// starts among the points near `v` that [`clearance`] ignores, so it may find no point of it.
fn leave_together(a: &Curve, b: &Curve, v: Point2) -> bool {
    let (along_a, along_b) = (departure(a, v), departure(b, v));
    let reach = length_of(a).min(length_of(b));
    dot(along_a, along_b) > 0.0 && cross(along_a, along_b).abs() * reach <= WITHIN_TOLERANCE
}

/// The length of an edge along itself: an arc's is longer than its chord, much longer for an
/// arc of almost a full turn.
fn length_of(curve: &Curve) -> f64 {
    match curve {
        Curve::Segment(start, end) => distance(*start, *end),
        Curve::Arc(arc) => arc.radius * arc.sweep.abs(),
    }
}

/// The unit direction in which `curve` leaves its end `v`.
fn departure(curve: &Curve, v: Point2) -> Point2 {
    let travel = match curve {
        Curve::Segment(start, end) => sub(*end, *start),
        Curve::Arc(arc) => {
            let radial = sub(v, arc.centre);
            let turn = arc.sweep.signum();
            Point2::new(-radial.y * turn, radial.x * turn)
        }
    };
    let direction = unit(travel);
    if v == curve.start() {
        direction
    } else {
        Point2::new(-direction.x, -direction.y)
    }
}

/// Whether the boxes are more than the tolerance apart, so their curves can neither meet nor
/// come close; this skips the full test for most pairs.
fn boxes_apart(a: [f64; 4], b: [f64; 4]) -> bool {
    let [a_min_x, a_min_y, a_max_x, a_max_y] = a;
    let [b_min_x, b_min_y, b_max_x, b_max_y] = b;
    a_min_x > b_max_x + WITHIN_TOLERANCE
        || b_min_x > a_max_x + WITHIN_TOLERANCE
        || a_min_y > b_max_y + WITHIN_TOLERANCE
        || b_min_y > a_max_y + WITHIN_TOLERANCE
}

/// Exact test of whether the closed segments `a0 a1` and `b0 b1` share a point.
fn segments_touch(a0: Point2, a1: Point2, b0: Point2, b1: Point2) -> bool {
    let (o1, o2) = (orient(a0, a1, b0), orient(a0, a1, b1));
    if o1 == Orientation::Zero && o2 == Orientation::Zero {
        let (a_low, a_high) = ordered(a0, a1);
        let (b_low, b_high) = ordered(b0, b1);
        return lex_le(a_low, b_high) && lex_le(b_low, a_high);
    }
    let (o3, o4) = (orient(b0, b1, a0), orient(b0, b1, a1));
    !same_strict_side(o1, o2) && !same_strict_side(o3, o4)
}

/// Whether consecutive segments `a → v → b` overlap: `b` goes back along `a`.
fn folds_back(a: Point2, v: Point2, b: Point2) -> bool {
    orient(a, v, b) == Orientation::Zero && lex_lt(a, v) == lex_lt(b, v)
}

fn same_strict_side(o1: Orientation, o2: Orientation) -> bool {
    o1 == o2 && o1 != Orientation::Zero
}

/// Lexicographic order; on a line it orders collinear points along the line, exactly.
fn lex_lt(p: Point2, q: Point2) -> bool {
    (p.x, p.y) < (q.x, q.y)
}

fn lex_le(p: Point2, q: Point2) -> bool {
    !lex_lt(q, p)
}

fn ordered(p: Point2, q: Point2) -> (Point2, Point2) {
    if lex_lt(q, p) { (q, p) } else { (p, q) }
}

/// An upper bound of the distance between two edges, exact up to rounding: the smallest
/// `d(q, a) + d(q, b)` over the points `q` where the distance can be smallest (ends, crossings
/// and points along common normals). Points near a `shared` end are skipped.
fn clearance(a: &Curve, b: &Curve, shared: &[Point2]) -> f64 {
    candidates(a, b)
        .filter(|q| shared.iter().all(|v| distance(*q, *v) > WITHIN_TOLERANCE))
        .map(|q| a.distance_to(q) + b.distance_to(q))
        .fold(f64::INFINITY, f64::min)
}

fn candidates(a: &Curve, b: &Curve) -> impl Iterator<Item = Point2> {
    // An arc's middle catches arcs retracing an arc of the same circle, which have no crossings
    // or common normals to offer.
    let middles = [a, b].map(|curve| match curve {
        Curve::Arc(arc) => Some(arc.middle),
        Curve::Segment(..) => None,
    });
    let mut special = [None; 6];
    match (a, b) {
        (Curve::Segment(s0, s1), Curve::Arc(arc)) | (Curve::Arc(arc), Curve::Segment(s0, s1)) => {
            let direction = sub(*s1, *s0);
            let [n0, n1] = arc.antipodes(unit(Point2::new(-direction.y, direction.x)));
            let [c0, c1] = line_circle(*s0, *s1, arc);
            special[..4].copy_from_slice(&[Some(n0), Some(n1), c0, c1]);
        }
        (Curve::Arc(first), Curve::Arc(second)) => {
            let axis = sub(second.centre, first.centre);
            let gap = length(axis);
            if gap > 0.0 {
                let toward = Point2::new(axis.x / gap, axis.y / gap);
                let [f0, f1] = first.antipodes(toward);
                let [s0, s1] = second.antipodes(toward);
                let [c0, c1] = circle_circle(first, second, gap, toward);
                special = [Some(f0), Some(f1), Some(s0), Some(s1), c0, c1];
            }
        }
        // Two segments are decided exactly by `curves_meet`.
        (Curve::Segment(..), Curve::Segment(..)) => {}
    }
    [a.start(), a.end(), b.start(), b.end()]
        .into_iter()
        .chain(middles.into_iter().flatten())
        .chain(special.into_iter().flatten())
}

fn unit(v: Point2) -> Point2 {
    let l = length(v);
    Point2::new(v.x / l, v.y / l)
}

/// Points where the segment `s0 s1` crosses the circle of `arc`.
fn line_circle(s0: Point2, s1: Point2, arc: &Arc) -> [Option<Point2>; 2] {
    // |s0 + t·along − centre|² = r², a quadratic in t with half linear coefficient `half_b`.
    let along = sub(s1, s0);
    let from_centre = sub(s0, arc.centre);
    let quadratic = dot(along, along);
    let half_b = dot(from_centre, along);
    let constant = arc
        .radius
        .mul_add(-arc.radius, dot(from_centre, from_centre));
    let discriminant = half_b.mul_add(half_b, -(quadratic * constant));
    if discriminant < 0.0 {
        return [None; 2];
    }
    let root = discriminant.sqrt();
    [(-half_b - root) / quadratic, (-half_b + root) / quadratic].map(|t| {
        (0.0..=1.0)
            .contains(&t)
            .then(|| Point2::new(t.mul_add(along.x, s0.x), t.mul_add(along.y, s0.y)))
    })
}

/// Points where the circles of two arcs cross; their centres are `d > 0` apart along `u`.
fn circle_circle(p: &Arc, q: &Arc, d: f64, u: Point2) -> [Option<Point2>; 2] {
    if d > p.radius + q.radius || d < (p.radius - q.radius).abs() {
        return [None; 2];
    }
    let along = (p.radius * p.radius - q.radius * q.radius + d * d) / (2.0 * d);
    let across = p.radius.mul_add(p.radius, -(along * along)).max(0.0).sqrt();
    let foot = Point2::new(
        along.mul_add(u.x, p.centre.x),
        along.mul_add(u.y, p.centre.y),
    );
    [
        Some(Point2::new(
            across.mul_add(-u.y, foot.x),
            across.mul_add(u.x, foot.y),
        )),
        Some(Point2::new(
            across.mul_add(u.y, foot.x),
            across.mul_add(-u.x, foot.y),
        )),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::tests::{bulged_square, ccw, circle, line, p, polygon};

    fn curves(edges: &[Edge]) -> Vec<Curve> {
        edges.iter().map(Edge::curve).collect()
    }

    fn reversed(edges: &[Edge]) -> Vec<Edge> {
        edges.iter().rev().map(Edge::reversed).collect()
    }

    #[test]
    fn segments_touch_when_crossing_meeting_or_overlapping() {
        let (o, x) = (p(0.0, 0.0), p(2.0, 0.0));
        assert!(segments_touch(o, x, p(1.0, -1.0), p(1.0, 1.0)));
        assert!(segments_touch(o, x, p(1.0, 0.0), p(1.0, 1.0)));
        assert!(segments_touch(o, x, p(2.0, 0.0), p(3.0, 0.0)));
        assert!(segments_touch(o, x, p(3.0, 0.0), p(1.0, 0.0)));
        assert!(!segments_touch(o, x, p(2.5, 0.0), p(3.0, 0.0)));
        assert!(!segments_touch(o, x, p(1.0, 1e-9), p(1.0, 1.0)));
        assert!(!segments_touch(o, x, p(3.0, -1.0), p(3.0, 1.0)));
    }

    #[test]
    fn consecutive_segments_fold_back_only_when_going_back_along_the_line() {
        let v = p(1.0, 1.0);
        assert!(folds_back(p(0.0, 0.0), v, p(0.5, 0.5)));
        assert!(folds_back(p(0.0, 0.0), v, p(-1.0, -1.0)));
        assert!(!folds_back(p(0.0, 0.0), v, p(2.0, 2.0)));
        assert!(!folds_back(p(0.0, 0.0), v, p(2.0, 1.0)));
        assert!(folds_back(p(1.0, 0.0), v, p(1.0, 0.5)));
    }

    #[test]
    fn distant_boxes_are_apart_and_touching_ones_are_not() {
        let unit = [0.0, 0.0, 1.0, 1.0];
        assert!(boxes_apart(unit, [1.0 + 2e-6, 0.0, 2.0, 1.0]));
        assert!(boxes_apart([0.0, 1.0 + 2e-6, 1.0, 2.0], unit));
        assert!(!boxes_apart(unit, [1.0 + 5e-7, 0.0, 2.0, 1.0]));
        assert!(!boxes_apart(unit, [0.5, 0.5, 2.0, 2.0]));
    }

    #[test]
    fn encloses_points_of_polygons() {
        let square = polygon(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]);
        assert!(encloses(&curves(&square), p(1.0, 1.0)));
        assert!(!encloses(&curves(&square), p(3.0, 1.0)));
        assert!(!encloses(&curves(&square), p(3.0, 0.0)));
        assert!(encloses(&curves(&reversed(&square)), p(1.0, 1.0)));
    }

    #[test]
    fn encloses_points_of_circles_and_circular_segments() {
        assert!(encloses(&curves(&circle(0.0, 0.0, 1.0)), p(0.5, 0.5)));
        assert!(!encloses(&curves(&circle(0.0, 0.0, 1.0)), p(1.0, 1.0)));
        // A half disc above the x axis, as an arc and its diameter.
        let half = [
            ccw(p(1.0, 0.0), p(-1.0, 0.0), p(0.0, 0.0)),
            line(p(-1.0, 0.0), p(1.0, 0.0)),
        ];
        let counterclockwise = curves(&half);
        assert!(encloses(&counterclockwise, p(0.0, 0.5)));
        assert!(!encloses(&counterclockwise, p(0.0, -0.5)));
        assert!(!encloses(&counterclockwise, p(0.0, 1.5)));
        assert!(!encloses(&counterclockwise, p(2.0, 0.0)));
        let clockwise = curves(&reversed(&half));
        assert!(encloses(&clockwise, p(0.0, 0.5)));
        assert!(!encloses(&clockwise, p(0.0, -0.5)));
    }

    #[test]
    fn encloses_points_on_the_line_of_an_arc_chord() {
        let bulged = curves(&bulged_square());
        assert!(encloses(&bulged, p(1.0, 2.0)));
        assert!(!encloses(&bulged, p(3.0, 2.0)));
    }

    #[test]
    fn arcs_meet_lines_they_cross_or_approach() {
        let quarter = ccw(p(1.0, 0.0), p(0.0, 1.0), p(0.0, 0.0)).curve();
        let crossing = Curve::Segment(p(0.0, 0.0), p(2.0, 2.0));
        assert!(curves_meet(&quarter, &crossing, &[]));
        assert!(curves_meet(&crossing, &quarter, &[]));
        let near = Curve::Segment(p(1.0 + 5e-7, -1.0), p(1.0 + 5e-7, 1.0));
        assert!(curves_meet(&quarter, &near, &[]));
        let apart = Curve::Segment(p(1.0 + 2e-6, -1.0), p(1.0 + 2e-6, 1.0));
        assert!(!curves_meet(&quarter, &apart, &[]));
        let below = Curve::Segment(p(-1.0, -0.5), p(1.0, -0.5));
        assert!(!curves_meet(&quarter, &below, &[]));
    }

    #[test]
    fn arcs_meet_arcs_they_cross_approach_or_overlap() {
        let arc = |centre, start, end| ccw(start, end, centre).curve();
        let a = arc(p(0.0, 0.0), p(1.0, 0.0), p(-1.0, 0.0));
        let crossing = arc(p(1.0, 0.0), p(2.0, 0.0), p(0.0, 0.0));
        assert!(curves_meet(&a, &crossing, &[]));
        let touching = arc(p(2.0, 0.0), p(3.0, 0.0), p(1.0, 0.0));
        assert!(curves_meet(&a, &touching, &[]));
        let near = arc(p(2.0 + 5e-7, 0.0), p(3.0 + 5e-7, 0.0), p(1.0 + 5e-7, 0.0));
        assert!(curves_meet(&a, &near, &[]));
        let far = arc(p(5.0, 0.0), p(6.0, 0.0), p(4.0, 0.0));
        assert!(!curves_meet(&a, &far, &[]));
        let inside = arc(p(0.0, 0.0), p(0.5, 0.0), p(-0.5, 0.0));
        assert!(!curves_meet(&a, &inside, &[]));
        let same_circle = arc(p(0.0, 0.0), p(0.0, 1.0), p(0.0, -1.0));
        assert!(curves_meet(&a, &same_circle, &[]));
    }

    #[test]
    fn consecutive_edges_may_only_meet_at_their_common_end() {
        let v = p(1.0, 0.0);
        let tangent = ccw(v, p(0.0, 1.0), p(0.0, 0.0)).curve();
        assert!(!curves_meet(
            &Curve::Segment(p(1.0, -1.0), v),
            &tangent,
            &[v]
        ));
        // A line ending on the circle and crossing it a second time inside the arc's span.
        let chord = Curve::Segment(p(0.0, 1.0), v);
        let long = ccw(v, p(0.0, -1.0), p(0.0, 0.0)).curve();
        assert!(curves_meet(&chord, &long, &[v]));
        assert!(!curves_meet(&chord, &long, &[v, p(0.0, 1.0)]));
    }
}
