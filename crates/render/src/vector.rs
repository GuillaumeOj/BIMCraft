//! The few `f64` vector and 3×3 matrix operations of the renderer, on plain arrays.

/// A 3×3 matrix, row-major: `m[row][column]`.
pub(crate) type Rotation = [[f64; 3]; 3];

pub(crate) fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    a.map(|v| v * s)
}

pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

/// `m · v`.
pub(crate) fn apply(m: &Rotation, v: [f64; 3]) -> [f64; 3] {
    m.map(|row| dot(row, v))
}

/// `a · b`.
pub(crate) fn mul(a: &Rotation, b: &Rotation) -> Rotation {
    core::array::from_fn(|row| {
        core::array::from_fn(|column| (0..3).map(|k| a[row][k] * b[k][column]).sum())
    })
}

/// Whether the rows of `m` are orthonormal: every entry of `m · mᵀ − I` within `tolerance`.
pub(crate) fn is_orthonormal(m: &Rotation, tolerance: f64) -> bool {
    (0..3).all(|i| {
        (0..3).all(|j| {
            let expected = if i == j { 1.0 } else { 0.0 };
            (dot(m[i], m[j]) - expected).abs() <= tolerance
        })
    })
}
