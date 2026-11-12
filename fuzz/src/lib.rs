//! Generators shared by the geometry kernel's fuzz targets: they turn raw tuples, which
//! libFuzzer's `Arbitrary` builds and mutates, into profile loops and extrusion directions.
//!
//! Points lie on a quarter-metre grid within ±32 m, so vertices, edges and loops of random input
//! coincide, touch and overlap often: the degenerate cases a tolerance policy must decide. A
//! scale byte moves the whole profile to other magnitudes: near the linear tolerance, near the
//! model extent, or beyond it, and one value makes a coordinate NaN.

use std::collections::HashMap;

use geom::{
    ArcDirection, BooleanError, Containment, Edge, LINEAR_TOLERANCE, MERGE_CHAIN_WIDTH,
    Orientation, Point2, Point3, Solid, TessellationError, Vector3, orient2d,
};

/// Most edges per loop: enough for every topology, few enough to keep each input fast.
pub const MAX_LOOP_EDGES: usize = 16;
/// Most holes per profile; see [`MAX_LOOP_EDGES`].
pub const MAX_HOLES: usize = 4;

/// One edge: its start on the grid, in quarter metres, and its kind. A kind below 128 is a line;
/// otherwise an arc whose centre lies on the perpendicular bisector of its chord, `kind & 0x3f`
/// minus 32 eighths of the chord away, counterclockwise when `kind & 0x40` is set.
pub type RawEdge = (i8, i8, u8);

/// An outer loop, holes, and a scale byte (see [`scale`]).
pub type RawProfile = (Vec<RawEdge>, Vec<Vec<RawEdge>>, u8);

/// The factor applied to every coordinate of a profile: a power of two from 2^-24 (a quarter
/// metre becomes a few tolerances) to 2^9 (32 m becomes beyond the model extent), so scaling
/// stays exact; 255 is NaN.
#[must_use]
pub fn scale(raw: u8) -> f64 {
    if raw == u8::MAX {
        return f64::NAN;
    }
    libm::ldexp(1.0, i32::from(raw % 34) - 24)
}

/// A closed loop through the raw edges: each edge ends where the next one starts.
#[must_use]
pub fn edge_loop(raw: &[RawEdge], scale: f64) -> Vec<Edge> {
    let raw = &raw[..raw.len().min(MAX_LOOP_EDGES)];
    let point =
        |&(x, y, _): &RawEdge| Point2::new(f64::from(x) / 4.0 * scale, f64::from(y) / 4.0 * scale);
    (0..raw.len())
        .map(|i| {
            let (start, end) = (point(&raw[i]), point(&raw[(i + 1) % raw.len()]));
            let kind = raw[i].2;
            if kind < 128 {
                return Edge::Line { start, end };
            }
            let offset = (f64::from(kind & 0x3f) - 32.0) / 8.0;
            // A full circle (start == end) has no chord: its centre goes along x instead.
            let (dx, dy) = if start == end {
                (scale, 0.0)
            } else {
                (start.y - end.y, end.x - start.x)
            };
            let centre = Point2::new(
                f64::midpoint(start.x, end.x) + offset * dx,
                f64::midpoint(start.y, end.y) + offset * dy,
            );
            let direction = if kind & 0x40 == 0 {
                ArcDirection::Clockwise
            } else {
                ArcDirection::CounterClockwise
            };
            Edge::Arc {
                start,
                end,
                centre,
                direction,
            }
        })
        .collect()
}

/// The outer loop and holes of a raw profile, for [`geom::Profile::new`].
#[must_use]
pub fn profile_loops((outer, holes, raw_scale): &RawProfile) -> (Vec<Edge>, Vec<Vec<Edge>>) {
    let scale = scale(*raw_scale);
    let holes = holes
        .iter()
        .take(MAX_HOLES)
        .map(|hole| edge_loop(hole, scale))
        .collect();
    (edge_loop(outer, scale), holes)
}

/// An extrusion direction in quarter metres.
#[must_use]
pub fn direction((x, y, z): (i8, i8, i8)) -> Vector3 {
    Vector3::new(f64::from(x) / 4.0, f64::from(y) / 4.0, f64::from(z) / 4.0)
}

/// The loop with every edge reversed and in reverse order: the same region, turning the other
/// way.
#[must_use]
pub fn reversed(edges: &[Edge]) -> Vec<Edge> {
    edges.iter().rev().map(Edge::reversed).collect()
}

/// The total area of a solid's faces, in square metres: half the length of each face's area
/// vector, summed by Newell's method over its rings.
#[must_use]
pub fn surface_area(solid: &Solid) -> f64 {
    let points = solid.vertices();
    solid
        .to_faces()
        .iter()
        .map(|face| {
            let mut area = [0.0_f64; 3];
            for ring in std::iter::once(&face.outer).chain(&face.holes) {
                for (i, &start) in ring.iter().enumerate() {
                    let (p, q) = (points[start], points[ring[(i + 1) % ring.len()]]);
                    area[0] += (p.y - q.y) * (p.z + q.z);
                    area[1] += (p.z - q.z) * (p.x + q.x);
                    area[2] += (p.x - q.x) * (p.y + q.y);
                }
            }
            area.iter().map(|c| c * c).sum::<f64>().sqrt() / 2.0
        })
        .sum()
}

/// How far a boolean's volume may lie from the bounds its operands set: the tolerance times
/// their area. The merge pass moves each vertex by at most the tolerance, which sweeps at most
/// the tolerance times the area of the faces around it, and removes only lumps no thicker than
/// the tolerance on average (ADR 0001). Rounding of constructed vertices, about 1e-12 m within
/// the model extent, is six orders of magnitude below it.
#[must_use]
pub fn volume_slack(a: &Solid, b: &Solid) -> f64 {
    LINEAR_TOLERANCE * (surface_area(a) + surface_area(b))
}

/// The result of a boolean the fuzz targets check, or `None` if the operation refused the
/// operands with [`BooleanError::FeaturesTooClose`], the one error valid operands may get
/// (ADR 0001, merge pass).
///
/// # Panics
///
/// On any other error: the operation failed where it must not.
#[must_use]
pub fn unless_too_close(
    result: Result<Option<Solid>, BooleanError>,
    operation: &str,
) -> Option<Option<Solid>> {
    match result {
        Err(BooleanError::FeaturesTooClose) => None,
        Err(error) => panic!("{operation}: {error:?}"),
        Ok(solid) => Some(solid),
    }
}

/// The farthest the merge pass may move a vertex, in metres: across the box of a group of
/// vertices merged into one, at most [`MERGE_CHAIN_WIDTH`] wide along each axis, or the
/// operation fails with [`BooleanError::MergeChainTooWide`] (ADR 0001, merge pass step 2).
pub const MERGE_REACH: f64 = MERGE_CHAIN_WIDTH * SQRT_3;

/// √3: the diagonal of a cube over its side.
const SQRT_3: f64 = 1.732_050_807_568_877_2;

/// Whether every point lies with respect to `solid` as `allowed` says, or within
/// [`MERGE_REACH`] of its boundary, where a vertex merge may have moved the point from.
pub fn all_located(
    points: &[Point3],
    solid: &Solid,
    allowed: impl Fn(Containment) -> bool,
) -> bool {
    points
        .iter()
        .all(|&p| allowed(solid.locate(p)) || solid.boundary_distance(p) <= MERGE_REACH)
}

/// How strictly [`check_tessellation`] holds a solid to the invariants of its mesh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Contract {
    /// For solids whose faces are exactly planar and simple, as extrusions: tessellation always
    /// succeeds, and by position every directed edge has exactly one reverse.
    Exact,
    /// For boolean results, whose faces are planar and simple only within the tolerance: a face
    /// may fail if its rings come within twice [`MERGE_CHAIN_WIDTH`] of themselves away from
    /// their corners, and an edge may be used more than once where a folded face lies on
    /// another, as long as it has as many reverses as uses (ADR 0021).
    WithinTolerance,
}

/// Tessellates a valid solid and checks its mesh (GEO-05): each face gives `n + 2h − 2`
/// triangles of its own vertices, strictly counterclockwise seen from outside but for slivers
/// (see [`is_sliver`]), adding up to its area vector and carrying its unit normal; by position,
/// every directed edge has a reverse as the contract says; and the volume the triangles enclose
/// is the solid's.
///
/// # Panics
///
/// When the tessellation fails beyond what the contract allows, or any of these does not hold.
pub fn check_tessellation(solid: &Solid, contract: Contract) {
    let mesh = match solid.tessellate() {
        Ok(mesh) => mesh,
        Err(TessellationError::Untriangulable { face })
            if contract == Contract::WithinTolerance && has_close_features(solid, face) =>
        {
            return;
        }
        Err(error) => panic!("tessellation: {error:?}"),
    };
    assert_eq!(mesh.normals.len(), mesh.positions.len());
    let corner = |index: u32| {
        let p = mesh.positions[usize::try_from(index).expect("32-bit indices fit")];
        [p.x, p.y, p.z]
    };
    let mut triangles = mesh.indices.as_chunks::<3>().0.iter();
    assert!(mesh.indices.as_chunks::<3>().1.is_empty());
    let mut base = 0;
    for (face, input) in solid.to_faces().iter().enumerate() {
        let rings: Vec<&Vec<usize>> = std::iter::once(&input.outer).chain(&input.holes).collect();
        let n: usize = rings.iter().map(|ring| ring.len()).sum();
        let (mut area, mut rounding) = ([0.0; 3], 0.0);
        for ring in &rings {
            let points: Vec<[f64; 3]> = ring
                .iter()
                .map(|&v| {
                    let p = solid.vertices()[v];
                    [p.x, p.y, p.z]
                })
                .collect();
            area = add(area, newell(&points));
            rounding += newell_magnitude(&points);
        }
        let face_area = norm(area) / 2.0;
        let unit = area.map(|c| c / norm(area));
        for normal in &mesh.normals[base..base + n] {
            assert!(
                norm(sub([normal.x, normal.y, normal.z], unit)) <= 1e-12,
                "face {face}: normal"
            );
        }
        let mut covered = [0.0; 3];
        for _ in 0..n + 2 * input.holes.len() - 2 {
            let triangle = triangles.next().expect("a triangle per face corner");
            for &index in triangle {
                let index = usize::try_from(index).expect("32-bit indices fit");
                assert!(
                    (base..base + n).contains(&index),
                    "face {face}: index outside the face"
                );
            }
            let corners = triangle.map(corner);
            assert!(
                turn(area, corners) == Orientation::Positive || is_sliver(corners),
                "face {face}: a triangle wider than a sliver turns the wrong way"
            );
            let [a, b, c] = corners;
            covered = add(covered, cross(sub(b, a), sub(c, a)));
            rounding += norm(sub(b, a)) * norm(sub(c, a));
        }
        // Area vectors add up exactly, even over a face planar only within the tolerance, where
        // the triangles' areas would not. Newell's sums run over coordinates, not edges: their
        // rounding grows with the distance from the origin, large next to a sliver's area.
        let slack = 1e-9 * face_area + 1e-14 * rounding;
        assert!(norm(sub(covered, area)) <= 2.0 * slack, "face {face}: area");
        base += n;
    }
    assert!(triangles.next().is_none() && base == mesh.positions.len());
    let key = |index: u32| corner(index).map(|c| (c + 0.0).to_bits());
    let mut edges: HashMap<_, usize> = HashMap::new();
    for t in mesh.indices.as_chunks::<3>().0 {
        for (i, j) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *edges.entry((key(i), key(j))).or_default() += 1;
        }
    }
    for (&(a, b), &count) in &edges {
        let reverses = edges.get(&(b, a)).copied();
        match contract {
            Contract::Exact => assert!(count == 1 && reverses == Some(1), "an edge used twice"),
            Contract::WithinTolerance => assert_eq!(reverses, Some(count), "an open edge"),
        }
    }
    let apex = mesh.indices.first().map_or([0.0; 3], |&i| corner(i));
    let (mut six_volume, mut rounding) = (0.0, 0.0);
    for t in mesh.indices.as_chunks::<3>().0 {
        let [a, b, c] = t.map(|i| sub(corner(i), apex));
        six_volume += dot(a, cross(b, c));
        rounding += norm(a) * norm(b) * norm(c);
    }
    // Each term rounds relative to its corners' distances, not to the volume: a needle ten
    // metres long and a tenth of a millimetre wide loses digits to cancellation. And the solid's
    // volume takes each face as flat, where the mesh follows its vertices, which may be off its
    // plane by the tolerance: that sweeps at most the tolerance times the area (ADR 0001).
    let volume = solid.volume();
    let slack = 1e-9 * volume + 1e-14 * rounding + LINEAR_TOLERANCE * surface_area(solid);
    assert!(
        (six_volume / 6.0 - volume).abs() <= slack,
        "enclosed volume"
    );
}

/// Whether two edges of a face's rings that share no vertex come within twice
/// [`MERGE_CHAIN_WIDTH`] of each other: a face simple only within the tolerance, which its
/// projection on a plane may show crossing itself.
fn has_close_features(solid: &Solid, face: usize) -> bool {
    let input = &solid.to_faces()[face];
    let point = |v: usize| {
        let p = solid.vertices()[v];
        [p.x, p.y, p.z]
    };
    let edges: Vec<(usize, usize)> = std::iter::once(&input.outer)
        .chain(&input.holes)
        .flat_map(|ring| (0..ring.len()).map(|i| (ring[i], ring[(i + 1) % ring.len()])))
        .collect();
    let reach = 2.0 * MERGE_CHAIN_WIDTH;
    edges.iter().enumerate().any(|(i, &(a, b))| {
        edges[i + 1..].iter().any(|&(c, d)| {
            a != c
                && a != d
                && b != c
                && b != d
                && segment_distance(point(a), point(b), point(c), point(d)) <= reach
        })
    })
}

/// The distance between two segments `p q` and `r s` (Ericson, "Real-Time Collision
/// Detection", 5.1.9).
fn segment_distance(
    start: [f64; 3],
    end: [f64; 3],
    other_start: [f64; 3],
    other_end: [f64; 3],
) -> f64 {
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

/// Whether a triangle is no wider than twice [`MERGE_CHAIN_WIDTH`]: the only triangles the
/// tessellation may leave flat or turning the wrong way, where a face planar and simple only
/// within the tolerance crosses itself (ADR 0021). It decides them projected on the face's
/// coordinate plane, where widths shrink by up to √3; twice covers that.
fn is_sliver([a, b, c]: [[f64; 3]; 3]) -> bool {
    let longest = [sub(b, a), sub(c, b), sub(a, c)]
        .iter()
        .map(|&side| norm(side))
        .fold(0.0, f64::max);
    norm(cross(sub(b, a), sub(c, a))) <= 2.0 * MERGE_CHAIN_WIDTH * longest
}

/// How a triangle turns, exactly, seen along the largest component of its face's area vector:
/// positive when it turns the way the face does.
fn turn(area: [f64; 3], corners: [[f64; 3]; 3]) -> Orientation {
    let axis = (0..3).fold(0, |best, i| {
        if area[i].abs() > area[best].abs() {
            i
        } else {
            best
        }
    });
    let (first, second) = ((axis + 1) % 3, (axis + 2) % 3);
    let [a, b, c] = corners.map(|p| Point2::new(p[first], p[second]));
    let turn = orient2d(a, b, c).expect("coordinates of a valid solid");
    if area[axis] > 0.0 {
        turn
    } else {
        turn.reversed()
    }
}

/// The area vector of a ring by Newell's method: normal to it, twice its area long.
fn newell(ring: &[[f64; 3]]) -> [f64; 3] {
    (0..ring.len()).fold([0.0; 3], |[x, y, z], i| {
        let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
        [
            x + (p[1] - q[1]) * (p[2] + q[2]),
            y + (p[2] - q[2]) * (p[0] + q[0]),
            z + (p[0] - q[0]) * (p[1] + q[1]),
        ]
    })
}

/// What the terms of [`newell`] add up to in magnitude, which their rounding is relative to.
fn newell_magnitude(ring: &[[f64; 3]]) -> f64 {
    (0..ring.len())
        .map(|i| {
            let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
            norm(sub(p, q)) * norm(add(p, q))
        })
        .sum()
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
