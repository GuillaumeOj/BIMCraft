//! Checks every predicate against the same determinant evaluated in exact rational arithmetic
//! (GEO-03, ENG-02), on random and on nearly degenerate input.

use geom::{
    MAX_COORDINATE, MIN_COORDINATE, Orientation, Point2, Point3, incircle, insphere, orient2d,
    orient3d,
};
use num_rational::BigRational;
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

fn exact(v: f64) -> BigRational {
    BigRational::from_float(v).unwrap_or_else(|| unreachable!("predicate inputs are finite"))
}

/// Laplace expansion along the first row, over the columns still in `cols`; borrows the entries
/// instead of building minors.
fn determinant(rows: &[Vec<BigRational>], cols: &[usize]) -> BigRational {
    let [first, rest @ ..] = rows else {
        return BigRational::from_integer(1.into());
    };
    let mut sum = BigRational::default();
    for (i, &col) in cols.iter().enumerate() {
        let others: Vec<usize> = cols.iter().copied().filter(|&c| c != col).collect();
        let term = &first[col] * determinant(rest, &others);
        sum = if i % 2 == 0 { sum + term } else { sum - term };
    }
    sum
}

fn sign_of_determinant(rows: &[Vec<BigRational>]) -> Orientation {
    let cols: Vec<usize> = (0..rows.len()).collect();
    sign(&determinant(rows, &cols))
}

fn sign(v: &BigRational) -> Orientation {
    match v.cmp(&BigRational::default()) {
        std::cmp::Ordering::Greater => Orientation::Positive,
        std::cmp::Ordering::Less => Orientation::Negative,
        std::cmp::Ordering::Equal => Orientation::Zero,
    }
}

fn exact_all(p: &[f64]) -> Vec<BigRational> {
    p.iter().copied().map(exact).collect()
}

/// Coordinates of `p` minus `origin`, exactly.
fn diff(p: &[f64], origin: &[BigRational]) -> Vec<BigRational> {
    p.iter().zip(origin).map(|(p, o)| exact(*p) - o).collect()
}

/// A row of differences followed by their squared length (the lifting used by in-circle tests).
fn lifted(p: &[f64], origin: &[BigRational]) -> Vec<BigRational> {
    let mut row = diff(p, origin);
    let norm = row.iter().map(|v| v * v).sum();
    row.push(norm);
    row
}

fn xy(p: Point2) -> [f64; 2] {
    [p.x, p.y]
}

fn xyz(p: Point3) -> [f64; 3] {
    [p.x, p.y, p.z]
}

fn orient2d_exact(pa: Point2, pb: Point2, pc: Point2) -> Orientation {
    let origin = exact_all(&xy(pc));
    sign_of_determinant(&[diff(&xy(pa), &origin), diff(&xy(pb), &origin)])
}

fn orient3d_exact(pa: Point3, pb: Point3, pc: Point3, pd: Point3) -> Orientation {
    let origin = exact_all(&xyz(pd));
    sign_of_determinant(&[
        diff(&xyz(pa), &origin),
        diff(&xyz(pb), &origin),
        diff(&xyz(pc), &origin),
    ])
}

fn incircle_exact(pa: Point2, pb: Point2, pc: Point2, pd: Point2) -> Orientation {
    let origin = exact_all(&xy(pd));
    sign_of_determinant(&[
        lifted(&xy(pa), &origin),
        lifted(&xy(pb), &origin),
        lifted(&xy(pc), &origin),
    ])
}

fn insphere_exact(pa: Point3, pb: Point3, pc: Point3, pd: Point3, pe: Point3) -> Orientation {
    let origin = exact_all(&xyz(pe));
    sign_of_determinant(&[
        lifted(&xyz(pa), &origin),
        lifted(&xyz(pb), &origin),
        lifted(&xyz(pc), &origin),
        lifted(&xyz(pd), &origin),
    ])
}

/// Moves `v` by `steps` units in the last place. Zero stays put: its neighbours are subnormal,
/// outside the supported range.
fn nudge(v: f64, steps: i32) -> f64 {
    if v == 0.0 {
        return v;
    }
    (0..steps.unsigned_abs()).fold(v, |v, _| {
        if steps > 0 {
            v.next_up()
        } else {
            v.next_down()
        }
    })
}

/// Mostly arbitrary values; small integers make exactly degenerate cases frequent.
fn coordinate() -> impl Strategy<Value = f64> {
    prop_oneof![
        (-100..100i32).prop_map(f64::from),
        -1.0..1.0,
        -1.0e3..1.0e3,
        -1.0e15..1.0e15,
    ]
}

/// Power-of-two exponents applied to a whole case. Scaling by 2^k is exact, so it keeps a case
/// degenerate or exactly as far from degenerate, while moving it towards the range bounds where
/// overflow and underflow would break exactness.
fn exponent() -> impl Strategy<Value = i32> {
    -60..=45
}

/// 2^`k`, exactly.
fn pow2(k: i32) -> f64 {
    libm::scalbn(1.0, k)
}

fn in_range(v: f64) -> bool {
    v == 0.0 || (MIN_COORDINATE..=MAX_COORDINATE).contains(&v.abs())
}

/// Scales every point by 2^`k`, rejecting the case if a coordinate leaves the supported range.
fn scale2<const N: usize>(points: [Point2; N], k: i32) -> Result<[Point2; N], TestCaseError> {
    let s = pow2(k);
    let scaled = points.map(|p| Point2::new(p.x * s, p.y * s));
    if scaled.iter().all(|p| in_range(p.x) && in_range(p.y)) {
        Ok(scaled)
    } else {
        Err(TestCaseError::reject("scaled out of range"))
    }
}

fn scale3<const N: usize>(points: [Point3; N], k: i32) -> Result<[Point3; N], TestCaseError> {
    let s = pow2(k);
    let scaled = points.map(|p| Point3::new(p.x * s, p.y * s, p.z * s));
    if scaled
        .iter()
        .all(|p| in_range(p.x) && in_range(p.y) && in_range(p.z))
    {
        Ok(scaled)
    } else {
        Err(TestCaseError::reject("scaled out of range"))
    }
}

/// Interpolation factors; quarters keep interpolated integer points exact.
fn factor() -> impl Strategy<Value = f64> {
    prop_oneof![(-8..8i32).prop_map(|k| f64::from(k) / 4.0), -2.0..2.0]
}

fn point2() -> impl Strategy<Value = Point2> {
    (coordinate(), coordinate()).prop_map(|(x, y)| Point2::new(x, y))
}

fn point3() -> impl Strategy<Value = Point3> {
    (coordinate(), coordinate(), coordinate()).prop_map(|(x, y, z)| Point3::new(x, y, z))
}

fn ulps() -> impl Strategy<Value = i32> {
    -3..=3
}

/// Integer points on the circle of radius 5 around the origin.
const CIRCLE: [(f64, f64); 12] = [
    (5.0, 0.0),
    (-5.0, 0.0),
    (0.0, 5.0),
    (0.0, -5.0),
    (3.0, 4.0),
    (-3.0, 4.0),
    (3.0, -4.0),
    (-3.0, -4.0),
    (4.0, 3.0),
    (-4.0, 3.0),
    (4.0, -3.0),
    (-4.0, -3.0),
];

/// Integer points on the sphere of radius 5 around the origin: the six on the axes and every
/// signed permutation of (3, 4, 0).
const SPHERE: [[f64; 3]; 30] = [
    [5.0, 0.0, 0.0],
    [-5.0, 0.0, 0.0],
    [0.0, 5.0, 0.0],
    [0.0, -5.0, 0.0],
    [0.0, 0.0, 5.0],
    [0.0, 0.0, -5.0],
    [3.0, 4.0, 0.0],
    [3.0, -4.0, 0.0],
    [-3.0, 4.0, 0.0],
    [-3.0, -4.0, 0.0],
    [3.0, 0.0, 4.0],
    [3.0, 0.0, -4.0],
    [-3.0, 0.0, 4.0],
    [-3.0, 0.0, -4.0],
    [4.0, 3.0, 0.0],
    [4.0, -3.0, 0.0],
    [-4.0, 3.0, 0.0],
    [-4.0, -3.0, 0.0],
    [4.0, 0.0, 3.0],
    [4.0, 0.0, -3.0],
    [-4.0, 0.0, 3.0],
    [-4.0, 0.0, -3.0],
    [0.0, 3.0, 4.0],
    [0.0, 3.0, -4.0],
    [0.0, -3.0, 4.0],
    [0.0, -3.0, -4.0],
    [0.0, 4.0, 3.0],
    [0.0, 4.0, -3.0],
    [0.0, -4.0, 3.0],
    [0.0, -4.0, -3.0],
];

/// Rounding the scaled and translated circle points makes them nearly, not exactly, cocircular.
fn on_circle(index: usize, scale: i32, offset: (f64, f64)) -> Point2 {
    let (x, y) = CIRCLE[index];
    let s = pow2(scale);
    Point2::new(x * s + offset.0, y * s + offset.1)
}

fn on_sphere(index: usize, scale: i32, offset: (f64, f64, f64)) -> Point3 {
    let [x, y, z] = SPHERE[index];
    let s = pow2(scale);
    Point3::new(x * s + offset.0, y * s + offset.1, z * s + offset.2)
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 2_000,
        rng_seed: RngSeed::Fixed(47),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn orient2d_matches_exact_on_random_points(
        points in prop::array::uniform3(point2()), k in exponent(),
    ) {
        let [a, b, c] = scale2(points, k)?;
        let expected = orient2d_exact(a, b, c);
        prop_assert_eq!(orient2d(a, b, c), Ok(expected));
        prop_assert_eq!(orient2d(b, a, c), Ok(expected.reversed()));
    }

    #[test]
    fn orient2d_matches_exact_near_a_line(
        a in point2(), b in point2(), t in factor(), dx in ulps(), dy in ulps(), k in exponent(),
    ) {
        let c = Point2::new(
            nudge(a.x + t * (b.x - a.x), dx),
            nudge(a.y + t * (b.y - a.y), dy),
        );
        let [a, b, c] = scale2([a, b, c], k)?;
        prop_assert_eq!(orient2d(a, b, c), Ok(orient2d_exact(a, b, c)));
    }

    #[test]
    fn orient3d_matches_exact_on_random_points(
        points in prop::array::uniform4(point3()), k in exponent(),
    ) {
        let [a, b, c, d] = scale3(points, k)?;
        let expected = orient3d_exact(a, b, c, d);
        prop_assert_eq!(orient3d(a, b, c, d), Ok(expected));
        prop_assert_eq!(orient3d(b, a, c, d), Ok(expected.reversed()));
    }

    #[test]
    fn orient3d_matches_exact_near_a_plane(
        a in point3(), b in point3(), c in point3(),
        s in factor(), t in factor(), dz in ulps(), k in exponent(),
    ) {
        let at = |pa: f64, pb: f64, pc: f64| pa + s * (pb - pa) + t * (pc - pa);
        let d = Point3::new(at(a.x, b.x, c.x), at(a.y, b.y, c.y), nudge(at(a.z, b.z, c.z), dz));
        let [a, b, c, d] = scale3([a, b, c, d], k)?;
        prop_assert_eq!(orient3d(a, b, c, d), Ok(orient3d_exact(a, b, c, d)));
    }

    #[test]
    fn incircle_matches_exact_on_random_points(
        points in prop::array::uniform4(point2()), k in exponent(),
    ) {
        let [a, b, c, d] = scale2(points, k)?;
        prop_assert_eq!(incircle(a, b, c, d), Ok(incircle_exact(a, b, c, d)));
    }

    #[test]
    fn incircle_matches_exact_near_a_circle(
        idx in prop::array::uniform4(0..CIRCLE.len()), scale in -20..20i32,
        offset in (coordinate(), coordinate()), dx in ulps(), k in exponent(),
    ) {
        let [a, b, c, d] = idx.map(|i| on_circle(i, scale, offset));
        let d = Point2::new(nudge(d.x, dx), d.y);
        let [a, b, c, d] = scale2([a, b, c, d], k)?;
        prop_assert_eq!(incircle(a, b, c, d), Ok(incircle_exact(a, b, c, d)));
    }

    #[test]
    fn insphere_matches_exact_on_random_points(
        points in prop::array::uniform5(point3()), k in exponent(),
    ) {
        let [a, b, c, d, e] = scale3(points, k)?;
        prop_assert_eq!(insphere(a, b, c, d, e), Ok(insphere_exact(a, b, c, d, e)));
    }

    #[test]
    fn insphere_matches_exact_near_a_sphere(
        idx in prop::array::uniform5(0..SPHERE.len()), scale in -20..20i32,
        offset in (coordinate(), coordinate(), coordinate()), dz in ulps(), k in exponent(),
    ) {
        let [a, b, c, d, e] = idx.map(|i| on_sphere(i, scale, offset));
        let e = Point3::new(e.x, e.y, nudge(e.z, dz));
        let [a, b, c, d, e] = scale3([a, b, c, d, e], k)?;
        prop_assert_eq!(insphere(a, b, c, d, e), Ok(insphere_exact(a, b, c, d, e)));
    }
}

/// Shewchuk's classic example: a 256 × 256 grid of points one ulp apart around (0.5, 0.5), tested
/// against the line through (12, 12) and (24, 24). A plain `f64` determinant gets many signs wrong.
#[test]
fn orient2d_is_exact_on_ulp_grid_where_naive_f64_fails() {
    let start = Point2::new(12.0, 12.0);
    let end = Point2::new(24.0, 24.0);
    let mut naive_errors = 0;
    let mut x = 0.5;
    for _ in 0..256 {
        let mut y = 0.5;
        for _ in 0..256 {
            let a = Point2::new(x, y);
            let expected = orient2d_exact(a, start, end);
            assert_eq!(orient2d(a, start, end), Ok(expected), "at ({x:e}, {y:e})");
            let naive = (a.x - end.x) * (start.y - end.y) - (a.y - end.y) * (start.x - end.x);
            if sign(&exact(naive)) != expected {
                naive_errors += 1;
            }
            y = y.next_up();
        }
        x = x.next_up();
    }
    assert!(
        naive_errors > 0,
        "the grid must defeat plain f64 arithmetic"
    );
}

#[test]
fn sphere_points_are_distinct_and_on_the_sphere() {
    for (i, p) in SPHERE.iter().enumerate() {
        assert_eq!(exact(p[0] * p[0] + p[1] * p[1] + p[2] * p[2]), exact(25.0));
        assert!(!SPHERE[..i].contains(p));
    }
}
