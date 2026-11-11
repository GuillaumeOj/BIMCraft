//! Validity checks of paths: coordinates, continuity, degenerate segments and closure.

use core::f64::consts::TAU;

use super::{Arc3, Curve3, MAX_PATH_SEGMENTS, PathError, PathSegment};
use crate::tolerance::{WITHIN_TOLERANCE, is_model_coordinate};
use crate::vector::{Vector, add, coords, cross, dot, length, scale, sub};

/// Checks the segments in the order of the [`PathError`] variants and returns their geometry.
pub(super) fn check(segments: &[PathSegment]) -> Result<Vec<Curve3>, PathError> {
    let (Some(first), Some(last)) = (segments.first(), segments.last()) else {
        return Err(PathError::Empty);
    };
    if segments.len() > MAX_PATH_SEGMENTS {
        return Err(PathError::TooManySegments);
    }
    for (segment, s) in segments.iter().enumerate() {
        for value in s.points().iter().flat_map(|p| [p.x, p.y, p.z]) {
            if !value.is_finite() {
                return Err(PathError::NonFinite { segment });
            }
            if !is_model_coordinate(value) {
                return Err(PathError::OutOfRange { segment });
            }
        }
    }
    if let Some(segment) = segments
        .windows(2)
        .position(|pair| pair[0].end() != pair[1].start())
    {
        return Err(PathError::Gap { segment });
    }
    let curves = segments
        .iter()
        .enumerate()
        .map(|(segment, s)| curve(s, segment))
        .collect::<Result<Vec<_>, _>>()?;
    if length(sub(coords(last.end()), coords(first.start()))) <= WITHIN_TOLERANCE {
        return Err(PathError::Closed);
    }
    Ok(curves)
}

fn curve(segment: &PathSegment, index: usize) -> Result<Curve3, PathError> {
    match *segment {
        PathSegment::Line { start, end } => {
            let (start, end) = (coords(start), coords(end));
            if length(sub(end, start)) <= WITHIN_TOLERANCE {
                return Err(PathError::ZeroLengthLine { segment: index });
            }
            Ok(Curve3::Line(start, end))
        }
        PathSegment::Arc {
            start,
            through,
            end,
        } => arc(coords(start), coords(through), coords(end))
            .map(Curve3::Arc)
            .ok_or(PathError::DegenerateArc { segment: index }),
    }
}

/// The arc through three points, or `None` when two of them merge or they are collinear under
/// the tolerance policy.
///
/// Everything is computed relative to `start`, so that the coordinates' magnitude does not
/// swamp the arc's own: with u = through − start, v = end − start and w = u × v, the centre is
/// start + (|u|² (v × w) + |v|² (w × u)) / (2 |w|²). The three points turn counterclockwise
/// around w, so the arc from `start` through `through` to `end` does too.
fn arc(start: Vector, through: Vector, end: Vector) -> Option<Arc3> {
    let (u, v) = (sub(through, start), sub(end, start));
    let w = cross(u, v);
    let merged = [u, v, sub(v, u)]
        .iter()
        .any(|&d| length(d) <= WITHIN_TOLERANCE);
    // |u × v| / |v| is the distance from `through` to the line through the ends.
    if merged || length(w) <= WITHIN_TOLERANCE * length(v) {
        return None;
    }
    let ww = dot(w, w);
    let centre = scale(
        add(scale(cross(v, w), dot(u, u)), scale(cross(w, u), dot(v, v))),
        0.5 / ww,
    );
    let radius = length(centre);
    let outward = scale(centre, -1.0 / radius);
    let normal = scale(w, 1.0 / ww.sqrt());
    let to_end = sub(v, centre);
    let turned = libm::atan2(dot(cross(outward, to_end), normal), dot(outward, to_end));
    Some(Arc3 {
        start,
        end,
        radius,
        outward,
        tangent: cross(normal, outward),
        sweep: if turned > 0.0 { turned } else { turned + TAU },
    })
}
