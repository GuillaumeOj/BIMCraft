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

/// Whether `normal` has unit length within [`NORMAL_LENGTH_TOLERANCE`]. The length is computed
/// in `f32`, in the renderer's order, so a normal accepted here is never refused there: a length
/// within the tolerance in `f64` can round past it in `f32`.
pub(crate) fn is_unit(normal: [f32; 3]) -> bool {
    let [x, y, z] = normal;
    let length = (x * x + y * y + z * z).sqrt();
    // NaN fails the comparison. The `f64` tolerance is below its `f32` rounding, so this is at
    // least as strict as the renderer's comparison in `f32`.
    f64::from((length - 1.0).abs()) <= NORMAL_LENGTH_TOLERANCE
}

/// Whether the column-major `rotation` is orthonormal within [`ROTATION_TOLERANCE`], checked on
/// its columns and on its rows (the renderer checks the rows), and its determinant is +1: a rigid
/// placement, never a reflection.
pub(crate) fn is_proper_rotation(rotation: &[f64; 9]) -> bool {
    let column = |i: usize| [rotation[3 * i], rotation[3 * i + 1], rotation[3 * i + 2]];
    let row = |i: usize| [rotation[i], rotation[3 + i], rotation[6 + i]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    // Within the tolerance, the Gram matrices of the columns and of the rows differ by a rotation
    // of their error, so one can pass where the other does not.
    let orthonormal = |vector: &dyn Fn(usize) -> [f64; 3]| {
        (0..3).all(|i| {
            (0..=i).all(|j| {
                let expected = if i == j { 1.0 } else { 0.0 };
                (dot(vector(i), vector(j)) - expected).abs() <= ROTATION_TOLERANCE
            })
        })
    };
    // Orthonormal columns give a determinant of +1 or -1; the sign is what is left to check.
    orthonormal(&column) && orthonormal(&row) && dot(cross(column(0), column(1)), column(2)) > 0.0
}

/// Whether [`decode`](crate::decode) accepts an element placed by `rotation` and `translation`: a
/// proper rotation, and a translation within the world.
#[must_use]
pub fn placement_is_valid(rotation: &[f64; 9], translation: &[f64; 3]) -> bool {
    translation.iter().all(|&c| in_world(c)) && is_proper_rotation(rotation)
}
