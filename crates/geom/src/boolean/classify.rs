//! Where each piece of a cut face lies relative to the other operand.
//!
//! Pieces are not classified one by one from a point inside each: a piece a few tolerances wide
//! has no point far enough from the other operand's faces for that to be reliable, and two
//! neighbouring pieces classified apart need not agree along the edge they share. Instead the
//! side is read where the other operand's surface meets the pieces, and carried on from there:
//!
//! - Along an edge that faces of the other operand hold, a piece lies on the side of those
//!   faces it reaches into. Seen along the edge, their half-planes split the space around it
//!   into sectors alternately inside and outside the other operand, and the piece lies in one of
//!   them; a piece reaching into a face coplanar with its own lies on that face. The faces are
//!   input faces, so their order around the edge is that of their planes, never of slivers.
//! - Across an edge no face of the other operand holds, the side stays the same: such an edge is
//!   part of this operand's surface only, whether it joins two pieces of a face or the pieces of
//!   two faces along an edge of the operand.
//! - Pieces joined only by such edges, which the other operand's surface does not meet along an
//!   edge it can be read from, are classified once, from a point inside the roomiest of them.
//! - A piece bounded by exactly the same vertices as a piece of a face of the other operand lies
//!   on that face, whatever its edges say: every one of its vertices lies on both planes.
//!
//! A piece read along several edges takes the reading with the widest angle to the half-planes
//! around its edge, the most reliable one.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::f64::consts::{PI, TAU};

use super::arrange::{Line, Pieces, line};
use super::face::{PlanarFace, Pool};
use super::intersect::Cuts;
use super::planar::{self, OUTSIDE, Region, interior_point};
use crate::Point2;
use crate::tolerance::{ROUNDING, WITHIN_TOLERANCE};
use crate::vector::{Vector, cross, dot, edges, fan_solid_angle, lexicographic, scale, sub, unit};

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

/// How a face holds an edge of the pieces.
#[derive(Clone, Copy, Debug)]
enum Hold {
    /// Along its boundary, which runs from the first vertex to the second there: the face lies
    /// on one side of the edge.
    Boundary(usize, usize),
    /// Across it: the face lies on both sides.
    Across,
}

/// A piece on one side of an edge: its number, its face, and the edge directed with the piece
/// on its left.
#[derive(Clone, Copy)]
struct Side {
    piece: usize,
    face: usize,
    from: usize,
    to: usize,
}

/// The two operands' faces, those before `split` from one operand and the others from the
/// other, and the vertices.
#[derive(Clone, Copy)]
struct Operands<'a> {
    faces: &'a [PlanarFace],
    split: usize,
    pool: &'a Pool,
}

impl<'a> Operands<'a> {
    fn second(self, face: usize) -> bool {
        face >= self.split
    }

    /// The faces of the operand that face `face` does not belong to.
    fn other(self, face: usize) -> &'a [PlanarFace] {
        if self.second(face) {
            &self.faces[..self.split]
        } else {
            &self.faces[self.split..]
        }
    }
}

/// The pieces of all faces, numbered face by face: where each face's pieces start, the face of
/// each piece, and a point inside each piece that lies inside its face, with the room around it.
struct Numbering {
    start: Vec<usize>,
    face_of: Vec<usize>,
    points: Vec<Option<(f64, Point2)>>,
}

impl Numbering {
    fn new(operands: Operands, pieces: &[Pieces]) -> Self {
        let mut numbering = Self {
            start: Vec::with_capacity(pieces.len()),
            face_of: Vec::new(),
            points: Vec::new(),
        };
        for (f, cut) in pieces.iter().enumerate() {
            numbering.start.push(numbering.points.len());
            let face = &operands.faces[f];
            let at = |v: usize| face.projection.apply(operands.pool.get(v));
            for (region, &inside) in cut.regions.regions.iter().zip(&cut.inside) {
                let point = interior_point(region, at).filter(|_| inside);
                numbering.face_of.push(f);
                numbering.points.push(point);
            }
        }
        numbering
    }

    /// The pieces on each side of every edge of the pieces inside their faces.
    fn sides(&self, pieces: &[Pieces]) -> BTreeMap<Line, Vec<Side>> {
        let mut sides: BTreeMap<Line, Vec<Side>> = BTreeMap::new();
        for (f, cut) in pieces.iter().enumerate() {
            for (&(from, to), &region) in &cut.regions.left {
                if region == OUTSIDE {
                    continue;
                }
                let piece = self.start[f] + region;
                if self.points[piece].is_some() {
                    let side = Side {
                        piece,
                        face: f,
                        from,
                        to,
                    };
                    sides
                        .entry((from.min(to), from.max(to)))
                        .or_default()
                        .push(side);
                }
            }
        }
        sides
    }
}

/// The class of every piece of every face, by face then region; `None` for the regions outside
/// their face. Faces before `split` belong to one operand, the others to the other; `pieces`
/// are the regions each face is cut into, `holding` gives the lines running along each edge of
/// the pieces, and `cuts` the faces each line was cut along.
pub(super) fn classes(
    faces: &[PlanarFace],
    split: usize,
    pieces: &[Pieces],
    holding: &BTreeMap<Line, Vec<Line>>,
    cuts: &Cuts,
    pool: &Pool,
) -> Vec<Vec<Option<Class>>> {
    let operands = Operands { faces, split, pool };
    let numbering = Numbering::new(operands, pieces);
    let sides = numbering.sides(pieces);
    let count = numbering.points.len();
    let (mut class, joined) = read_sides(operands, &sides, holding, cuts, count);
    for (piece, on) in coincident(operands, pieces, &numbering) {
        class[piece] = Some(on);
    }
    let known: Vec<usize> = (0..class.len()).filter(|&p| class[p].is_some()).collect();
    spread(&joined, &mut class, known);
    let mut visited = vec![false; count];
    for piece in 0..count {
        if class[piece].is_some() || visited[piece] {
            continue;
        }
        // The roomiest piece; equally roomy ones by position, not by number, which depends on
        // which solid comes first.
        let lifted = |p: usize, q: Point2| faces[numbering.face_of[p]].lift(q);
        let members = component(&joined, piece, &mut visited);
        let roomiest = (members.iter())
            .filter_map(|&p| numbering.points[p].map(|(room, q)| (room, lifted(p, q), p)))
            .max_by(|a, b| a.0.total_cmp(&b.0).then(lexicographic(b.1, a.1)));
        let Some((_, point, seed)) = roomiest else {
            continue;
        };
        let face = &faces[numbering.face_of[seed]];
        let side = classify(point, face, operands.other(numbering.face_of[seed]), pool);
        // No piece joined to this one is classified yet, so they all take the seed's side.
        for p in members {
            class[p] = Some(side);
        }
    }
    pieces
        .iter()
        .zip(numbering.start)
        .map(|(regions, s)| class[s..s + regions.inside.len()].to_vec())
        .collect()
}

/// The pieces bounded by exactly the same vertices as a piece of a face of the other operand,
/// with the class that says so: on a face facing the same way if they run round the same way,
/// the opposite way otherwise. Every vertex of such a piece lies on both planes, so the two
/// faces coincide over it, however steep the angle between their planes: a piece a tolerance or
/// two wide where faces meet may lie within the tolerance of both.
fn coincident(operands: Operands, pieces: &[Pieces], numbering: &Numbering) -> Vec<(usize, Class)> {
    let mut by_boundary: BTreeMap<Vec<Vec<usize>>, Vec<usize>> = BTreeMap::new();
    let mut boundaries = Vec::new();
    for (f, cut) in pieces.iter().enumerate() {
        for (r, region) in cut.regions.regions.iter().enumerate() {
            let piece = numbering.start[f] + r;
            if numbering.points[piece].is_some() {
                let forward = canonical(region, false);
                by_boundary.entry(forward).or_default().push(piece);
                boundaries.push((piece, f, region));
            }
        }
    }
    let mut on = Vec::new();
    for (piece, f, region) in boundaries {
        let other = |&p: &usize| operands.second(numbering.face_of[p]) != operands.second(f);
        let matched = |reversed: bool| {
            (by_boundary.get(&canonical(region, reversed)).into_iter())
                .flatten()
                .any(other)
        };
        if matched(false) {
            on.push((piece, Class::OnSame));
        } else if matched(true) {
            on.push((piece, Class::OnOpposite));
        }
    }
    on
}

/// A region's rings, run the other way round if `reversed`, in [`planar::canonical`]'s form.
fn canonical(region: &Region, reversed: bool) -> Vec<Vec<usize>> {
    planar::canonical(
        std::iter::once(&region.outer).chain(&region.holes),
        reversed,
    )
}

/// Reads the edges of `count` pieces: for each piece, the side read along its edges, and the
/// pieces it shares an edge with that the other operand does not come near.
fn read_sides(
    operands: Operands,
    sides: &BTreeMap<Line, Vec<Side>>,
    holding: &BTreeMap<Line, Vec<Line>>,
    cuts: &Cuts,
    count: usize,
) -> (Vec<Option<Class>>, Vec<Vec<usize>>) {
    let pool = operands.pool;
    let mut borders: BTreeMap<Line, Vec<(usize, usize, usize)>> = BTreeMap::new();
    for (h, face) in operands.faces.iter().enumerate() {
        for (p, q) in face.rings.iter().flat_map(|ring| edges(ring)) {
            borders.entry(line(p, q)).or_default().push((h, p, q));
        }
    }
    let mut read: Vec<Option<(Class, f64)>> = vec![None; count];
    let mut joined: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (&edge, sides) in sides {
        let along_edge = holding.get(&edge).map_or(&[][..], Vec::as_slice);
        let (near, holds) = holds(along_edge, cuts, &borders);
        // The longest line along the edge gives its direction best. It is at least as long as
        // the edge, so it has a direction wherever the edge is long enough to be read along.
        let (start, end) = along_edge.first().copied().unwrap_or(edge);
        let axis = unit(sub(pool.get(end), pool.get(start))).unwrap_or_default();
        for operand in [false, true] {
            let mine: Vec<Side> = (sides.iter().copied())
                .filter(|side| operands.second(side.face) == operand)
                .collect();
            let theirs: Vec<(usize, Hold)> = (holds.iter())
                .filter(|&(&h, _)| operands.second(h) != operand)
                .map(|(&h, &hold)| (h, hold))
                .collect();
            if !near.iter().any(|&h| operands.second(h) != operand) {
                for pair in mine.windows(2) {
                    if pair[0].piece != pair[1].piece {
                        joined[pair[0].piece].push(pair[1].piece);
                        joined[pair[1].piece].push(pair[0].piece);
                    }
                }
                continue;
            }
            // A piece on both sides of the edge is not bounded by it: a cut ending inside it.
            let bounded = |side: &Side| mine.iter().filter(|s| s.piece == side.piece).count() == 1;
            for side in mine.iter().filter(|side| bounded(side)) {
                let towards = sub(pool.get(side.to), pool.get(side.from));
                let along = if dot(axis, towards) < 0.0 {
                    scale(axis, -1.0)
                } else {
                    axis
                };
                let reading = sector(side.face, along, &theirs, operands);
                // Equal margins are settled by the class, never by the order of the edges, which
                // depends on which solid comes first.
                if let Some((class, margin)) = reading
                    && read[side.piece]
                        .is_none_or(|(c, best)| (margin, rank(class)) > (best, rank(c)))
                {
                    read[side.piece] = Some((class, margin));
                }
            }
        }
    }
    (
        read.into_iter().map(|r| r.map(|(c, _)| c)).collect(),
        joined,
    )
}

/// A fixed order of the classes, for readings equally reliable.
const fn rank(class: Class) -> u8 {
    match class {
        Class::Inside => 0,
        Class::Outside => 1,
        Class::OnSame => 2,
        Class::OnOpposite => 3,
    }
}

/// The faces near an edge of the pieces, and those holding it and how. Near are the faces with a
/// boundary or a cut along one of the lines the edge is part of: an edge another operand's face
/// comes that close to is not part of this operand's surface only. A face holds the edge if its
/// boundary runs along such a line, or if it is cut along such a line: a cut runs only where it
/// lies inside both faces or on their boundaries.
fn holds(
    lines: &[Line],
    cuts: &Cuts,
    borders: &BTreeMap<Line, Vec<(usize, usize, usize)>>,
) -> (BTreeSet<usize>, BTreeMap<usize, Hold>) {
    let (mut near, mut holds) = (BTreeSet::new(), BTreeMap::new());
    for line in lines {
        for &h in cuts.get(line).into_iter().flatten() {
            near.insert(h);
            holds.entry(h).or_insert(Hold::Across);
        }
        for &(h, p, q) in borders.get(line).into_iter().flatten() {
            near.insert(h);
            holds.insert(h, Hold::Boundary(p, q));
        }
    }
    (near, holds)
}

/// Gives every item reached through links from the labelled items `from`, in order, the label
/// of the item it is reached from.
pub(super) fn spread<T: Copy>(links: &[Vec<usize>], class: &mut [Option<T>], from: Vec<usize>) {
    let mut queue = VecDeque::from(from);
    while let Some(piece) = queue.pop_front() {
        for &next in &links[piece] {
            if class[next].is_none() {
                class[next] = class[piece];
                queue.push_back(next);
            }
        }
    }
}

/// The pieces reached through links from a piece, itself included, in the order reached; each
/// is marked `seen`.
fn component(links: &[Vec<usize>], piece: usize, seen: &mut [bool]) -> Vec<usize> {
    seen[piece] = true;
    let mut reached = vec![piece];
    let mut queue = VecDeque::from([piece]);
    while let Some(p) = queue.pop_front() {
        for &next in &links[p] {
            if !seen[next] {
                seen[next] = true;
                reached.push(next);
                queue.push_back(next);
            }
        }
    }
    reached
}

/// The part of a vector across a unit axis.
fn across(v: Vector, axis: Vector) -> Vector {
    sub(v, scale(axis, dot(v, axis)))
}

/// The side of the other operand that a piece of face `f` lies on next to an edge with unit
/// direction `along` (the piece on its left), from the faces of the other operand holding the
/// edge; with the angle between the piece and the nearest of their half-planes around the edge,
/// which says how reliable the reading is.
fn sector(
    f: usize,
    along: Vector,
    theirs: &[(usize, Hold)],
    Operands { faces, pool, .. }: Operands,
) -> Option<(Class, f64)> {
    let face = &faces[f];
    let into = unit(cross(face.normal, along))?;
    let up = cross(along, into);
    // The half-plane met first turning anticlockwise from the piece, seen along the edge, and
    // the angle to the last one met.
    let mut nearest: Option<(f64, Vector, Vector)> = None;
    let mut farthest: f64 = 0.0;
    for &(h, hold) in theirs {
        let other = &faces[h];
        let rays = match hold {
            Hold::Across => {
                let ray = unit(cross(other.normal, along))?;
                vec![ray, scale(ray, -1.0)]
            }
            Hold::Boundary(p, q) => {
                // The face's own edge runs within the tolerance of the piece's edge, not along it.
                let edge = sub(pool.get(q), pool.get(p));
                vec![unit(across(cross(other.normal, edge), along))?]
            }
        };
        if rays.iter().any(|&ray| dot(ray, into) > 0.0) && face.coplanar(other, pool) {
            let class = if dot(face.normal, other.normal) > 0.0 {
                Class::OnSame
            } else {
                Class::OnOpposite
            };
            return Some((class, f64::INFINITY));
        }
        for ray in rays {
            let mut angle = libm::atan2(dot(ray, up), dot(ray, into));
            if angle <= 0.0 {
                angle += TAU;
            }
            farthest = farthest.max(angle);
            if nearest.is_none_or(|(least, _, _)| angle < least) {
                nearest = Some((angle, ray, other.normal));
            }
        }
    }
    let (angle, ray, normal) = nearest?;
    // The piece lies just clockwise of that half-plane: inside if turning from the half-plane
    // towards its outward normal is anticlockwise, so that the piece is on the other side.
    let class = if dot(cross(along, ray), normal) > 0.0 {
        Class::Inside
    } else {
        Class::Outside
    };
    Some((class, angle.min(TAU - farthest)))
}

/// Classifies a point of `face` against the faces of a solid.
///
/// A point inside a face of the solid coplanar with `face`, within the tolerance, lies on it.
/// Being near a face is not enough: a thin piece of face may run along an edge of the solid
/// without lying on either face there. Otherwise
/// the winding number of the solid's boundary around the point decides: about 1 inside, about 0
/// outside, and far from ambiguous once the point is more than the tolerance from every face.
fn classify(p: Vector, face: &PlanarFace, solid: &[PlanarFace], pool: &Pool) -> Class {
    let on = solid.iter().find(|other| {
        other.reaches(p)
            && other.distance(p).abs() <= WITHIN_TOLERANCE
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
/// Faces whose plane passes through the point, up to rounding, are left out. Seen from its own
/// plane a face subtends no angle, but the formula's arc tangent would then turn on the sign of
/// a zero and give ±2π per triangle. Faces merely within the tolerance stay in: a point a
/// tolerance inside a face subtends half the sphere there, and leaving that out would put a
/// piece a few tolerances wide outside the solid.
fn winding(p: Vector, solid: &[PlanarFace], pool: &Pool) -> f64 {
    let mut total = 0.0;
    let seen = solid
        .iter()
        .filter(|face| face.distance(p).abs() > ROUNDING);
    for ring in seen.flat_map(|face| &face.rings) {
        total += fan_solid_angle(p, ring.iter().map(|&v| pool.get(v)));
    }
    total / (4.0 * PI)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean::planar::regions;

    /// A square from −1 to 1 in x and y, facing up, followed by the given points.
    fn pool_with(points: &[Vector]) -> Pool {
        let square = [
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
        ];
        Pool::of(square.iter().chain(points).copied().collect())
    }

    /// Reads a piece of face 0 along the y axis, on the left of `along`, against faces 1 and up.
    fn reading(
        faces: &[PlanarFace],
        pool: &Pool,
        along: Vector,
        theirs: &[(usize, Hold)],
    ) -> Option<Class> {
        let operands = Operands {
            faces,
            split: 1,
            pool,
        };
        sector(0, along, theirs, operands).map(|(class, _)| class)
    }

    #[test]
    fn a_face_across_an_edge_has_the_other_solid_on_its_inner_side() {
        // The plane x = 0 facing −x: the other solid lies towards +x.
        let pool = pool_with(&[
            [0.0, -1.0, -1.0],
            [0.0, -1.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, -1.0],
        ]);
        let faces = [
            PlanarFace::new(vec![vec![0, 1, 2, 3]], &pool, 0),
            PlanarFace::new(vec![vec![4, 5, 6, 7]], &pool, 1),
        ];
        let across = [(1, Hold::Across)];
        // On the left of +y, seen from above, lies −x.
        assert_eq!(
            reading(&faces, &pool, [0.0, 1.0, 0.0], &across),
            Some(Class::Outside)
        );
        assert_eq!(
            reading(&faces, &pool, [0.0, -1.0, 0.0], &across),
            Some(Class::Inside)
        );
    }

    #[test]
    fn faces_along_an_edge_hold_the_solid_between_them() {
        // The other solid fills x > 0, z < 0 near the y axis: a top facing +z and a side facing
        // −x, along the axis. The piece lies in the plane x + z = 0, which passes between them.
        let pool = Pool::of(vec![
            [-1.0, -1.0, 1.0],
            [1.0, -1.0, -1.0],
            [1.0, 1.0, -1.0],
            [-1.0, 1.0, 1.0],
            [0.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, -1.0],
            [0.0, 1.0, -1.0],
        ]);
        let faces = [
            PlanarFace::new(vec![vec![0, 1, 2, 3]], &pool, 2),
            PlanarFace::new(vec![vec![4, 5, 6, 7]], &pool, 3),
            PlanarFace::new(vec![vec![8, 4, 7, 9]], &pool, 4),
        ];
        let along_edge = [(1, Hold::Boundary(7, 4)), (2, Hold::Boundary(4, 7))];
        // On the left of +y lies the half towards −x, +z: outside.
        assert_eq!(
            reading(&faces, &pool, [0.0, 1.0, 0.0], &along_edge),
            Some(Class::Outside)
        );
        assert_eq!(
            reading(&faces, &pool, [0.0, -1.0, 0.0], &along_edge),
            Some(Class::Inside)
        );
    }

    #[test]
    fn a_piece_reaching_into_a_coplanar_face_lies_on_it() {
        // Back to back with the square: the half x > 0 of its plane, facing down.
        let pool = pool_with(&[
            [0.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, -1.0, 0.0],
        ]);
        let faces = [
            PlanarFace::new(vec![vec![0, 1, 2, 3]], &pool, 5),
            PlanarFace::new(vec![vec![4, 5, 6, 7]], &pool, 6),
        ];
        assert_eq!(
            reading(
                &faces,
                &pool,
                [0.0, -1.0, 0.0],
                &[(1, Hold::Boundary(4, 5))]
            ),
            Some(Class::OnOpposite)
        );
        let operands = Operands {
            faces: &faces,
            split: 1,
            pool: &pool,
        };
        let point = [0.5, 0.0, 0.0];
        assert_eq!(
            classify(point, &faces[0], operands.other(0), &pool),
            Class::OnOpposite
        );
    }

    #[test]
    fn a_cut_holds_an_edge_across_and_a_boundary_along_it() {
        // Faces 0 and 1 are cut along (8, 9); face 2 has its own edge there.
        let cuts: Cuts = [((8, 9), [0, 1].into())].into();
        let borders = BTreeMap::from([((8, 9), vec![(2, 9, 8)])]);
        let (near, holding) = holds(&[(8, 9)], &cuts, &borders);
        assert_eq!(near, [0, 1, 2].into());
        assert!(matches!(holding[&0], Hold::Across));
        assert!(matches!(holding[&2], Hold::Boundary(9, 8)));
        let (near, holding) = holds(&[(10, 11)], &cuts, &borders);
        assert!(near.is_empty() && holding.is_empty());
    }

    #[test]
    fn a_class_spreads_over_links_and_each_component_is_found_once() {
        // Pieces 0–1–2 joined, 3 alone; only piece 1 is classified.
        let links = vec![vec![1], vec![0, 2], vec![1], Vec::new()];
        let mut class = vec![None, Some(Class::Inside), None, None];
        spread(&links, &mut class, vec![1]);
        assert_eq!(
            class,
            [Some(Class::Inside); 3]
                .into_iter()
                .chain([None])
                .collect::<Vec<_>>()
        );
        let mut seen = vec![false; 4];
        assert_eq!(component(&links, 2, &mut seen), [2, 1, 0]);
        assert_eq!(seen, [true, true, true, false]);
    }

    #[test]
    fn an_edge_along_a_cut_of_the_other_solid_joins_nothing() {
        // Two pieces of the square on either side of a cut along x = 0 by the other solid's
        // face: each takes its side from the cut, and the cut does not join them.
        let pool = pool_with(&[
            [0.0, -1.0, -1.0],
            [0.0, -1.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, -1.0],
            [0.0, -0.6, 0.0],
            [0.0, 0.9, 0.0],
        ]);
        let faces = [
            PlanarFace::new(vec![vec![0, 1, 2, 3]], &pool, 7),
            PlanarFace::new(vec![vec![4, 5, 6, 7]], &pool, 8),
        ];
        let operands = Operands {
            faces: &faces,
            split: 1,
            pool: &pool,
        };
        let side = |piece, from, to| Side {
            piece,
            face: 0,
            from,
            to,
        };
        let sides = BTreeMap::from([((8, 9), vec![side(0, 8, 9), side(1, 9, 8)])]);
        let holding = BTreeMap::from([((8, 9), vec![(8, 9)])]);
        let cuts: Cuts = [((8, 9), [0, 1].into())].into();
        let (read, joined) = read_sides(operands, &sides, &holding, &cuts, 2);
        // On the left of +y, seen from above, lies −x, away from the other solid.
        assert_eq!(read, [Some(Class::Outside), Some(Class::Inside)]);
        assert!(joined.iter().all(Vec::is_empty));
        // Without the cut, the edge is the square's own and joins them.
        let (_, joined) = read_sides(operands, &sides, &holding, &Cuts::new(), 2);
        assert_eq!(joined, [vec![1], vec![0]]);
    }

    /// One face's pieces, all inside it: the regions the given rings bound.
    fn pieces_of(face: &PlanarFace, rings: &[Vec<usize>], pool: &Pool) -> Pieces {
        let at = |v: usize| face.projection.apply(pool.get(v));
        let regions = regions(rings.to_vec(), at);
        let inside = vec![true; regions.regions.len()];
        Pieces { regions, inside }
    }

    #[test]
    fn pieces_bounded_alike_lie_on_each_other() {
        // A triangle of each solid's face, with the same corners: running the same way round
        // they face the same way; the other way round, opposite ways (the first piece matches
        // both, and the same way round is found first). A piece sharing only two corners lies
        // on nothing.
        let pool = Pool::of(vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ]);
        let faces = [
            PlanarFace::new(vec![vec![0, 1, 3, 2]], &pool, 9),
            PlanarFace::new(vec![vec![0, 1, 3, 2]], &pool, 10),
            PlanarFace::new(vec![vec![0, 2, 3, 1]], &pool, 11),
        ];
        let operands = Operands {
            faces: &faces,
            split: 1,
            pool: &pool,
        };
        let same = [
            pieces_of(&faces[0], &[vec![0, 1, 2], vec![1, 3, 2]], &pool),
            pieces_of(&faces[1], &[vec![0, 1, 2]], &pool),
            pieces_of(&faces[2], &[vec![0, 2, 1]], &pool),
        ];
        let numbering = Numbering::new(operands, &same);
        assert_eq!(
            coincident(operands, &same, &numbering),
            [
                (0, Class::OnSame),
                (2, Class::OnSame),
                (3, Class::OnOpposite),
            ]
        );
    }

    #[test]
    fn a_ring_through_its_smallest_vertex_twice_has_one_form() {
        let forms = |ring: &[usize]| {
            let region = Region {
                outer: ring.to_vec(),
                holes: Vec::new(),
            };
            canonical(&region, false)
        };
        assert_eq!(forms(&[5, 1, 9, 1, 7]), forms(&[1, 7, 5, 1, 9]));
        assert_eq!(forms(&[5, 1, 9, 1, 7]), [vec![1, 7, 5, 1, 9]]);
    }
}
