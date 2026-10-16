//! Generators shared by the geometry kernel's fuzz targets: they turn raw tuples, which
//! libFuzzer's `Arbitrary` builds and mutates, into profile loops and extrusion directions.
//!
//! Points lie on a quarter-metre grid within ±32 m, so vertices, edges and loops of random input
//! coincide, touch and overlap often: the degenerate cases a tolerance policy must decide. A
//! scale byte moves the whole profile to other magnitudes: near the linear tolerance, near the
//! model extent, or beyond it, and one value makes a coordinate NaN.

use geom::{ArcDirection, Edge, Point2, Vector3};

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
