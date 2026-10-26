//! The working set of a boolean: vertices of both operands plus constructed ones, and faces with
//! the plane they keep from their input face.

use std::collections::{BTreeMap, BTreeSet};

use crate::predicates::{MIN_COORDINATE, orient2d};
use crate::tolerance::{ROUNDING, WITHIN_TOLERANCE, WITHIN_TOLERANCE_SQUARED};
use crate::vector::{
    Vector, add, bounds, coords, cross, distance_squared, dot, edges, largest_axis, length, newell,
    scale, sub, unit,
};
use crate::{Orientation, Point2, Solid};

/// Every vertex of a boolean: those of the operands first, then the constructed ones.
pub(super) struct Pool {
    points: Vec<Vector>,
    inputs: usize,
    /// The planes each vertex lies on by how it was made, as the plane numbers of
    /// [`PlanarFace::plane`]: an operand vertex lies on the planes of its faces, a crossing on
    /// those of the lines that cross.
    planes: Vec<BTreeSet<usize>>,
    /// The vertices lying on each three planes, smallest plane first.
    on_three: BTreeMap<[usize; 3], Vec<usize>>,
    /// The vertices placed so far in each cell of a grid one rounding allowance wide.
    cells: BTreeMap<[i64; 3], Vec<usize>>,
    /// The copies made of operand vertices ([`Self::duplicate`]), which are operand vertices too.
    copies: BTreeSet<usize>,
}

impl Pool {
    pub(super) fn new(a: &Solid, b: &Solid) -> Self {
        let points: Vec<Vector> = a
            .vertices()
            .iter()
            .chain(b.vertices())
            .map(|&p| coords(p))
            .collect();
        Self::of(points)
    }

    /// A pool of given points, all from operands.
    pub(super) fn of(points: Vec<Vector>) -> Self {
        Self {
            inputs: points.len(),
            planes: vec![BTreeSet::new(); points.len()],
            on_three: BTreeMap::new(),
            cells: BTreeMap::new(),
            copies: BTreeSet::new(),
            points,
        }
    }

    pub(super) fn get(&self, vertex: usize) -> Vector {
        self.points[vertex]
    }

    pub(super) fn len(&self) -> usize {
        self.points.len()
    }

    /// Whether a vertex comes from an operand rather than from an intersection, as itself or as
    /// a copy, so that which copy keeps the original number does not matter.
    pub(super) fn is_input(&self, vertex: usize) -> bool {
        vertex < self.inputs || self.copies.contains(&vertex)
    }

    /// Adds a constructed vertex, flushed like a moved one.
    pub(super) fn push(&mut self, point: Vector) -> usize {
        self.points.push(flushed(point));
        self.planes.push(BTreeSet::new());
        self.points.len() - 1
    }

    /// The vertex lying on the given planes: one already made on three of them, if any lies
    /// within the tolerance of `point`, else a new one there. Three planes meet at one point
    /// unless they share a line, so a vertex made on them is that point, however rounding put
    /// it; the distance only tells the two apart where they share a line.
    pub(super) fn on_planes(&mut self, point: Vector, planes: &BTreeSet<usize>) -> usize {
        if let Some(vertex) = self.made_on(point, planes).or_else(|| self.at(point)) {
            self.lies_on(vertex, planes.iter().copied());
            return vertex;
        }
        let vertex = self.push(point);
        self.place(vertex);
        self.lies_on(vertex, planes.iter().copied());
        vertex
    }

    /// Puts every vertex so far in the grid that [`Self::at`] searches, once their positions are
    /// final.
    pub(super) fn place_all(&mut self) {
        for vertex in 0..self.points.len() {
            self.place(vertex);
        }
    }

    fn place(&mut self, vertex: usize) {
        let cell = rounding_cell(self.points[vertex]);
        self.cells.entry(cell).or_default().push(vertex);
    }

    /// The first vertex placed within rounding of `point`: below it, two computations of one
    /// point cannot be told apart, so they are one.
    pub(super) fn at(&self, point: Vector) -> Option<usize> {
        let cell = rounding_cell(point);
        let near = (-1..=1)
            .flat_map(|dx| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dz| [dx, dy, dz])));
        near.filter_map(|[dx, dy, dz]| self.cells.get(&[cell[0] + dx, cell[1] + dy, cell[2] + dz]))
            .flatten()
            .copied()
            .filter(|&v| distance_squared(self.points[v], point) <= ROUNDING * ROUNDING)
            .min()
    }

    /// The first vertex made on three of the given planes within the tolerance of `point`.
    pub(super) fn made_on(&self, point: Vector, planes: &BTreeSet<usize>) -> Option<usize> {
        let planes: Vec<usize> = planes.iter().copied().collect();
        threes(&planes)
            .filter_map(|three| self.on_three.get(&three))
            .flatten()
            .copied()
            .filter(|&v| distance_squared(self.points[v], point) <= WITHIN_TOLERANCE_SQUARED)
            .min()
    }

    /// Records that a vertex lies on the given planes.
    pub(super) fn lies_on(&mut self, vertex: usize, planes: impl Iterator<Item = usize>) {
        for plane in planes {
            if !self.planes[vertex].insert(plane) {
                continue;
            }
            let all: Vec<usize> = self.planes[vertex].iter().copied().collect();
            for three in threes(&all).filter(|three| three.contains(&plane)) {
                self.on_three.entry(three).or_default().push(vertex);
            }
        }
    }

    /// The planes a vertex lies on by how it was made.
    pub(super) fn planes(&self, vertex: usize) -> &BTreeSet<usize> {
        &self.planes[vertex]
    }

    /// Moves a vertex. Components too small for the exact predicates are flushed to zero: they
    /// are far below the tolerance, and the solid's checks would reject them.
    pub(super) fn set(&mut self, vertex: usize, point: Vector) {
        self.points[vertex] = flushed(point);
    }

    /// Adds a copy of a vertex, for a solid that touches itself there.
    pub(super) fn duplicate(&mut self, vertex: usize) -> usize {
        self.points.push(self.points[vertex]);
        self.planes.push(self.planes[vertex].clone());
        let copy = self.points.len() - 1;
        if self.is_input(vertex) {
            self.copies.insert(copy);
        }
        copy
    }
}

/// Every three of the given planes, in increasing order, each with its planes in increasing
/// order: the keys of [`Pool::on_three`].
fn threes(planes: &[usize]) -> impl Iterator<Item = [usize; 3]> + '_ {
    (0..planes.len()).flat_map(move |i| {
        (i + 1..planes.len()).flat_map(move |j| {
            planes[j + 1..]
                .iter()
                .map(move |&c| [planes[i], planes[j], c])
        })
    })
}

/// The planes of the faces along each edge of the given faces, by the edge's ends in increasing
/// order.
pub(super) fn edge_planes(faces: &[PlanarFace]) -> BTreeMap<(usize, usize), BTreeSet<usize>> {
    let mut planes: BTreeMap<(usize, usize), BTreeSet<usize>> = BTreeMap::new();
    for face in faces {
        for (u, v) in face.rings.iter().flat_map(|ring| edges(ring)) {
            planes
                .entry((u.min(v), u.max(v)))
                .or_default()
                .insert(face.plane);
        }
    }
    planes
}

/// The cell one rounding allowance wide holding a point.
#[expect(
    clippy::cast_possible_truncation,
    reason = "model coordinates over the rounding allowance stay within ±1e13"
)]
fn rounding_cell(p: Vector) -> [i64; 3] {
    p.map(|c| (c / ROUNDING).floor() as i64)
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
    /// The number of its plane: its own index among the faces of the boolean, or that of the
    /// face whose plane it was put onto (ADR 0001, merge pass step 1).
    pub(super) plane: usize,
}

impl PlanarFace {
    pub(super) fn new(rings: Vec<Vec<usize>>, pool: &Pool, number: usize) -> Self {
        let (normal, centre) = plane(&rings, pool);
        let mut face = Self {
            rings,
            normal,
            centre,
            projection: Projection::along(normal),
            min: [0.0; 3],
            max: [0.0; 3],
            plane: number,
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

    /// Which side of the plane a vertex lies on, positive outside: on it if it is known to lie
    /// on it ([`Pool::planes`]), otherwise the side its distance says. Decisions of the cut take
    /// a vertex's side from here, never from a tolerance: a vertex a micrometre off a plane lies
    /// off it, and the merge pass removes what that leaves within the tolerance.
    pub(super) fn side_of(&self, vertex: usize, pool: &Pool) -> i8 {
        if pool.planes(vertex).contains(&self.plane) {
            0
        } else if self.distance(pool.get(vertex)) > 0.0 {
            1
        } else {
            -1
        }
    }

    /// Whether every vertex of this face lies on the plane of `other`, within the tolerance:
    /// only for putting faces onto one plane (merge pass, step 1). The cut reads
    /// [`Self::coplanar`].
    pub(super) fn lies_within_tolerance_of(&self, other: &Self, pool: &Pool) -> bool {
        self.rings
            .iter()
            .flatten()
            .all(|&v| other.side(pool.get(v)) == 0)
    }

    /// Whether every vertex of this face is known to lie on the plane of `other`
    /// ([`Pool::planes`]).
    fn lies_exactly_on(&self, other: &Self, pool: &Pool) -> bool {
        (self.rings.iter().flatten()).all(|&v| pool.planes(v).contains(&other.plane))
    }

    /// Whether the faces lie in one plane as the cut sees them: every vertex of one of them lies
    /// on the other's plane. Faces within the tolerance of each other lie in one plane only once
    /// they are put onto it (merge pass, step 1): a face within the tolerance of two planes
    /// meeting at a shallow angle goes onto one of them, and lies across the other.
    pub(super) fn coplanar(&self, other: &Self, pool: &Pool) -> bool {
        self.lies_exactly_on(other, pool) || other.lies_exactly_on(self, pool)
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
        let floor = PlanarFace::new(vec![vec![0, 1, 2]], &pool, 0);
        let roof = PlanarFace::new(vec![vec![3, 4, 5]], &pool, 1);
        let wall = PlanarFace::new(vec![vec![0, 3, 6, 4, 1]], &pool, 2);
        assert_eq!(meeting_direction(&floor, &roof), None);
        assert!(meeting_direction(&floor, &wall).is_some());
    }

    #[test]
    fn a_face_whose_vertices_merge_loses_its_rings() {
        let pool = Pool::of(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
        let mut face = PlanarFace::new(vec![vec![0, 1, 2]], &pool, 0);
        face.remap(&[0, 0, 2], &pool);
        assert!(face.rings.is_empty());
    }

    #[test]
    fn a_vertex_made_on_three_planes_again_is_the_same() {
        let mut pool = Pool::of(vec![[0.0, 0.0, 0.0]]);
        pool.lies_on(0, [1, 2, 3].into_iter());
        pool.place_all();
        // Made on three of its planes, 0.5 µm off by rounding of nearly parallel planes: vertex
        // 0. On only two of them, it is another, unless it lies within rounding of vertex 0.
        let near = [0.5e-6, 0.0, 0.0];
        assert_eq!(pool.on_planes(near, &[1, 2, 3, 4].into()), 0);
        assert_eq!(pool.planes(0), &[1, 2, 3, 4].into());
        let other = pool.on_planes(near, &[1, 2, 5].into());
        assert_eq!(other, 1);
        assert_eq!(pool.on_planes([0.5e-9, 0.0, 0.0], &[6, 7, 8].into()), 0);
        assert_eq!(pool.at([0.5e-6, 1e-12, 0.0]), Some(1));
        assert_eq!(pool.at([2.0e-6, 0.0, 0.0]), None);
    }

    #[test]
    fn a_vertex_lies_on_a_plane_only_if_it_is_known_to() {
        // The square z = 0 facing up; vertex 4 is 0.1 µm above it, vertex 5 as far below but
        // known to lie on it.
        let mut pool = Pool::of(vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.5, 0.5, 0.1e-6],
            [0.5, 0.5, -0.1e-6],
        ]);
        let face = PlanarFace::new(vec![vec![0, 1, 2, 3]], &pool, 7);
        pool.lies_on(5, std::iter::once(7));
        assert_eq!(face.side_of(4, &pool), 1);
        assert_eq!(face.side_of(5, &pool), 0);
    }

    #[test]
    fn a_copy_of_an_operand_vertex_is_one_too() {
        // Lumps touching at a vertex get copies of it; which one keeps the original number
        // depends on the operands' order, so the copy must count alike.
        let mut pool = Pool::of(vec![[0.0, 0.0, 0.0]]);
        let made = pool.push([1.0, 0.0, 0.0]);
        let (copy, copy_of_made) = (pool.duplicate(0), pool.duplicate(made));
        assert!(pool.is_input(copy));
        assert!(!pool.is_input(copy_of_made));
    }
}
