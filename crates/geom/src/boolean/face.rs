//! The working set of a boolean: vertices of both operands plus constructed ones, and faces with
//! the plane they keep from their input face.

use crate::predicates::{MIN_COORDINATE, orient2d};
use crate::tolerance::WITHIN_TOLERANCE;
use crate::vector::{
    Vector, add, bounds, coords, cross, dot, largest_axis, length, newell, scale, sub, unit,
};
use crate::{Orientation, Point2, Solid};

/// Every vertex of a boolean: those of the operands first, then the constructed ones.
pub(super) struct Pool {
    points: Vec<Vector>,
    inputs: usize,
}

impl Pool {
    pub(super) fn new(a: &Solid, b: &Solid) -> Self {
        let points: Vec<Vector> = a
            .vertices()
            .iter()
            .chain(b.vertices())
            .map(|&p| coords(p))
            .collect();
        Self {
            inputs: points.len(),
            points,
        }
    }

    /// A pool of given points, all from operands.
    #[cfg(test)]
    pub(super) fn of(points: Vec<Vector>) -> Self {
        Self {
            inputs: points.len(),
            points,
        }
    }

    pub(super) fn get(&self, vertex: usize) -> Vector {
        self.points[vertex]
    }

    pub(super) fn len(&self) -> usize {
        self.points.len()
    }

    /// Whether a vertex comes from an operand rather than from an intersection.
    pub(super) const fn is_input(&self, vertex: usize) -> bool {
        vertex < self.inputs
    }

    /// Adds a constructed vertex, flushed like a moved one.
    pub(super) fn push(&mut self, point: Vector) -> usize {
        self.points.push(flushed(point));
        self.points.len() - 1
    }

    /// Moves a vertex. Components too small for the exact predicates are flushed to zero: they
    /// are far below the tolerance, and the solid's checks would reject them.
    pub(super) fn set(&mut self, vertex: usize, point: Vector) {
        self.points[vertex] = flushed(point);
    }

    /// Adds a copy of a vertex, for a solid that touches itself there.
    pub(super) fn duplicate(&mut self, vertex: usize) -> usize {
        self.points.push(self.points[vertex]);
        self.points.len() - 1
    }
}

fn flushed(point: Vector) -> Vector {
    point.map(|c| if c.abs() < MIN_COORDINATE { 0.0 } else { c })
}

/// A projection of a plane onto two coordinate axes, keeping the orientation seen from the side
/// its normal points to: rings turning counter-clockwise there have a positive area.
#[derive(Clone, Copy, Debug)]
pub(super) struct Projection {
    /// The two kept axes, then the dropped one.
    axes: [usize; 3],
}

impl Projection {
    /// Drops the axis along which the normal is largest, so the projection never degenerates.
    pub(super) fn along(normal: Vector) -> Self {
        let k = largest_axis(normal);
        let (u, v) = ((k + 1) % 3, (k + 2) % 3);
        let axes = if normal[k] > 0.0 {
            [u, v, k]
        } else {
            [v, u, k]
        };
        Self { axes }
    }

    pub(super) const fn apply(self, p: Vector) -> Point2 {
        Point2::new(p[self.axes[0]], p[self.axes[1]])
    }
}

/// A face of an operand, keeping the plane of its input face (ADR 0001).
pub(super) struct PlanarFace {
    /// The outer ring first, then the holes, as vertices of the pool.
    pub(super) rings: Vec<Vec<usize>>,
    /// The unit normal, pointing out of the solid.
    pub(super) normal: Vector,
    centre: Vector,
    pub(super) projection: Projection,
    min: Vector,
    max: Vector,
}

impl PlanarFace {
    pub(super) fn new(rings: Vec<Vec<usize>>, pool: &Pool) -> Self {
        let (normal, centre) = plane(&rings, pool);
        let mut face = Self {
            rings,
            normal,
            centre,
            projection: Projection::along(normal),
            min: [0.0; 3],
            max: [0.0; 3],
        };
        face.update_bounds(pool);
        face
    }

    /// Renames its vertices, keeping its plane. Edges that collapse are dropped, then rings
    /// left with fewer than three vertices.
    pub(super) fn remap(&mut self, rename: &[usize], pool: &Pool) {
        for ring in &mut self.rings {
            for v in ring.iter_mut() {
                *v = rename[*v];
            }
        }
        clean_face(&mut self.rings);
        self.update_bounds(pool);
    }

    fn update_bounds(&mut self, pool: &Pool) {
        let points = self.rings.iter().flatten().map(|&v| pool.get(v));
        (self.min, self.max) = bounds(points).unwrap_or((self.centre, self.centre));
    }

    /// The centre of its vertices, a point of its plane.
    pub(super) const fn centre(&self) -> Vector {
        self.centre
    }

    /// The signed distance of a point from the plane, positive outside.
    pub(super) fn distance(&self, p: Vector) -> f64 {
        dot(self.normal, sub(p, self.centre))
    }

    /// Where a vertex lies relative to the plane: within the tolerance counts as on it.
    pub(super) fn side(&self, p: Vector) -> i8 {
        let d = self.distance(p);
        if d > WITHIN_TOLERANCE {
            1
        } else if d < -WITHIN_TOLERANCE {
            -1
        } else {
            0
        }
    }

    /// Whether every vertex of this face lies on the plane of `other`, within the tolerance.
    pub(super) fn lies_on(&self, other: &Self, pool: &Pool) -> bool {
        self.rings
            .iter()
            .flatten()
            .all(|&v| other.side(pool.get(v)) == 0)
    }

    /// Whether the faces lie in one plane, within the tolerance: one of them lies on the
    /// other's plane.
    pub(super) fn coplanar(&self, other: &Self, pool: &Pool) -> bool {
        self.lies_on(other, pool) || other.lies_on(self, pool)
    }

    pub(super) const fn min_x(&self) -> f64 {
        self.min[0]
    }

    pub(super) const fn max_x(&self) -> f64 {
        self.max[0]
    }

    /// Whether the bounding boxes, grown by the tolerance, overlap.
    pub(super) fn overlaps(&self, other: &Self) -> bool {
        (0..3).all(|i| {
            self.min[i] <= other.max[i] + WITHIN_TOLERANCE
                && other.min[i] <= self.max[i] + WITHIN_TOLERANCE
        })
    }

    /// Whether the bounding box, grown by the tolerance, contains a point.
    pub(super) fn reaches(&self, p: Vector) -> bool {
        (0..3).all(|i| {
            self.min[i] - WITHIN_TOLERANCE <= p[i] && p[i] <= self.max[i] + WITHIN_TOLERANCE
        })
    }

    /// The point of the plane that projects onto `q`.
    pub(super) fn lift(&self, point: Point2) -> Vector {
        let [first, second, dropped] = self.projection.axes;
        let (normal, centre) = (self.normal, self.centre);
        let mut lifted = [0.0; 3];
        lifted[first] = point.x;
        lifted[second] = point.y;
        lifted[dropped] = centre[dropped]
            - (normal[first] * (point.x - centre[first])
                + normal[second] * (point.y - centre[second]))
                / normal[dropped];
        lifted
    }

    /// Whether a point of the plane, projected, lies inside the face: exact even–odd test.
    pub(super) fn contains(&self, pool: &Pool, q: Point2) -> bool {
        let at = |v: usize| self.projection.apply(pool.get(v));
        self.rings.iter().fold(false, |inside, ring| {
            inside ^ ring_contains(ring.iter().map(|&v| at(v)), q)
        })
    }
}

/// The area vector of a face given as rings of pool vertices (normal to it, twice its area
/// long, by Newell's method over all its rings), and the centre of its vertices, as the solid's
/// checks work them out.
pub(super) fn area_and_centre(rings: &[Vec<usize>], pool: &Pool) -> (Vector, Vector) {
    let area = rings.iter().fold([0.0; 3], |sum, ring| {
        let ring: Vec<Vector> = ring.iter().map(|&v| pool.get(v)).collect();
        add(sum, newell(&ring))
    });
    let (sum, count) = rings
        .iter()
        .flatten()
        .fold(([0.0; 3], 0_u32), |(sum, count), &v| {
            (add(sum, pool.get(v)), count + 1)
        });
    (area, scale(sum, 1.0 / f64::from(count)))
}

/// The unit normal of a face given as rings of pool vertices, and the centre of its vertices.
pub(super) fn plane(rings: &[Vec<usize>], pool: &Pool) -> (Vector, Vector) {
    let (area, centre) = area_and_centre(rings, pool);
    (scale(area, 1.0 / length(area)), centre)
}

/// Cleans every ring of a face with [`clean_ring`]; a face whose outer ring goes loses its holes
/// too, so it is left empty.
pub(super) fn clean_face(rings: &mut Vec<Vec<usize>>) {
    for ring in rings.iter_mut() {
        *ring = clean_ring(ring.iter().copied());
    }
    if rings.first().is_some_and(Vec::is_empty) {
        rings.clear();
    }
    rings.retain(|ring| !ring.is_empty());
}

/// A ring with repeated vertices (`u`, `u`) and spikes (`u`, `v`, `u`) removed, all around
/// it; empty if fewer than three vertices remain. Merging vertices leaves both behind.
pub(super) fn clean_ring(ring: impl Iterator<Item = usize>) -> Vec<usize> {
    let mut out: Vec<usize> = ring.collect();
    let mut i = 0;
    while out.len() >= 3 && i < out.len() {
        let n = out.len();
        let (next, after) = ((i + 1) % n, (i + 2) % n);
        if out[i] == out[next] {
            out.remove(next);
        } else if out[i] == out[after] {
            // Remove the higher index first so the lower one stays valid.
            out.remove(next.max(after));
            out.remove(next.min(after));
        } else {
            i += 1;
            continue;
        }
        i = i.saturating_sub(2);
    }
    if out.len() < 3 {
        out.clear();
    }
    out
}

/// Whether the ring, as a closed polyline, crosses a ray from `q` towards +x an odd number of
/// times. Each crossing is decided by the exact sign of `orient2d`.
///
/// A point exactly on the ring may come out either way (inside on some edges, outside on
/// others). Callers never depend on it: they test interior points of pieces, vertices of other
/// rings that do not touch this one, or add their own tolerance test for the boundary.
pub(super) fn ring_contains(ring: impl Iterator<Item = Point2> + Clone, q: Point2) -> bool {
    let next = ring.clone().cycle().skip(1);
    ring.zip(next).fold(false, |inside, (a, b)| {
        if (a.y > q.y) == (b.y > q.y) {
            return inside;
        }
        // Coordinates come from valid solids or flushed constructions, so the predicate accepts
        // them. Zero means `q` is on the edge, where the answer is not defined (see above).
        let side = orient2d(a, b, q).unwrap_or(Orientation::Zero);
        let crosses = match side {
            Orientation::Positive => b.y > a.y,
            Orientation::Negative => b.y < a.y,
            Orientation::Zero => true,
        };
        inside ^ crosses
    })
}

/// The unit direction of the line where two planes meet; `None` if they are parallel.
pub(super) fn meeting_direction(a: &PlanarFace, b: &PlanarFace) -> Option<Vector> {
    unit(cross(a.normal, b.normal))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_lose_repeats_and_spikes() {
        let clean = |ring: &[usize]| clean_ring(ring.iter().copied());
        assert_eq!(clean(&[0, 1, 1, 2, 3, 0]), [1, 2, 3, 0]);
        assert_eq!(clean(&[0, 1, 2, 5, 2, 3]), [0, 1, 2, 3]);
        // Spikes across the start, and one inside another.
        assert_eq!(clean(&[5, 0, 1, 2, 0]), [1, 2, 0]);
        assert_eq!(clean(&[0, 1, 2, 7, 8, 7, 2]), [0, 1, 2]);
        assert_eq!(clean(&[0, 1, 0]), Vec::<usize>::new());
    }

    #[test]
    fn parallel_planes_do_not_meet() {
        let pool = Pool::of(vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 0.0, 2.0],
        ]);
        let floor = PlanarFace::new(vec![vec![0, 1, 2]], &pool);
        let roof = PlanarFace::new(vec![vec![3, 4, 5]], &pool);
        let wall = PlanarFace::new(vec![vec![0, 3, 6, 4, 1]], &pool);
        assert_eq!(meeting_direction(&floor, &roof), None);
        assert!(meeting_direction(&floor, &wall).is_some());
    }

    #[test]
    fn a_face_whose_vertices_merge_loses_its_rings() {
        let pool = Pool::of(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
        let mut face = PlanarFace::new(vec![vec![0, 1, 2]], &pool);
        face.remap(&[0, 0, 2], &pool);
        assert!(face.rings.is_empty());
    }
}
