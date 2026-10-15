//! Conforming Delaunay meshes of straight-edged profiles, for the warping solve. Rationale:
//! `docs/dev/adr/0011-shear-centre.md`.

use std::collections::{BTreeMap, BTreeSet};

use super::ShearCentreError;
use crate::predicates::{Orientation, incircle, orient2d};
use crate::profile::{Curve, encloses};
use crate::{Edge, Point2, Profile};

/// Elements the mesh aims at, whatever the profile's size or proportions.
pub(crate) const TARGET_ELEMENTS: f64 = 4000.0;

/// Largest number of nodes a shear-centre mesh may have, so hostile input cannot run for
/// minutes.
pub const MAX_MESH_NODES: usize = 50_000;

/// Largest number of lattice points tested for the interior; a profile filling little of its
/// bounding box, such as a long diagonal strip, would otherwise test billions.
const MAX_LATTICE_POINTS: f64 = 2_000_000.0;

/// Largest number of point-to-edge tests the interior lattice may cost: each lattice point is
/// tested against every edge, and a profile can have thousands of edges once faceted.
const MAX_LATTICE_TESTS: f64 = 200_000_000.0;

/// Triangles covering a profile exactly, counterclockwise, sharing nodes.
#[derive(Clone, Debug)]
pub(crate) struct Mesh {
    pub(crate) nodes: Vec<Point2>,
    pub(crate) triangles: Vec<[usize; 3]>,
}

#[cfg(test)]
impl Mesh {
    /// The area the triangles cover.
    pub(crate) fn covered_area(&self) -> f64 {
        self.triangles
            .iter()
            .map(|t| {
                let [p, q, r] = t.map(|n| self.nodes[n]);
                ((q.x - p.x) * (r.y - p.y) - (r.x - p.x) * (q.y - p.y)) / 2.0
            })
            .sum()
    }
}

/// The mesh of a profile whose edges are all lines.
pub(crate) fn mesh(profile: &Profile) -> Result<Mesh, ShearCentreError> {
    let curves: Vec<Vec<Curve>> = core::iter::once(profile.outer())
        .chain(profile.holes())
        .map(|edges| edges.iter().map(Edge::curve).collect())
        .collect();
    let size = (4.0 * profile.area() / (3.0_f64.sqrt() * TARGET_ELEMENTS)).sqrt();
    #[expect(clippy::cast_precision_loss, reason = "edge counts are far below 2⁵³")]
    let edge_count = curves.iter().map(Vec::len).sum::<usize>() as f64;
    let bounds = profile.bounds();
    let lattice = lattice(bounds, size, edge_count)?;
    let mut builder = Builder::new(bounds, MAX_MESH_NODES);
    let mut pieces = Vec::new();
    for edges in &curves {
        boundary_nodes(edges, size / 2.0, &mut builder, &mut pieces)?;
    }
    for point in lattice {
        let clear = curves
            .iter()
            .flatten()
            .all(|curve| curve.distance_to(point) >= size / 2.0);
        if clear && inside(&curves, point) {
            builder.insert(point)?;
        }
    }
    let boundary = builder.recover(pieces)?;
    Ok(builder.finish(&boundary))
}

/// Nodes along each edge of a loop, at most `step` apart, and the boundary pieces between them.
fn boundary_nodes(
    edges: &[Curve],
    step: f64,
    builder: &mut Builder,
    pieces: &mut Vec<(usize, usize)>,
) -> Result<(), ShearCentreError> {
    let first = builder.insert(edges[0].start())?;
    let mut previous = first;
    for (index, edge) in edges.iter().enumerate() {
        let (start, end) = (edge.start(), edge.end());
        let (dx, dy) = (end.x - start.x, end.y - start.y);
        let count = whole((libm::hypot(dx, dy) / step).ceil().max(1.0))?;
        for k in 1..count {
            let t = f64::from(k) / f64::from(count);
            let node =
                builder.insert(Point2::new(dx.mul_add(t, start.x), dy.mul_add(t, start.y)))?;
            pieces.push((previous, node));
            previous = node;
        }
        let last = if index + 1 == edges.len() {
            first
        } else {
            builder.insert(end)?
        };
        pieces.push((previous, last));
        previous = last;
    }
    Ok(())
}

/// A count as an integer, when it is within the node budget.
fn whole(count: f64) -> Result<u32, ShearCentreError> {
    #[expect(clippy::cast_precision_loss, reason = "the budget is far below 2⁵³")]
    let budget = MAX_MESH_NODES as f64;
    if count <= budget {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a whole number between 0 and the budget"
        )]
        Ok(count as u32)
    } else {
        Err(ShearCentreError::MeshTooLarge)
    }
}

/// Points of a triangular lattice of spacing `size` over a box, row by row, when there are few
/// enough of them to test each against `edge_count` edges.
fn lattice(
    bounds: [f64; 4],
    size: f64,
    edge_count: f64,
) -> Result<impl Iterator<Item = Point2>, ShearCentreError> {
    let [min_x, min_y, max_x, max_y] = bounds;
    let row_height = size * 3.0_f64.sqrt() / 2.0;
    let rows = ((max_y - min_y) / row_height).ceil();
    let columns = ((max_x - min_x) / size).ceil() + 1.0;
    let points = rows * columns;
    if points > MAX_LATTICE_POINTS || points * edge_count > MAX_LATTICE_TESTS {
        return Err(ShearCentreError::MeshTooLarge);
    }
    let (rows, columns) = (whole(rows)?, whole(columns)?);
    Ok((0..rows).flat_map(move |row| {
        let y = row_height.mul_add(f64::from(row) + 0.5, min_y);
        let shift = if row % 2 == 0 { 0.0 } else { 0.5 };
        (0..columns)
            .map(move |column| Point2::new(size.mul_add(f64::from(column) + shift, min_x), y))
    }))
}

/// Whether a point away from every loop lies inside the outer loop and outside the holes.
fn inside(loops: &[Vec<Curve>], point: Point2) -> bool {
    encloses(&loops[0], point) && !loops[1..].iter().any(|hole| encloses(hole, point))
}

/// Points of the enclosing triangle; real nodes come after them.
const CORNERS: usize = 3;

/// An incremental Delaunay triangulation (Bowyer–Watson) inside a large enclosing triangle.
struct Builder {
    /// The three corners of the enclosing triangle, then the mesh nodes.
    points: Vec<Point2>,
    triangles: Vec<[usize; 3]>,
    alive: Vec<bool>,
    /// The triangle each directed edge belongs to, counterclockwise.
    edges: BTreeMap<(usize, usize), usize>,
    /// Where the last point location ended, to start the next one nearby.
    hint: usize,
    max_nodes: usize,
}

impl Builder {
    fn new(bounds: [f64; 4], max_nodes: usize) -> Self {
        let [min_x, min_y, max_x, max_y] = bounds;
        let (cx, cy) = (f64::midpoint(min_x, max_x), f64::midpoint(min_y, max_y));
        let span = (max_x - min_x).max(max_y - min_y);
        let mut builder = Self {
            points: vec![
                Point2::new(cx - 20.0 * span, cy - 10.0 * span),
                Point2::new(cx + 20.0 * span, cy - 10.0 * span),
                Point2::new(cx, cy + 20.0 * span),
            ],
            triangles: Vec::new(),
            alive: Vec::new(),
            edges: BTreeMap::new(),
            hint: 0,
            max_nodes,
        };
        builder.add_triangle([0, 1, 2]);
        builder
    }

    fn add_triangle(&mut self, corners: [usize; 3]) -> usize {
        let id = self.triangles.len();
        for (u, v) in sides(corners) {
            self.edges.insert((u, v), id);
        }
        self.triangles.push(corners);
        self.alive.push(true);
        id
    }

    /// Inserts a node, or returns the existing one at the same place.
    fn insert(&mut self, point: Point2) -> Result<usize, ShearCentreError> {
        if self.points.len() >= self.max_nodes + CORNERS {
            return Err(ShearCentreError::MeshTooLarge);
        }
        let start = self.locate(point)?;
        if let Some(&existing) = self.triangles[start]
            .iter()
            .find(|&&v| self.points[v] == point)
        {
            return Ok(existing);
        }
        let index = self.points.len();
        self.points.push(point);
        let cavity = self.cavity(start, point)?;
        let mut rim = Vec::new();
        for &t in &cavity {
            for (u, v) in sides(self.triangles[t]) {
                let outside = self
                    .edges
                    .get(&(v, u))
                    .is_none_or(|neighbour| !cavity.contains(neighbour));
                if outside {
                    rim.push((u, v));
                }
            }
        }
        for &t in &cavity {
            for side in sides(self.triangles[t]) {
                self.edges.remove(&side);
            }
            self.alive[t] = false;
        }
        for (u, v) in rim {
            self.hint = self.add_triangle([u, v, index]);
        }
        Ok(index)
    }

    /// The triangle containing `point`, walking from the last one built. A visibility walk in a
    /// Delaunay triangulation never cycles, and exact predicates keep it so.
    fn locate(&self, point: Point2) -> Result<usize, ShearCentreError> {
        let mut t = self.hint;
        loop {
            let mut crossed = None;
            for (u, v) in sides(self.triangles[t]) {
                let side = orient2d(self.points[u], self.points[v], point)
                    .map_err(|_| ShearCentreError::MeshFailed)?;
                if side == Orientation::Negative {
                    crossed = Some((u, v));
                    break;
                }
            }
            let Some((u, v)) = crossed else {
                return Ok(t);
            };
            // No neighbour: the point is outside the enclosing triangle.
            t = *self
                .edges
                .get(&(v, u))
                .ok_or(ShearCentreError::MeshFailed)?;
        }
    }

    /// The triangles whose circumcircle strictly contains `point`, from the one holding it.
    fn cavity(&self, start: usize, point: Point2) -> Result<BTreeSet<usize>, ShearCentreError> {
        let mut cavity = BTreeSet::from([start]);
        let mut stack = vec![start];
        while let Some(t) = stack.pop() {
            for (u, v) in sides(self.triangles[t]) {
                let Some(&next) = self.edges.get(&(v, u)) else {
                    continue;
                };
                if cavity.contains(&next) {
                    continue;
                }
                let [a, b, c] = self.triangles[next].map(|i| self.points[i]);
                let inside = incircle(a, b, c, point).map_err(|_| ShearCentreError::MeshFailed)?;
                if inside == Orientation::Positive {
                    cavity.insert(next);
                    stack.push(next);
                }
            }
        }
        Ok(cavity)
    }

    fn has_edge(&self, a: usize, b: usize) -> bool {
        self.edges.contains_key(&(a, b)) || self.edges.contains_key(&(b, a))
    }

    /// Splits boundary pieces missing from the triangulation until every piece is an edge, and
    /// returns the pieces.
    fn recover(
        &mut self,
        mut pending: Vec<(usize, usize)>,
    ) -> Result<Vec<(usize, usize)>, ShearCentreError> {
        let mut done = Vec::with_capacity(pending.len());
        loop {
            while let Some((a, b)) = pending.pop() {
                if self.has_edge(a, b) {
                    done.push((a, b));
                    continue;
                }
                let (p, q) = (self.points[a], self.points[b]);
                let middle = Point2::new(f64::midpoint(p.x, q.x), f64::midpoint(p.y, q.y));
                let m = self.insert(middle)?;
                if m == a || m == b {
                    // Unreachable in practice: a piece too short to split in floating point
                    // joins nearest neighbours, which a Delaunay triangulation always connects.
                    return Err(ShearCentreError::MeshFailed);
                }
                pending.push((a, m));
                pending.push((m, b));
            }
            // A later split can remove an edge recovered earlier; check them all again. Every
            // round inserts nodes, so the node budget ends the loop.
            if done.iter().all(|&(a, b)| self.has_edge(a, b)) {
                return Ok(done);
            }
            pending = core::mem::take(&mut done);
        }
    }

    /// The triangles inside the profile, with the enclosing triangle's corners dropped and the
    /// nodes renumbered.
    fn finish(self, boundary: &[(usize, usize)]) -> Mesh {
        let walls: BTreeSet<(usize, usize)> = boundary.iter().map(|&(a, b)| key(a, b)).collect();
        // Crossing a boundary edge switches between outside and inside; the triangles at the
        // enclosing corners are outside.
        let mut inside: Vec<Option<bool>> = vec![None; self.triangles.len()];
        let mut stack = Vec::new();
        for (t, corners) in self.triangles.iter().enumerate() {
            if self.alive[t] && corners.iter().any(|&v| v < CORNERS) {
                inside[t] = Some(false);
                stack.push(t);
            }
        }
        while let Some(t) = stack.pop() {
            let here = inside[t] == Some(true);
            for (u, v) in sides(self.triangles[t]) {
                if let Some(&next) = self.edges.get(&(v, u))
                    && inside[next].is_none()
                {
                    inside[next] = Some(here != walls.contains(&key(u, v)));
                    stack.push(next);
                }
            }
        }
        let mut renumber = vec![usize::MAX; self.points.len()];
        let mut nodes = Vec::new();
        let mut triangles = Vec::new();
        for (t, corners) in self.triangles.iter().enumerate() {
            if self.alive[t] && inside[t] == Some(true) {
                triangles.push(corners.map(|v| {
                    if renumber[v] == usize::MAX {
                        renumber[v] = nodes.len();
                        nodes.push(self.points[v]);
                    }
                    renumber[v]
                }));
            }
        }
        Mesh { nodes, triangles }
    }
}

/// The three directed sides of a counterclockwise triangle.
const fn sides([a, b, c]: [usize; 3]) -> [(usize, usize); 3] {
    [(a, b), (b, c), (c, a)]
}

/// An undirected edge as a map key.
fn key(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_box() -> Builder {
        Builder::new([0.0, 0.0, 1.0, 1.0], 8)
    }

    #[test]
    fn inserting_a_point_twice_returns_the_same_node() {
        let mut builder = unit_box();
        let first = builder.insert(Point2::new(0.5, 0.5)).expect("inserts");
        assert_eq!(builder.insert(Point2::new(0.5, 0.5)), Ok(first));
    }

    #[test]
    fn the_lattice_is_bounded_by_its_tests_against_edges() {
        // 116 rows of 101 points is small, but not against 100 000 edges each.
        assert!(lattice([0.0, 0.0, 1.0, 1.0], 0.01, 10.0).is_ok());
        assert!(matches!(
            lattice([0.0, 0.0, 1.0, 1.0], 0.01, 100_000.0),
            Err(ShearCentreError::MeshTooLarge)
        ));
    }

    #[test]
    fn the_node_budget_is_enforced() {
        let mut builder = unit_box();
        for i in 0..8 {
            builder
                .insert(Point2::new(f64::from(i) / 8.0, 0.25))
                .expect("within budget");
        }
        assert_eq!(
            builder.insert(Point2::new(0.5, 0.75)),
            Err(ShearCentreError::MeshTooLarge)
        );
    }

    #[test]
    fn unusable_points_fail_the_mesh() {
        let mut builder = unit_box();
        assert_eq!(
            builder.insert(Point2::new(f64::NAN, 0.0)),
            Err(ShearCentreError::MeshFailed)
        );
        assert_eq!(
            builder.insert(Point2::new(1e6, 1e6)),
            Err(ShearCentreError::MeshFailed)
        );
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod properties {
    use core::f64::consts::TAU;

    use proptest::prelude::*;
    use proptest::test_runner::RngSeed;

    use super::*;

    /// A star-shaped polygon (vertices at increasing angles, random radii) with reflex corners,
    /// and stray points around it that block its sides.
    fn star_with_strays() -> impl Strategy<Value = (Vec<Point2>, Vec<Point2>)> {
        (
            // At least 4 corners with steps from 0.5 to 1: every gap stays below π.
            prop::collection::vec((0.5..1.0f64, 0.2..1.0f64), 4..12),
            prop::collection::vec((-1.2..1.2f64, -1.2..1.2f64), 0..40),
        )
            .prop_map(|(corners, strays)| {
                let total: f64 = corners.iter().map(|c| c.0).sum();
                let mut angle = 0.0;
                let polygon = corners
                    .iter()
                    .map(|&(step, radius)| {
                        angle += step / total * TAU;
                        Point2::new(libm::cos(angle) * radius, libm::sin(angle) * radius)
                    })
                    .collect();
                let strays = strays.into_iter().map(|(x, y)| Point2::new(x, y)).collect();
                (polygon, strays)
            })
    }

    fn config() -> ProptestConfig {
        ProptestConfig {
            cases: 500,
            rng_seed: RngSeed::Fixed(11),
            failure_persistence: None,
            ..ProptestConfig::default()
        }
    }

    proptest! {
        #![proptest_config(config())]

        #[test]
        fn recovered_pieces_are_edges_and_bound_the_mesh((polygon, strays) in star_with_strays()) {
            let mut builder = Builder::new([-1.2, -1.2, 1.2, 1.2], MAX_MESH_NODES);
            let corners: Vec<usize> = polygon
                .iter()
                .map(|&p| builder.insert(p).expect("inserts"))
                .collect();
            for &p in &strays {
                builder.insert(p).expect("inserts");
            }
            let pieces = (0..corners.len())
                .map(|i| (corners[i], corners[(i + 1) % corners.len()]))
                .collect();
            let Ok(boundary) = builder.recover(pieces) else {
                // Sharp corners may exhaust the budget; that is a typed failure, not a panic.
                return Ok(());
            };
            prop_assert!(boundary.iter().all(|&(a, b)| builder.has_edge(a, b)));
            let area = builder.finish(&boundary).covered_area();
            let edges = (0..polygon.len())
                .map(|i| Edge::Line { start: polygon[i], end: polygon[(i + 1) % polygon.len()] })
                .collect();
            let expected = Profile::new(edges, vec![]).expect("a star polygon is valid").area();
            prop_assert!(((area - expected) / expected).abs() < 1e-9, "{area} != {expected}");
        }
    }
}
