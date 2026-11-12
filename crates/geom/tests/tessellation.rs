//! Tests of `Solid::tessellate` (GEO-05, ENG-02): boxes, prisms over non-convex profiles, with
//! holes and arcs, slanted extrusions and boolean results, then random extrusions and booleans.
//!
//! Every mesh goes through [`check_with`], which holds the invariants of the specification: the
//! triangle count of each face, indices in range, no degenerate triangle, each face's area kept,
//! normals agreeing with the face, a closed and consistently oriented mesh, and the enclosed
//! volume equal to the solid's; random boolean results, simple and planar only within the
//! tolerance, are held to them as ADR 0022 relaxes them.

use std::collections::HashMap;

use geom::{
    ArcDirection, Edge, FaceInput, LINEAR_TOLERANCE, MERGE_CHAIN_WIDTH, Orientation, Point2,
    Point3, Profile, Solid, TessellationError, TriangleMesh, Vector3, orient2d,
};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

type V = [f64; 3];

const fn v(p: Point3) -> V {
    [p.x, p.y, p.z]
}

fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: V) -> f64 {
    dot(a, a).sqrt()
}

/// The area vector of a ring by Newell's method.
fn newell(ring: &[V]) -> V {
    let mut n = [0.0; 3];
    for (i, p) in ring.iter().enumerate() {
        let q = ring[(i + 1) % ring.len()];
        n[0] += (p[1] - q[1]) * (p[2] + q[2]);
        n[1] += (p[2] - q[2]) * (p[0] + q[0]);
        n[2] += (p[0] - q[0]) * (p[1] + q[1]);
    }
    n
}

/// What the terms of [`newell`] add up to in magnitude, which their rounding is relative to.
fn newell_magnitude(ring: &[V]) -> f64 {
    (0..ring.len())
        .map(|i| {
            let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
            norm(sub(p, q)) * norm(add(p, q))
        })
        .sum()
}

/// The exact orientation of a triangle seen along the largest component of `normal`: positive
/// when it turns the same way as the face.
fn orientation_in(normal: V, corners: [V; 3]) -> Orientation {
    let axis = (0..3).fold(0, |best, i| {
        if normal[i].abs() > normal[best].abs() {
            i
        } else {
            best
        }
    });
    let (first, second) = ((axis + 1) % 3, (axis + 2) % 3);
    let [a, b, c] = corners.map(|p| Point2::new(p[first], p[second]));
    let turn = orient2d(a, b, c).unwrap_or(Orientation::Zero);
    if normal[axis] > 0.0 {
        turn
    } else {
        turn.reversed()
    }
}

/// Position bits, so equal positions are one key exactly.
fn key(p: Point3) -> [u64; 3] {
    // Adding zero turns −0 into +0, which is the same position.
    [
        (p.x + 0.0).to_bits(),
        (p.y + 0.0).to_bits(),
        (p.z + 0.0).to_bits(),
    ]
}

/// How strictly [`check_with`] holds a solid to the invariants of its mesh, as the
/// `extruded_boolean` and `extrusion` fuzz targets do (`Contract` in `fuzz/src/lib.rs`, which
/// these tests cannot depend on; keep the two alike).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Contract {
    /// Faces exactly planar and simple: tessellation succeeds, every triangle turns strictly
    /// counterclockwise, and by position every directed edge has exactly one reverse.
    Exact,
    /// Faces planar and simple only within the tolerance, as boolean results at random: a face
    /// may fail if its rings come within twice `MERGE_CHAIN_WIDTH` of themselves, slivers that
    /// narrow may turn the wrong way, and an edge may be used more than once if it has as many
    /// reverses (ADR 0022).
    WithinTolerance,
}

/// Tessellates a solid whose faces are exact and checks every invariant of its mesh.
fn check(solid: &Solid) -> Result<TriangleMesh, String> {
    check_with(solid, Contract::Exact)?.ok_or_else(|| "a face failed".into())
}

/// Tessellates a solid and checks every invariant of its mesh against it, as `contract` says;
/// `None` for a face the contract allows to fail.
fn check_with(solid: &Solid, contract: Contract) -> Result<Option<TriangleMesh>, String> {
    let mesh = match solid.tessellate() {
        Ok(mesh) => mesh,
        Err(TessellationError::Untriangulable { face })
            if contract == Contract::WithinTolerance && has_close_features(solid, face) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error.to_string()),
    };
    if mesh.normals.len() != mesh.positions.len() || mesh.indices.len() % 3 != 0 {
        return Err("mismatched buffers".into());
    }
    let mut triangles = mesh.indices.as_chunks::<3>().0.iter();
    let mut base = 0;
    for (face, input) in solid.to_faces().iter().enumerate() {
        let rings: Vec<&Vec<usize>> = std::iter::once(&input.outer).chain(&input.holes).collect();
        let n: usize = rings.iter().map(|r| r.len()).sum();
        let (mut area, mut rounding) = ([0.0; 3], 0.0);
        for ring in &rings {
            let points: Vec<V> = ring.iter().map(|&i| v(solid.vertices()[i])).collect();
            area = add(area, newell(&points));
            rounding += newell_magnitude(&points);
        }
        let unit = area.map(|c| c / norm(area));
        for normal in &mesh.normals[base..base + n] {
            if norm(sub([normal.x, normal.y, normal.z], unit)) > 1e-12 {
                return Err(format!("face {face}: vertex normal off the face normal"));
            }
        }
        let mut covered = [0.0; 3];
        for _ in 0..n + 2 * input.holes.len() - 2 {
            let triangle = triangles
                .next()
                .ok_or(format!("face {face}: too few triangles"))?;
            let mut corners = [[0.0; 3]; 3];
            for (corner, &index) in corners.iter_mut().zip(triangle) {
                let index = usize::try_from(index).map_err(|e| e.to_string())?;
                if !(base..base + n).contains(&index) {
                    return Err(format!("face {face}: index outside the face"));
                }
                *corner = v(mesh.positions[index]);
            }
            let [a, b, c] = corners;
            let doubled = cross(sub(b, a), sub(c, a));
            let counterclockwise = orientation_in(area, corners) == Orientation::Positive;
            let sliver = contract == Contract::WithinTolerance && is_sliver(corners);
            if !counterclockwise && !sliver {
                return Err(format!("face {face}: a degenerate or flipped triangle"));
            }
            if counterclockwise && dot(doubled, unit) <= 0.0 {
                return Err(format!("face {face}: triangle normal against the face"));
            }
            covered = add(covered, doubled);
            rounding += norm(sub(b, a)) * norm(sub(c, a));
        }
        // Area vectors add up exactly, even over a face planar only within the tolerance;
        // Newell's sums round relative to the coordinates, not the area.
        if norm(sub(covered, area)) > 2e-9 * norm(area) / 2.0 + 2e-14 * rounding {
            return Err(format!("face {face}: the triangles do not cover the face"));
        }
        base += n;
    }
    if triangles.next().is_some() || base != mesh.positions.len() {
        return Err("more triangles or positions than the faces need".into());
    }
    check_closed(&mesh, contract)?;
    check_volume(solid, &mesh)?;
    Ok(Some(mesh))
}

/// By position, every directed edge has exactly one reverse, or for
/// [`Contract::WithinTolerance`] as many reverses as uses.
fn check_closed(mesh: &TriangleMesh, contract: Contract) -> Result<(), String> {
    let mut edges: HashMap<([u64; 3], [u64; 3]), usize> = HashMap::new();
    for t in mesh.indices.as_chunks::<3>().0 {
        for (i, j) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (a, b) = (mesh.positions[i as usize], mesh.positions[j as usize]);
            *edges.entry((key(a), key(b))).or_default() += 1;
        }
    }
    for (&(a, b), &count) in &edges {
        let reverses = edges.get(&(b, a)).copied();
        let closed = match contract {
            Contract::Exact => count == 1 && reverses == Some(1),
            Contract::WithinTolerance => reverses == Some(count),
        };
        if !closed {
            return Err("the mesh is not closed and consistently oriented".into());
        }
    }
    Ok(())
}

/// The volume the triangles enclose, by the divergence theorem, equals the solid's, within
/// rounding and the tolerance times the area, which faces off their plane may sweep.
fn check_volume(solid: &Solid, mesh: &TriangleMesh) -> Result<(), String> {
    let apex = mesh.positions.first().map_or([0.0; 3], |&p| v(p));
    let (mut six_volume, mut rounding) = (0.0, 0.0);
    for t in mesh.indices.as_chunks::<3>().0 {
        let [a, b, c] = t.map(|i| sub(v(mesh.positions[i as usize]), apex));
        six_volume += dot(a, cross(b, c));
        rounding += norm(a) * norm(b) * norm(c);
    }
    let surface: f64 = mesh
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| v(mesh.positions[i as usize]));
            norm(cross(sub(b, a), sub(c, a))) / 2.0
        })
        .sum();
    let (volume, expected) = (six_volume / 6.0, solid.volume());
    let slack = 1e-9 * expected.abs() + 1e-14 * rounding + LINEAR_TOLERANCE * surface;
    if (volume - expected).abs() > slack {
        return Err(format!("volume {volume} instead of {expected}"));
    }
    Ok(())
}

/// Whether a triangle is no wider than twice `MERGE_CHAIN_WIDTH`, the most a sliver the
/// tessellation may leave turning the wrong way is wide once out of its face's projection.
fn is_sliver([a, b, c]: [V; 3]) -> bool {
    let longest = [sub(b, a), sub(c, b), sub(a, c)]
        .map(norm)
        .into_iter()
        .fold(0.0, f64::max);
    norm(cross(sub(b, a), sub(c, a))) <= 2.0 * MERGE_CHAIN_WIDTH * longest
}

/// Whether two edges of a face's rings that share no vertex come within twice
/// `MERGE_CHAIN_WIDTH` of each other.
fn has_close_features(solid: &Solid, face: usize) -> bool {
    let input = &solid.to_faces()[face];
    let point = |i: usize| v(solid.vertices()[i]);
    let edges: Vec<(usize, usize)> = std::iter::once(&input.outer)
        .chain(&input.holes)
        .flat_map(|ring| (0..ring.len()).map(|i| (ring[i], ring[(i + 1) % ring.len()])))
        .collect();
    edges.iter().enumerate().any(|(i, &(a, b))| {
        edges[i + 1..].iter().any(|&(c, d)| {
            ![c, d].contains(&a)
                && ![c, d].contains(&b)
                && segment_distance(point(a), point(b), point(c), point(d))
                    <= 2.0 * MERGE_CHAIN_WIDTH
        })
    })
}

/// The distance between segments `p q` and `r s` (Ericson, "Real-Time Collision Detection",
/// 5.1.9).
fn segment_distance(start: V, end: V, other_start: V, other_end: V) -> f64 {
    let (along, other, gap) = (
        sub(end, start),
        sub(other_end, other_start),
        sub(start, other_start),
    );
    let (length, other_length) = (dot(along, along), dot(other, other));
    let (gap_other, gap_along, cosine) = (dot(other, gap), dot(along, gap), dot(along, other));
    let denominator = length * other_length - cosine * cosine;
    let mut t = if denominator > 0.0 {
        ((cosine * gap_other - gap_along * other_length) / denominator).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut u = (cosine * t + gap_other) / other_length;
    if u < 0.0 {
        u = 0.0;
        t = (-gap_along / length).clamp(0.0, 1.0);
    } else if u > 1.0 {
        u = 1.0;
        t = ((cosine - gap_along) / length).clamp(0.0, 1.0);
    }
    let closest = add(start, along.map(|x| x * t));
    let other_closest = add(other_start, other.map(|x| x * u));
    norm(sub(closest, other_closest))
}

fn polygon(points: &[(f64, f64)]) -> Vec<Edge> {
    (0..points.len())
        .map(|i| {
            let (x0, y0) = points[i];
            let (x1, y1) = points[(i + 1) % points.len()];
            Edge::Line {
                start: Point2::new(x0, y0),
                end: Point2::new(x1, y1),
            }
        })
        .collect()
}

fn square(x: f64, y: f64, side: f64) -> Vec<Edge> {
    polygon(&[(x, y), (x + side, y), (x + side, y + side), (x, y + side)])
}

fn circle(x: f64, y: f64, radius: f64) -> Vec<Edge> {
    let start = Point2::new(x + radius, y);
    vec![Edge::Arc {
        start,
        end: start,
        centre: Point2::new(x, y),
        direction: ArcDirection::CounterClockwise,
    }]
}

fn prism(outer: Vec<Edge>, holes: Vec<Vec<Edge>>, direction: Vector3) -> Result<Solid, String> {
    let profile = Profile::new(outer, holes).map_err(|e| e.to_string())?;
    Solid::extrude(&profile, direction).map_err(|e| e.to_string())
}

fn up(height: f64) -> Vector3 {
    Vector3::new(0.0, 0.0, height)
}

fn cuboid(min: [f64; 3], max: [f64; 3]) -> Result<Solid, String> {
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = max;
    let base = polygon(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
    prism(base, Vec::new(), up(z1 - z0))?.translated_z(z0)
}

/// Moving a solid along z, through its faces, for boxes that do not start at zero.
trait TranslateZ {
    fn translated_z(self, dz: f64) -> Result<Solid, String>;
}

impl TranslateZ for Solid {
    fn translated_z(self, dz: f64) -> Result<Solid, String> {
        let vertices = self
            .vertices()
            .iter()
            .map(|p| Point3::new(p.x, p.y, p.z + dz))
            .collect();
        Solid::from_faces(vertices, &self.to_faces()).map_err(|e| e.to_string())
    }
}

/// Unwraps a boolean result that must exist.
fn solid(result: Result<Option<Solid>, geom::BooleanError>) -> Result<Solid, String> {
    result
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "an empty result".into())
}

#[test]
fn a_box_gives_two_triangles_per_face() -> Result<(), String> {
    let mesh = check(&cuboid([0.0; 3], [2.0, 3.0, 4.0])?)?;
    assert_eq!(mesh.triangle_count(), 12);
    assert_eq!(mesh.positions.len(), 24);
    Ok(())
}

#[test]
fn every_face_normal_points_outward() -> Result<(), String> {
    let mesh = check(&cuboid([0.0; 3], [1.0; 3])?)?;
    for (p, n) in mesh.positions.iter().zip(&mesh.normals) {
        assert!(dot(sub(v(*p), [0.5; 3]), [n.x, n.y, n.z]) > 0.0);
    }
    Ok(())
}

#[test]
fn an_l_shaped_prism_triangulates_its_reflex_corner() -> Result<(), String> {
    let outer = polygon(&[
        (0.0, 0.0),
        (3.0, 0.0),
        (3.0, 1.0),
        (1.0, 1.0),
        (1.0, 2.0),
        (0.0, 2.0),
    ]);
    let mesh = check(&prism(outer, Vec::new(), up(1.0))?)?;
    // Two caps of 6 − 2 triangles and six side quads.
    assert_eq!(mesh.triangle_count(), 2 * 4 + 6 * 2);
    Ok(())
}

#[test]
fn a_comb_with_many_reflex_corners_triangulates() -> Result<(), String> {
    let mut points = vec![(0.0, 0.0), (9.0, 0.0)];
    for tooth in (0..4).rev() {
        let x = f64::from(tooth) * 2.0;
        points.extend([(x + 1.0, 3.0), (x + 1.0, 1.0), (x + 0.5, 1.0), (x, 3.0)]);
    }
    let sides = points.len();
    let mesh = check(&prism(polygon(&points), Vec::new(), up(0.5))?)?;
    assert_eq!(mesh.triangle_count(), 2 * (sides - 2) + 2 * sides);
    Ok(())
}

#[test]
fn holes_are_bridged_into_their_face() -> Result<(), String> {
    let holes = vec![
        square(1.0, 1.0, 1.0),
        square(3.0, 1.0, 1.0),
        square(1.0, 3.0, 1.5),
    ];
    let mesh = check(&prism(square(0.0, 0.0, 6.0), holes, up(2.0))?)?;
    // Caps: 16 vertices and 3 holes give 16 + 6 − 2 triangles each; 16 side quads.
    assert_eq!(mesh.triangle_count(), 2 * 20 + 16 * 2);
    Ok(())
}

#[test]
fn holes_side_by_side_along_the_first_axis_are_bridged() -> Result<(), String> {
    // Their farthest vertices share an abscissa, so the order of bridging relies on the second.
    let holes = vec![
        square(1.0, 1.0, 1.0),
        square(1.0, 3.0, 1.0),
        square(1.0, 5.0, 1.0),
    ];
    check(&prism(square(0.0, 0.0, 7.0), holes, up(1.0))?)?;
    Ok(())
}

#[test]
fn faceted_arcs_and_round_holes_triangulate() -> Result<(), String> {
    let outer = circle(0.0, 0.0, 5.0);
    let holes = vec![
        circle(-2.0, 0.0, 1.0),
        circle(2.0, 0.0, 1.0),
        square(-0.5, 2.0, 1.0),
    ];
    check(&prism(outer, holes, up(3.0))?)?;
    Ok(())
}

#[test]
fn slanted_extrusions_keep_their_faces_flat() -> Result<(), String> {
    let holes = vec![circle(0.0, 0.0, 0.4)];
    check(&prism(
        square(-1.0, -1.0, 2.0),
        holes,
        Vector3::new(2.0, -1.5, 0.7),
    )?)?;
    Ok(())
}

#[test]
fn faces_seen_along_every_axis_both_ways_triangulate() -> Result<(), String> {
    // A prism lying along x has caps facing ±x, and its sides face ±y and ±z.
    let standing = prism(square(0.0, 0.0, 1.0), Vec::new(), up(5.0))?;
    let turned = standing
        .vertices()
        .iter()
        .map(|p| Point3::new(p.z, p.x, p.y))
        .collect();
    let lying = Solid::from_faces(turned, &standing.to_faces()).map_err(|e| e.to_string())?;
    check(&lying)?;
    Ok(())
}

#[test]
fn a_box_with_a_through_hole_triangulates() -> Result<(), String> {
    let block = cuboid([0.0; 3], [4.0, 4.0, 2.0])?;
    let bar = cuboid([1.0, -1.0, 0.5], [2.0, 5.0, 1.5])?;
    let holed = solid(block.difference(&bar))?;
    assert_eq!(holed.shells()[0].genus(), 1);
    check(&holed)?;
    Ok(())
}

#[test]
fn a_box_with_a_blind_and_a_through_hole_triangulates() -> Result<(), String> {
    let block = cuboid([0.0; 3], [6.0, 6.0, 2.0])?;
    let through = cuboid([1.0, 1.0, -1.0], [2.0, 2.0, 3.0])?;
    let blind = cuboid([3.0, 3.0, 1.0], [5.0, 4.0, 3.0])?;
    let holed = solid(solid(block.difference(&through))?.difference(&blind))?;
    check(&holed)?;
    Ok(())
}

#[test]
fn unions_and_intersections_triangulate() -> Result<(), String> {
    let a = cuboid([0.0; 3], [2.0, 2.0, 2.0])?;
    let b = cuboid([1.0, 1.0, 1.0], [3.0, 3.0, 3.0])?;
    // The union's faces are L-shaped.
    check(&solid(a.union(&b))?)?;
    check(&solid(a.intersection(&b))?)?;
    let ring = prism(circle(0.0, 0.0, 2.0), vec![circle(0.0, 0.0, 1.0)], up(1.0))?;
    let bar = cuboid([-3.0, -0.25, 0.25], [3.0, 0.25, 2.0])?;
    check(&solid(ring.difference(&bar))?)?;
    Ok(())
}

#[test]
fn a_solid_with_a_void_triangulates_both_shells() -> Result<(), String> {
    let hollow = solid(cuboid([0.0; 3], [3.0; 3])?.difference(&cuboid([1.0; 3], [2.0; 3])?))?;
    assert_eq!(hollow.shells().len(), 2);
    assert_eq!(check(&hollow)?.triangle_count(), 24);
    Ok(())
}

#[test]
fn a_face_whose_hole_lies_outside_it_is_refused_by_index() -> Result<(), String> {
    // Validation leaves holes inside their face to the operations that build solids; the
    // tessellation must still refuse such a face rather than loop or panic.
    let base = prism(square(0.0, 0.0, 1.0), Vec::new(), up(1.0))?;
    let mut vertices = base.vertices().to_vec();
    let mut faces = base.to_faces();
    let level = |faces: &[FaceInput], vertices: &[Point3], high: bool| {
        faces
            .iter()
            .position(|f| f.outer.iter().all(|&i| (vertices[i].z > 0.5) == high))
            .ok_or("a cap")
    };
    let (top, bottom) = (
        level(&faces, &vertices, true)?,
        level(&faces, &vertices, false)?,
    );
    // A small tube outside the square, from a hole in the top face to one in the bottom face.
    let first = vertices.len();
    for z in [1.0, 0.0] {
        for (x, y) in [(3.0, 0.0), (3.0, 0.5), (3.5, 0.5), (3.5, 0.0)] {
            vertices.push(Point3::new(x, y, z));
        }
    }
    faces[top].holes.push((first..first + 4).collect());
    faces[bottom]
        .holes
        .push((first + 4..first + 8).rev().collect());
    for i in 0..4 {
        let (a, b) = (first + i, first + (i + 1) % 4);
        faces.push(FaceInput {
            outer: vec![b, a, a + 4, b + 4],
            holes: Vec::new(),
        });
    }
    let solid = Solid::from_faces(vertices, &faces).map_err(|e| e.to_string())?;
    let error = solid.tessellate().expect_err("a hole outside its face");
    assert!(matches!(
        error,
        TessellationError::Untriangulable { face } if face == top || face == bottom
    ));
    // The message names the face, never coordinates (SEC-03).
    let message = error.to_string();
    assert!(message.contains("cannot be triangulated"));
    assert!(!message.contains('.'));
    Ok(())
}

// Random solids.

/// A star-shaped polygon around (cx, cy): corners at increasing angles, at radii between half
/// and the whole of `radius`, so it is simple but rarely convex.
fn star(cx: f64, cy: f64, radius: f64, corners: &[(f64, f64)]) -> Vec<Edge> {
    let count = corners.len();
    let points: Vec<(f64, f64)> = corners
        .iter()
        .enumerate()
        .map(|(i, &(jitter, reach))| {
            #[expect(clippy::cast_precision_loss, reason = "a few corners")]
            let angle = (i as f64 + jitter) * std::f64::consts::TAU / count as f64;
            let r = radius * (0.5 + 0.5 * reach);
            (cx + r * libm::cos(angle), cy + r * libm::sin(angle))
        })
        .collect();
    polygon(&points)
}

/// Holes inside the inner half of a star: squares or circles, on a ring around its centre.
fn holes(cx: f64, cy: f64, radius: f64, kinds: &[bool]) -> Vec<Vec<Edge>> {
    let count = kinds.len();
    kinds
        .iter()
        .enumerate()
        .map(|(i, &round)| {
            #[expect(clippy::cast_precision_loss, reason = "a few holes")]
            let angle = i as f64 * std::f64::consts::TAU / count as f64;
            let size = radius * 0.08;
            let (x, y) = (
                cx + 0.25 * radius * libm::cos(angle),
                cy + 0.25 * radius * libm::sin(angle),
            );
            if round {
                circle(x, y, size)
            } else {
                square(x - size, y - size, size)
            }
        })
        .collect()
}

#[derive(Clone, Debug)]
struct Prism {
    centre: (f64, f64),
    radius: f64,
    corners: Vec<(f64, f64)>,
    holes: Vec<bool>,
    direction: (f64, f64, f64),
}

fn prisms() -> impl Strategy<Value = Prism> {
    (
        (-100.0..100.0_f64, -100.0..100.0_f64),
        0.5..20.0_f64,
        prop::collection::vec((0.0..0.9_f64, 0.0..1.0_f64), 3..14),
        prop::collection::vec(any::<bool>(), 0..4),
        (-2.0..2.0_f64, -2.0..2.0_f64, 0.05..5.0_f64),
    )
        .prop_map(|(centre, radius, corners, holes, direction)| Prism {
            centre,
            radius,
            corners,
            holes,
            direction,
        })
}

fn build(prism: &Prism) -> Option<Solid> {
    let (cx, cy) = prism.centre;
    let outer = star(cx, cy, prism.radius, &prism.corners);
    let profile = Profile::new(outer, holes(cx, cy, prism.radius, &prism.holes)).ok()?;
    let (dx, dy, dz) = prism.direction;
    let scale = prism.radius;
    Solid::extrude(&profile, Vector3::new(dx * scale, dy * scale, dz * scale)).ok()
}

fn config(cases: u32, seed: u64) -> ProptestConfig {
    ProptestConfig {
        cases,
        rng_seed: RngSeed::Fixed(seed),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config(300, 69))]

    #[test]
    fn random_extrusions_tessellate(prism in prisms()) {
        if let Some(solid) = build(&prism) {
            check(&solid).map_err(TestCaseError::fail)?;
        }
    }
}

proptest! {
    #![proptest_config(config(60, 70))]

    #[test]
    fn random_boolean_results_tessellate(
        a in prisms(),
        b in prisms(),
        shift in (-1.0..1.0_f64, -1.0..1.0_f64, -1.0..1.0_f64),
    ) {
        let mut b = b;
        // Near the first operand, so they overlap.
        b.centre = (a.centre.0 + shift.0 * a.radius, a.centre.1 + shift.1 * a.radius);
        b.radius = b.radius.min(a.radius * 1.5);
        let (Some(first), Some(second)) = (build(&a), build(&b)) else {
            return Ok(());
        };
        let second = second.translated_z(shift.2 * a.radius).map_err(TestCaseError::fail)?;
        for result in [first.union(&second), first.difference(&second), first.intersection(&second)] {
            if let Ok(Some(solid)) = result {
                check_with(&solid, Contract::WithinTolerance).map_err(TestCaseError::fail)?;
            }
        }
    }
}
