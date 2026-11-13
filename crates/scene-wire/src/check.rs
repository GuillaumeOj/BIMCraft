//! The value ranges a delta must respect.

use crate::{MAX_WORLD_COORDINATE, NORMAL_LENGTH_TOLERANCE, ROTATION_TOLERANCE};

/// Whether `value` is finite and within [`MAX_WORLD_COORDINATE`] (NaN fails the comparison).
pub(crate) fn in_world(value: f64) -> bool {
    value.abs() <= MAX_WORLD_COORDINATE
}

/// Whether every component of `point` is a world coordinate.
pub(crate) fn point_in_world(point: [f32; 3]) -> bool {
    point.iter().all(|&c| in_world(f64::from(c)))
}

/// Whether `normal` has unit length within [`NORMAL_LENGTH_TOLERANCE`].
pub(crate) fn is_unit(normal: [f32; 3]) -> bool {
    let [x, y, z] = normal.map(f64::from);
    let length = (x * x + y * y + z * z).sqrt();
    (length - 1.0).abs() <= NORMAL_LENGTH_TOLERANCE
}

/// Whether the three columns of the column-major `rotation` are orthonormal within
/// [`ROTATION_TOLERANCE`]; a reflection passes.
pub(crate) fn is_orthonormal(rotation: &[f64; 9]) -> bool {
    let column = |i: usize| [rotation[3 * i], rotation[3 * i + 1], rotation[3 * i + 2]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    (0..3).all(|i| {
        (0..=i).all(|j| {
            let expected = if i == j { 1.0 } else { 0.0 };
            (dot(column(i), column(j)) - expected).abs() <= ROTATION_TOLERANCE
        })
    })
}
