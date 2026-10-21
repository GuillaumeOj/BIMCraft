//! Keeping faces flat around the vertices `align` moved (ADR 0001, merge pass step 1).
//!
//! A vertex moved onto the plane of a face of the other operand leaves the other faces around it
//! bent: their far vertices stay on their planes, the moved one does not. The cut records every
//! vertex of a face as lying on its plane, so that must hold to rounding. A face with vertices
//! moved off its plane is cut along a collar, a line across it near them, about as far from
//! each as the face is wide there: as its shortest edge is long, at most a third of the edge.
//! The strip between the collar and the moved vertices becomes small triangles, each on a plane
//! of its own, and the rest keeps its plane: the collar's points are made on it. Cutting off a
//! triangle at the vertices beside a moved one instead would leave triangles as thin as a long
//! face, and three of their planes meeting near a tip nearly share a line, so the vertices made
//! on them could not be told apart.

use std::collections::{BTreeMap, BTreeSet};

use super::face::{PlanarFace, Pool, Projection};
use crate::predicates::orient2d;
use crate::tolerance::{ROUNDING, WITHIN_TOLERANCE};
use crate::vector::{Vector, add, distance_squared, dot, edges, scale, sub};
use crate::{Orientation, Point2};

/// A plane, as a unit normal and a point on it.
#[derive(Clone, Copy)]
struct Plane {
    normal: Vector,
    centre: Vector,
}

impl Plane {
    fn distance(self, p: Vector) -> f64 {
        dot(self.normal, sub(p, self.centre))
    }
}

/// A ring of pool vertices.
type Ring = Vec<usize>;

/// The collar points, by the vertex off a face's plane they are near and the other end of the
/// edge they lie on, with the planes they are made on.
type Points = BTreeMap<(usize, usize), (usize, BTreeSet<usize>)>;

/// Which vertices lie off the planes their faces keep once `align` has moved them.
struct Bend<'a> {
    /// The planes as they were before the move, by number.
    planes: Vec<Plane>,
    /// The number of the plane each face keeps.
    numbers: Vec<usize>,
    moved: &'a BTreeSet<usize>,
}

impl Bend<'_> {
    fn meant(&self, f: usize) -> Plane {
        self.planes[self.numbers[f]]
    }

    /// Whether vertex `v` of face `f` moved off the face's plane, beyond rounding.
    fn off(&self, f: usize, v: usize, pool: &Pool) -> bool {
        self.moved.contains(&v) && self.meant(f).distance(pool.get(v)).abs() > ROUNDING
    }
}

/// Cuts the faces bent by moving the `moved` vertices into a flat rest and flat triangles along
/// a collar where it can be made and fits ([`cut`]), and works out the planes of every face
/// whose vertices moved again. Each face still
/// has the plane it keeps as before the move. The triangles are new faces after those of their
/// operand, each on a plane of its own; returns the faces and where the second operand's start.
pub(super) fn collar(
    faces: Vec<PlanarFace>,
    split: usize,
    moved: &BTreeSet<usize>,
    pool: &mut Pool,
) -> (Vec<PlanarFace>, usize) {
    let bend = Bend {
        planes: (faces.iter())
            .map(|face| Plane {
                normal: face.normal,
                centre: face.centre(),
            })
            .collect(),
        numbers: faces.iter().map(|face| face.plane).collect(),
        moved,
    };
    let points = collar_points(&faces, &bend, pool);
    let mut blocks: [Vec<(PlanarFace, bool)>; 2] = [Vec::new(), Vec::new()];
    let mut strips: [Vec<Ring>; 2] = [Vec::new(), Vec::new()];
    for (f, mut face) in faces.into_iter().enumerate() {
        let block = usize::from(f >= split);
        let marked: Vec<Vec<(usize, bool)>> = (face.rings.iter())
            .map(|ring| {
                let mut out = Vec::new();
                for (v, w) in edges(ring) {
                    out.push((v, !bend.off(f, v, pool)));
                    for key in [(v, w), (w, v)] {
                        if let Some((c, on)) = points.get(&key) {
                            out.push((*c, on.contains(&bend.numbers[f])));
                        }
                    }
                }
                out
            })
            .collect();
        let changed = face.rings.iter().flatten().any(|v| moved.contains(v))
            || marked.iter().map(Vec::len).sum::<usize>()
                != face.rings.iter().map(Vec::len).sum::<usize>();
        // Without a collar point, the strip would reach the next vertex: the face stays whole.
        let collared = (face.rings.iter().flat_map(|ring| edges(ring))).all(|(v, w)| {
            match (bend.off(f, v, pool), bend.off(f, w, pool)) {
                (true, false) => points.contains_key(&(v, w)),
                (false, true) => points.contains_key(&(w, v)),
                _ => true,
            }
        });
        let projection = Projection::along(bend.meant(f).normal);
        match cut(&marked, pool, projection).filter(|_| collared) {
            Some((rest, triangles)) => {
                face.rings = rest;
                strips[block].extend(triangles);
            }
            None => {
                face.rings = (marked.iter())
                    .map(|ring| ring.iter().map(|&(v, _)| v).collect())
                    .collect();
            }
        }
        blocks[block].push((face, changed));
    }
    let shift = strips[0].len();
    let renumber = |plane: usize| if plane < split { plane } else { plane + shift };
    let mut out: Vec<PlanarFace> = Vec::new();
    for (block, triangles) in blocks.into_iter().zip(strips) {
        for (mut face, changed) in block {
            let plane = renumber(face.plane);
            if changed {
                face = PlanarFace::new(std::mem::take(&mut face.rings), pool, plane);
            } else {
                face.plane = plane;
            }
            out.push(face);
        }
        for triangle in triangles {
            let number = out.len();
            out.push(PlanarFace::new(vec![triangle], pool, number));
        }
    }
    (out, split + shift)
}

/// Makes the collar points: on each edge from a vertex off a face's plane to one on it, one near
/// the first, as far from it as its shortest edge is long, at most a third of the edge. It is
/// made on the plane of every face along the edge that keeps its plane beyond it, so that face
/// stays flat there. None is made where that would move it farther than the tolerance.
fn collar_points(faces: &[PlanarFace], bend: &Bend, pool: &mut Pool) -> Points {
    let mut along: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    let mut shortest: BTreeMap<usize, f64> = BTreeMap::new();
    let mut needed: BTreeSet<(usize, usize)> = BTreeSet::new();
    for (f, face) in faces.iter().enumerate() {
        for (u, w) in face.rings.iter().flat_map(|ring| edges(ring)) {
            along.entry((u.min(w), u.max(w))).or_default().push(f);
            let length = distance_squared(pool.get(u), pool.get(w)).sqrt();
            for v in [u, w] {
                let s = shortest.entry(v).or_insert(f64::INFINITY);
                *s = s.min(length);
            }
            match (bend.off(f, u, pool), bend.off(f, w, pool)) {
                (true, false) => _ = needed.insert((u, w)),
                (false, true) => _ = needed.insert((w, u)),
                _ => {}
            }
        }
    }
    let mut points = Points::new();
    for (u, a) in needed {
        let on: BTreeSet<usize> = along[&(u.min(a), u.max(a))]
            .iter()
            .filter(|&&g| !(bend.off(g, u, pool) && bend.off(g, a, pool)))
            .map(|&g| bend.numbers[g])
            .collect();
        let (pu, pa) = (pool.get(u), pool.get(a));
        let length = distance_squared(pu, pa).sqrt();
        let guide = add(
            pu,
            scale(sub(pa, pu), shortest[&u].min(length / 3.0) / length),
        );
        let planes: Vec<Plane> = on.iter().map(|&p| bend.planes[p]).collect();
        if let Some(point) = onto(guide, &planes) {
            points.insert((u, a), (pool.push(point), on));
        }
    }
    points
}

/// The point nearest `guide` lying on every given plane (at most two, along one edge); `None`
/// if that moves it farther than the tolerance, as for nearly parallel planes.
fn onto(guide: Vector, planes: &[Plane]) -> Option<Vector> {
    let point = match planes {
        [] => guide,
        [p] => sub(guide, scale(p.normal, p.distance(guide))),
        [p, q] => {
            let c = dot(p.normal, q.normal);
            let det = 1.0 - c * c;
            if det <= 0.0 {
                return None;
            }
            let (rp, rq) = (-p.distance(guide), -q.distance(guide));
            let alpha = (rp - c * rq) / det;
            let beta = (rq - c * rp) / det;
            add(guide, add(scale(p.normal, alpha), scale(q.normal, beta)))
        }
        _ => return None,
    };
    (distance_squared(point, guide) <= WITHIN_TOLERANCE * WITHIN_TOLERANCE).then_some(point)
}

/// The rest of a face and the triangles of its strips, from its rings with each vertex marked
/// whether it lies on the face's plane; `None` if some ring has no vertex on it, a strip has
/// more than two vertices off it, or a strip would not lie inside the face, in its projection.
fn cut(
    rings: &[Vec<(usize, bool)>],
    pool: &Pool,
    projection: Projection,
) -> Option<(Vec<Ring>, Vec<Ring>)> {
    let at = |v: usize| projection.apply(pool.get(v));
    let mut rest = Vec::new();
    let mut triangles = Vec::new();
    let mut collars = Vec::new();
    for ring in rings {
        let start = ring.iter().position(|&(_, on)| on)?;
        let n = ring.len();
        let mut kept = Vec::new();
        let mut i = 0;
        while i < n {
            let (v, _) = ring[(start + i) % n];
            kept.push(v);
            let run: Ring = (1..n)
                .map(|k| ring[(start + i + k) % n])
                .take_while(|&(_, on)| !on)
                .map(|(v, _)| v)
                .collect();
            let step = run.len() + 1;
            if !run.is_empty() {
                let next = ring[(start + i + step) % n].0;
                let polygon = std::iter::once(v).chain(run).chain([next]).collect();
                triangles.extend(strip(polygon, pool, at)?);
                collars.push((v, next));
            }
            i += step;
        }
        rest.push(kept);
    }
    let edges_of_face: Vec<(Point2, Point2, usize, usize)> = (rings.iter())
        .flat_map(|ring| edges(ring).map(|((u, _), (w, _))| (at(u), at(w), u, w)))
        .collect();
    let inside = triangles.iter().all(|t: &Ring| {
        let [a, b, c] = [t[0], t[1], t[2]].map(at);
        orient2d(a, b, c) == Ok(Orientation::Positive)
            && (rings.iter().flatten())
                .all(|&(v, _)| t.contains(&v) || !inside_closed(a, b, c, at(v)))
    });
    let clear = collars.iter().all(|&(p, q)| {
        (edges_of_face.iter())
            .filter(|&&(_, _, u, w)| ![u, w].iter().any(|x| *x == p || *x == q))
            .all(|&(a, b, _, _)| !crosses(at(p), at(q), a, b))
    });
    (inside && clear).then_some((rest, triangles))
}

/// Whether two segments share a point, ends included.
fn crosses(a: Point2, b: Point2, c: Point2, d: Point2) -> bool {
    let side = |p, q, r| orient2d(p, q, r).unwrap_or(Orientation::Zero);
    let ends = [(a, b, c), (a, b, d), (c, d, a), (c, d, b)];
    let sides = ends.map(|(p, q, r)| side(p, q, r));
    if sides.contains(&Orientation::Zero) {
        // An end on the other's line: they share a point only if it lies on the other.
        return (ends.iter().zip(sides))
            .any(|(&(p, q, r), s)| s == Orientation::Zero && inside_box(p, q, r));
    }
    sides[0] != sides[1] && sides[2] != sides[3]
}

/// Whether `r` lies in the box spanned by `p` and `q`.
fn inside_box(p: Point2, q: Point2, r: Point2) -> bool {
    r.x >= p.x.min(q.x) && r.x <= p.x.max(q.x) && r.y >= p.y.min(q.y) && r.y <= p.y.max(q.y)
}

/// The triangles of a strip, given round its boundary: ears cut off one by one, each time the
/// one whose new edge is shortest (the first of those), so the triangles are as well shaped as
/// the strip allows. An ear turns the face's way, in its projection, with no other vertex of
/// the strip inside or on it; `None` if no ear is left.
fn strip(mut polygon: Ring, pool: &Pool, at: impl Fn(usize) -> Point2) -> Option<Vec<Ring>> {
    let mut triangles = Vec::new();
    while polygon.len() > 3 {
        let n = polygon.len();
        let ear = |i: usize| [polygon[(i + n - 1) % n], polygon[i], polygon[(i + 1) % n]];
        let fits = |i: usize| {
            let [a, b, c] = ear(i).map(&at);
            orient2d(a, b, c) == Ok(Orientation::Positive)
                && (polygon.iter()).all(|v| ear(i).contains(v) || !inside_closed(a, b, c, at(*v)))
        };
        let length = |i: usize| {
            let [a, _, c] = ear(i);
            distance_squared(pool.get(a), pool.get(c))
        };
        let best = (0..n)
            .filter(|&i| fits(i))
            .min_by(|&i, &j| length(i).total_cmp(&length(j)))?;
        triangles.push(ear(best).to_vec());
        polygon.remove(best);
    }
    triangles.push(polygon);
    Some(triangles)
}

/// Whether `p` lies inside or on the triangle `a`, `b`, `c` turning counter-clockwise.
fn inside_closed(a: Point2, b: Point2, c: Point2, p: Point2) -> bool {
    [(a, b), (b, c), (c, a)]
        .iter()
        .all(|&(u, w)| orient2d(u, w, p) != Ok(Orientation::Negative))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1 m cube, corner `i` with bit 0 for x, bit 1 for y, bit 2 for z, its faces turned out.
    fn cube(pool: &Pool) -> Vec<PlanarFace> {
        let rings = [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ];
        (rings.iter().enumerate())
            .map(|(f, ring)| PlanarFace::new(vec![ring.to_vec()], pool, f))
            .collect()
    }

    fn cube_pool() -> Pool {
        Pool::of(
            (0..8)
                .map(|i| [i & 1, (i >> 1) & 1, (i >> 2) & 1].map(f64::from))
                .collect(),
        )
    }

    /// Moves vertices of a cube, cuts its faces along collars, and checks that every face lies
    /// flat; returns the faces.
    fn collared(shifts: &[(usize, Vector)]) -> (Vec<PlanarFace>, usize, Pool) {
        let mut pool = cube_pool();
        let faces = cube(&pool);
        for &(v, by) in shifts {
            pool.set(v, add(pool.get(v), by));
        }
        let vertices = shifts.iter().map(|&(v, _)| v).collect();
        let (faces, split) = collar(faces, 6, &vertices, &mut pool);
        (faces, split, pool)
    }

    fn flat(face: &PlanarFace, pool: &Pool) -> bool {
        (face.rings.iter().flatten()).all(|&v| face.distance(pool.get(v)).abs() <= ROUNDING)
    }

    #[test]
    fn a_face_bent_at_one_corner_keeps_its_plane_but_for_a_triangle() {
        // The top corner raised 0.5 µm: only the top face bends. It keeps its plane but for a
        // triangle at the corner, a third of an edge wide, and its neighbours take the
        // collar's points on their edges.
        let (faces, split, pool) = collared(&[(7, [0.0, 0.0, 0.5e-6])]);
        assert_eq!((faces.len(), split), (7, 7));
        assert!(faces.iter().all(|face| flat(face, &pool)));
        assert_eq!(faces[1].rings[0].len(), 5);
        assert_eq!(faces[1].plane, 1);
        let triangle = &faces[6];
        assert_eq!((triangle.plane, triangle.rings[0].len()), (6, 3));
        assert!(triangle.rings[0].contains(&7));
        assert_eq!([faces[3].rings[0].len(), faces[5].rings[0].len()], [5, 5]);
    }

    #[test]
    fn a_collar_point_lies_on_the_faces_that_stay_flat_beyond_it() {
        // Corner 7 moves off all three of its faces, corner 5 off the top only. Along the edge
        // between them the top is off at both ends, so the collar point near 7 is made on the
        // side face alone.
        let (faces, _, pool) = collared(&[(7, [0.3e-6, 0.3e-6, 0.3e-6]), (5, [0.0, 0.0, 0.5e-6])]);
        assert!(faces.iter().all(|face| flat(face, &pool)));
        let side = &faces[5];
        assert!(
            side.rings[0]
                .iter()
                .all(|&v| side.distance(pool.get(v)).abs() <= ROUNDING)
        );
    }

    #[test]
    fn a_face_with_every_vertex_moved_off_it_stays_whole() {
        // The whole top moved up and along y: no vertex of the top stays on its plane, so it is
        // not cut; the sides across y each get two triangles along their top edge.
        let up = [0.0, 0.5e-6, 0.5e-6];
        let (faces, split, pool) = collared(&[(4, up), (5, up), (6, up), (7, up)]);
        assert_eq!((faces.len(), split), (10, 10));
        assert_eq!(faces[1].rings, [[4, 5, 7, 6]]);
        assert!((faces.iter().enumerate()).all(|(f, face)| f == 1 || flat(face, &pool)));
    }

    #[test]
    fn a_strip_is_left_whole_where_it_does_not_fit() {
        let pool = Pool::of(vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.5, 0.2, 0.0],
        ]);
        let projection = Projection::along([0.0, 0.0, 1.0]);
        let marked = |on: [bool; 4]| vec![(0..4).map(|v| (v, on[v])).collect::<Vec<_>>()];
        // No vertex on the plane, or only one: the strip has no ear left.
        assert!(cut(&marked([false; 4]), &pool, projection).is_none());
        assert!(cut(&marked([true, false, false, false]), &pool, projection).is_none());
        // A vertex of a hole inside the strip's triangle.
        let mut holed = marked([true, false, true, true]);
        holed.push(vec![(4, true)]);
        assert!(cut(&holed, &pool, projection).is_none());
        assert!(cut(&marked([true, false, true, true]), &pool, projection).is_some());
    }

    #[test]
    fn a_face_whose_collar_point_cannot_be_made_stays_whole() {
        // The corner moved 2 µm off the top: the collar point near it, on the top's plane, would
        // lie farther than the tolerance from the edge, so none is made.
        let (faces, split, _) = collared(&[(7, [0.0, 0.0, 2e-6])]);
        assert_eq!((faces.len(), split), (6, 6));
        assert_eq!(faces[1].rings, [[4, 5, 7, 6]]);
    }

    #[test]
    fn a_collar_point_is_not_moved_beyond_the_tolerance() {
        let plane = |normal, z| Plane {
            normal,
            centre: [0.0, 0.0, z],
        };
        let floor = plane([0.0, 0.0, 1.0], 0.0);
        let guide = [1.0, 2.0, 0.5e-6];
        assert_eq!(onto(guide, &[]), Some(guide));
        assert_eq!(onto(guide, &[floor]), Some([1.0, 2.0, 0.0]));
        assert_eq!(onto(guide, &[plane([0.0, 0.0, 1.0], 2e-6)]), None);
        assert_eq!(onto(guide, &[floor, floor]), None);
        assert_eq!(onto(guide, &[floor, floor, floor]), None);
        let wall = Plane {
            normal: [1.0, 0.0, 0.0],
            centre: [1.0, 0.0, 0.0],
        };
        assert_eq!(onto(guide, &[floor, wall]), Some([1.0, 2.0, 0.0]));
    }

    #[test]
    fn segments_cross_where_they_share_a_point() {
        let p = |x, y| Point2::new(x, y);
        let (a, b) = (p(0.0, 0.0), p(2.0, 0.0));
        assert!(crosses(a, b, p(1.0, -1.0), p(1.0, 1.0)));
        assert!(!crosses(a, b, p(1.0, 1.0), p(1.0, 2.0)));
        // An end on the other, and collinear pieces overlapping or apart.
        assert!(crosses(a, b, p(1.0, 0.0), p(1.0, 1.0)));
        assert!(!crosses(a, b, p(3.0, 0.0), p(3.0, 1.0)));
        assert!(crosses(a, b, p(1.0, 0.0), p(3.0, 0.0)));
        assert!(!crosses(a, b, p(3.0, 0.0), p(4.0, 0.0)));
    }
}
