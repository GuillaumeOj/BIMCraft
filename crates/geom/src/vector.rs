//! Small vector helpers on `[f64; 3]`, shared by the B-rep checks and the booleans.
//!
//! Only `+`, `-`, `*`, `/` and `sqrt` are used, all exactly rounded, so results are identical on
//! every target (ENG-04).

use crate::Point3;

pub(crate) type Vector = [f64; 3];

pub(crate) const fn coords(p: Point3) -> Vector {
    [p.x, p.y, p.z]
}

pub(crate) fn sub(a: Vector, b: Vector) -> Vector {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn add(a: Vector, b: Vector) -> Vector {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn scale(a: Vector, k: f64) -> Vector {
    [a[0] * k, a[1] * k, a[2] * k]
}

pub(crate) fn dot(a: Vector, b: Vector) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: Vector, b: Vector) -> Vector {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn length(a: Vector) -> f64 {
    dot(a, a).sqrt()
}

/// The vector scaled to length 1; `None` for the zero vector.
pub(crate) fn unit(a: Vector) -> Option<Vector> {
    let l = length(a);
    (l > 0.0).then(|| scale(a, 1.0 / l))
}

/// Orders points by x, then y, then z.
pub(crate) fn lexicographic(a: Vector, b: Vector) -> std::cmp::Ordering {
    a[0].total_cmp(&b[0])
        .then(a[1].total_cmp(&b[1]))
        .then(a[2].total_cmp(&b[2]))
}

/// The squared distance between two points.
pub(crate) fn distance_squared(a: Vector, b: Vector) -> f64 {
    let d = sub(a, b);
    dot(d, d)
}

/// The area vector of a ring by Newell's method: exact for planar rings, a best fit otherwise.
pub(crate) fn newell(ring: &[Vector]) -> Vector {
    edges(ring).fold([0.0; 3], |[x, y, z], (p, q)| {
        [
            x + (p[1] - q[1]) * (p[2] + q[2]),
            y + (p[2] - q[2]) * (p[0] + q[0]),
            z + (p[0] - q[0]) * (p[1] + q[1]),
        ]
    })
}

/// The edges of a ring as pairs of consecutive points, closing back to the first.
pub(crate) fn edges<T: Copy>(ring: &[T]) -> impl Iterator<Item = (T, T)> + '_ {
    ring.iter()
        .copied()
        .zip(ring.iter().copied().cycle().skip(1))
}

/// The smallest box holding the points, as its lowest and highest corners; `None` without points.
pub(crate) fn bounds(points: impl IntoIterator<Item = Vector>) -> Option<(Vector, Vector)> {
    let mut points = points.into_iter();
    let first = points.next()?;
    Some(points.fold((first, first), |(lo, hi), p| {
        (
            [0, 1, 2].map(|i| lo[i].min(p[i])),
            [0, 1, 2].map(|i| hi[i].max(p[i])),
        )
    }))
}
