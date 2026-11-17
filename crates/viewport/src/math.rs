//! Small `f64` vector helpers.

/// A vector or a point, in metres.
pub(crate) type Vec3 = [f64; 3];

pub(crate) fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn scale(a: Vec3, s: f64) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub(crate) fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn norm(a: Vec3) -> f64 {
    libm::sqrt(dot(a, a))
}

/// `a` scaled to unit length, or `None` when it is (nearly) zero.
pub(crate) fn normalize(a: Vec3) -> Option<Vec3> {
    let length = norm(a);
    (length > 1e-300 && length.is_finite()).then(|| scale(a, 1.0 / length))
}

/// `m · v` for a row-major matrix.
pub(crate) fn apply(m: &[[f64; 3]; 3], v: Vec3) -> Vec3 {
    [dot(m[0], v), dot(m[1], v), dot(m[2], v)]
}

/// Promotes an `f32` point to `f64`.
pub(crate) fn widen(p: [f32; 3]) -> Vec3 {
    p.map(f64::from)
}

/// The distance between two screen points.
pub(crate) fn distance_2d(a: [f64; 2], b: [f64; 2]) -> f64 {
    libm::hypot(a[0] - b[0], a[1] - b[1])
}

/// A count as the number of an announcement or of a state, saturating.
pub(crate) fn count(n: usize) -> f64 {
    u32::try_from(n).map_or(f64::MAX, f64::from)
}

#[cfg(test)]
#[expect(clippy::float_cmp, reason = "small integers and halves, exact in f64")]
mod tests {
    use super::*;

    #[test]
    fn helpers_compute_what_they_say() {
        assert_eq!(add([1.0, 2.0, 3.0], [1.0; 3]), [2.0, 3.0, 4.0]);
        assert_eq!(sub([1.0, 2.0, 3.0], [1.0; 3]), [0.0, 1.0, 2.0]);
        assert_eq!(cross([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), [0.0, 0.0, 1.0]);
        assert_eq!(normalize([0.0, 0.0, 4.0]), Some([0.0, 0.0, 1.0]));
        assert_eq!(normalize([0.0; 3]), None);
        assert_eq!(normalize([f64::NAN; 3]), None);
        assert_eq!(
            apply(
                &[[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
                [1.0, 2.0, 3.0]
            ),
            [2.0, 1.0, 3.0]
        );
        assert_eq!(widen([1.5, 2.0, -1.0]), [1.5, 2.0, -1.0]);
        assert!((distance_2d([0.0, 0.0], [3.0, 4.0]) - 5.0).abs() < 1e-15);
    }
}
