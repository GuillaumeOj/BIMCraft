//! Computes a fixed set of geometry results and compares them bit for bit with `golden.txt`
//! (ENG-04). The same test runs natively and under `wasm32-wasip1`, so any target that rounds
//! differently fails with the first value that diverges.
//!
//! After a deliberate change (a `libm` upgrade, a new case), regenerate the file natively with
//! `BIMCRAFT_BLESS=1 cargo test -p geom-golden` and review the diff.

// Blessing writes the golden file; tests may opt out of the ARCH-02 lints (docs/dev/quality.md).
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::f64::consts::PI;
use std::fmt::Write as _;

use geom::{Orientation, Point2, Point3, incircle, insphere, orient2d, orient3d};

const GOLDEN: &str = include_str!("../golden.txt");
const GOLDEN_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/golden.txt");

/// Random cases per function, on top of the edge cases.
const SAMPLES: usize = 32;

/// `SplitMix64`: tiny, seeded and identical on every target, unlike a library generator whose
/// algorithm may change between versions.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[lo, hi)`, built from the mantissa bits so no integer-to-float cast is involved.
    fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        let unit = f64::from_bits(0x3FF0_0000_0000_0000 | (self.next_u64() >> 12)) - 1.0;
        lo + (hi - lo) * unit
    }

    fn point2(&mut self, half_extent: f64) -> Point2 {
        Point2::new(
            self.uniform(-half_extent, half_extent),
            self.uniform(-half_extent, half_extent),
        )
    }

    fn point3(&mut self, half_extent: f64) -> Point3 {
        Point3::new(
            self.uniform(-half_extent, half_extent),
            self.uniform(-half_extent, half_extent),
            self.uniform(-half_extent, half_extent),
        )
    }
}

/// Inputs that exercise special paths: signed zeros, subnormals, huge values, multiples of π.
const EDGES: [f64; 14] = [
    0.0,
    -0.0,
    f64::MIN_POSITIVE,
    5e-324,
    1e-300,
    0.5,
    -1.0,
    1.0,
    PI / 2.0,
    PI,
    2.0 * PI,
    1e15,
    1e300,
    f64::INFINITY,
];

/// One line per result: `<case> <bits as 16 hex digits>`, or `nan` since NaN bits are unspecified
/// (ADR 0003).
#[derive(Default)]
struct Output(String);

impl Output {
    fn value(&mut self, case: &str, index: usize, v: f64) {
        if v.is_nan() {
            let _ = writeln!(self.0, "{case}[{index}] nan");
        } else {
            let _ = writeln!(self.0, "{case}[{index}] {:016x}", v.to_bits());
        }
    }

    fn sign(&mut self, case: &str, index: usize, sign: Result<Orientation, geom::PredicateError>) {
        let token = match sign {
            Ok(Orientation::Positive) => "positive",
            Ok(Orientation::Negative) => "negative",
            Ok(Orientation::Zero) => "zero",
            Err(_) => "error",
        };
        let _ = writeln!(self.0, "{case}[{index}] {token}");
    }
}

type Unary = (&'static str, fn(f64) -> f64, f64, f64);
type Binary = (&'static str, fn(f64, f64) -> f64, f64, f64);

/// The functions and the range their random arguments are drawn from.
const UNARY: [Unary; 12] = [
    ("sin", libm::sin, -1e3, 1e3),
    ("cos", libm::cos, -1e3, 1e3),
    ("tan", libm::tan, -10.0, 10.0),
    ("asin", libm::asin, -1.0, 1.0),
    ("acos", libm::acos, -1.0, 1.0),
    ("atan", libm::atan, -1e3, 1e3),
    ("exp", libm::exp, -700.0, 700.0),
    ("log", libm::log, 0.0, 1e6),
    ("log10", libm::log10, 0.0, 1e6),
    ("cbrt", libm::cbrt, -1e6, 1e6),
    ("sqrt", f64::sqrt, 0.0, 1e6),
    ("floor", f64::floor, -1e6, 1e6),
];

const BINARY: [Binary; 3] = [
    ("atan2", libm::atan2, -1e3, 1e3),
    ("pow", libm::pow, 0.0, 20.0),
    ("hypot", libm::hypot, -1e3, 1e3),
];

fn unary(out: &mut Output, rng: &mut Rng) {
    for (name, f, lo, hi) in UNARY {
        let inputs = EDGES
            .into_iter()
            .chain((0..SAMPLES).map(|_| rng.uniform(lo, hi)));
        for (i, x) in inputs.enumerate() {
            out.value(name, i, f(x));
        }
    }
}

fn binary(out: &mut Output, rng: &mut Rng) {
    for (name, f, lo, hi) in BINARY {
        let edges = EDGES.into_iter().zip(EDGES.into_iter().rev());
        let random = (0..SAMPLES).map(|_| (rng.uniform(lo, hi), rng.uniform(lo, hi)));
        for (i, (x, y)) in edges.chain(random).enumerate() {
            out.value(name, i, f(x, y));
        }
    }
    for i in 0..SAMPLES {
        let (a, b, c) = (
            rng.uniform(-1e3, 1e3),
            rng.uniform(-1e3, 1e3),
            rng.uniform(-1e3, 1e3),
        );
        out.value("mul_add", i, a.mul_add(b, c));
    }
}

/// Compositions the kernel will use: rotations, distances and unit vectors.
fn compositions(out: &mut Output, rng: &mut Rng) {
    for i in 0..SAMPLES {
        let point = rng.point2(1e3);
        let angle = rng.uniform(-2.0 * PI, 2.0 * PI);
        let (sin, cos) = (libm::sin(angle), libm::cos(angle));
        out.value("rotate.x", i, point.x * cos - point.y * sin);
        out.value("rotate.y", i, point.x * sin + point.y * cos);

        let other = rng.point2(1e3);
        out.value(
            "distance",
            i,
            libm::hypot(other.x - point.x, other.y - point.y),
        );

        let vector = rng.point3(1e3);
        let length = (vector.x * vector.x + vector.y * vector.y + vector.z * vector.z).sqrt();
        out.value("unit.x", i, vector.x / length);
        out.value("unit.y", i, vector.y / length);
        out.value("unit.z", i, vector.z / length);
    }
}

/// Nearly degenerate input, so the predicates take their exact stage; their points are built
/// with `libm`, so the signs also depend on it being identical everywhere.
fn predicates(out: &mut Output, rng: &mut Rng) {
    for i in 0..SAMPLES {
        let line = [rng.point2(1e3), rng.point2(1e3)];
        let t = rng.uniform(-2.0, 2.0);
        let on_line = Point2::new(
            line[0].x + t * (line[1].x - line[0].x),
            line[0].y + t * (line[1].y - line[0].y),
        );
        out.sign("orient2d", i, orient2d(line[0], line[1], on_line));

        let plane = [rng.point3(1e3), rng.point3(1e3), rng.point3(1e3)];
        let (u, v) = (rng.uniform(-2.0, 2.0), rng.uniform(-2.0, 2.0));
        let along = |axis: fn(Point3) -> f64| {
            let origin = axis(plane[0]);
            origin + u * (axis(plane[1]) - origin) + v * (axis(plane[2]) - origin)
        };
        let on_plane = Point3::new(along(|p| p.x), along(|p| p.y), along(|p| p.z));
        out.sign(
            "orient3d",
            i,
            orient3d(plane[0], plane[1], plane[2], on_plane),
        );

        let centre = rng.point2(1e3);
        let radius = rng.uniform(1e-3, 1e3);
        let circle: [Point2; 4] = std::array::from_fn(|_| {
            let angle = rng.uniform(0.0, 2.0 * PI);
            Point2::new(
                centre.x + radius * libm::cos(angle),
                centre.y + radius * libm::sin(angle),
            )
        });
        out.sign(
            "incircle",
            i,
            incircle(circle[0], circle[1], circle[2], circle[3]),
        );

        let centre = rng.point3(1e3);
        let sphere: [Point3; 5] = std::array::from_fn(|_| {
            let theta = rng.uniform(0.0, 2.0 * PI);
            let phi = libm::acos(rng.uniform(-1.0, 1.0));
            Point3::new(
                centre.x + radius * libm::sin(phi) * libm::cos(theta),
                centre.y + radius * libm::sin(phi) * libm::sin(theta),
                centre.z + radius * libm::cos(phi),
            )
        });
        let [first, second, third, fourth, fifth] = sphere;
        out.sign("insphere", i, insphere(first, second, third, fourth, fifth));
    }
}

fn compute() -> String {
    let mut out = Output::default();
    let mut rng = Rng(0x0BAD_5EED_2026_0010);
    unary(&mut out, &mut rng);
    binary(&mut out, &mut rng);
    compositions(&mut out, &mut rng);
    predicates(&mut out, &mut rng);
    out.0
}

/// The value a token stands for, so a failure shows numbers and not only bits.
fn decode(token: &str) -> String {
    u64::from_str_radix(token, 16).map_or_else(
        |_| token.to_owned(),
        |bits| format!("{token} = {:e}", f64::from_bits(bits)),
    )
}

/// The first line where `actual` departs from `expected`, as a readable message.
fn first_divergence(expected: &str, actual: &str) -> Option<String> {
    let (mut expected, mut actual) = (expected.lines(), actual.lines());
    for line in 1.. {
        match (expected.next(), actual.next()) {
            (None, None) => return None,
            (Some(e), Some(a)) if e == a => {}
            (e, a) => {
                let side = |l: Option<&str>| {
                    l.map_or_else(
                        || "<missing>".to_owned(),
                        |l| {
                            let (case, token) = l.split_once(' ').unwrap_or((l, ""));
                            format!("{case} {}", decode(token))
                        },
                    )
                };
                return Some(format!(
                    "line {line}: expected {}, got {}",
                    side(e),
                    side(a)
                ));
            }
        }
    }
    None
}

#[test]
fn geometry_results_match_golden_output() {
    let actual = compute();
    if std::env::var_os("BIMCRAFT_BLESS").is_some() {
        std::fs::write(GOLDEN_PATH, &actual).unwrap();
        return;
    }
    if let Some(divergence) = first_divergence(GOLDEN, &actual) {
        panic!(
            "geometry results differ from golden.txt on this target (ENG-04), first at {divergence}. \
             If the change is deliberate, regenerate with BIMCRAFT_BLESS=1 (see this file's docs)."
        );
    }
}

#[test]
fn divergence_reports_the_first_differing_value() {
    let expected = "sin[0] 3ff0000000000000\nsin[1] 0000000000000000\n";
    assert_eq!(first_divergence(expected, expected), None);
    assert_eq!(
        first_divergence(
            expected,
            "sin[0] 3ff0000000000000\nsin[1] 8000000000000000\n"
        )
        .as_deref(),
        Some("line 2: expected sin[1] 0000000000000000 = 0e0, got sin[1] 8000000000000000 = -0e0")
    );
    assert_eq!(
        first_divergence(expected, "sin[0] 3ff0000000000000\n").as_deref(),
        Some("line 2: expected sin[1] 0000000000000000 = 0e0, got <missing>")
    );
}
