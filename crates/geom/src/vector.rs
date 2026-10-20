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

/// The axis along which a vector is largest, the first one on a tie.
pub(crate) fn largest_axis(v: Vector) -> usize {
    (0..3).fold(0, |best, axis| {
        if v[axis].abs() > v[best].abs() {
            axis
        } else {
            best
        }
    })
}

/// The centre of points: their average.
pub(crate) fn centroid<'a>(points: impl Iterator<Item = &'a Vector>) -> Vector {
    let (sum, count) = points.fold(([0.0; 3], 0_u32), |(sum, count), &p| {
        (add(sum, p), count + 1)
    });
    scale(sum, 1.0 / f64::from(count))
}

/// The squared distance from a point to a segment.
pub(crate) fn segment_distance_squared(point: Vector, start: Vector, end: Vector) -> f64 {
    let along = sub(end, start);
    let fraction = (dot(sub(point, start), along) / dot(along, along)).clamp(0.0, 1.0);
    let off = sub(point, add(start, scale(along, fraction)));
    dot(off, off)
}

/// The signed solid angle of a triangle seen from the origin (Van Oosterom and Strackee, 1983).
pub(crate) fn solid_angle(a: Vector, b: Vector, c: Vector) -> f64 {
    let (la, lb, lc) = (length(a), length(b), length(c));
    let numerator = dot(a, cross(b, c));
    let denominator = la * lb * lc + dot(a, b) * lc + dot(a, c) * lb + dot(b, c) * la;
    2.0 * libm::atan2(numerator, denominator)
}

/// The signed solid angle a ring subtends at `p`: the sum over the fan of triangles from its
/// first point, which adds up to the ring's whatever its shape.
pub(crate) fn fan_solid_angle(p: Vector, ring: impl IntoIterator<Item = Vector>) -> f64 {
    let mut points = ring.into_iter().map(|q| sub(q, p));
    let Some(first) = points.next() else {
        return 0.0;
    };
    let mut previous = points.next();
    let mut total = 0.0;
    for q in points {
        if let Some(b) = previous {
            total += solid_angle(first, b, q);
        }
        previous = Some(q);
    }
    total
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ring_subtends_its_fan_and_nothing_without_points() {
        // The unit square seen from just above its centre covers half the sphere, 2π.
        let square = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let angle = fan_solid_angle([0.5, 0.5, -1e-9], square);
        assert!((angle - 2.0 * core::f64::consts::PI).abs() < 1e-6);
        assert!(fan_solid_angle([0.0; 3], []).abs() < f64::EPSILON);
    }
}
