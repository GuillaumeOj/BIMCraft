//! Computes a fixed set of geometry results and compares them bit for bit with `golden.txt`
//! (ENG-04). The same test runs natively and under `wasm32-wasip1`, so any target that rounds
//! differently fails with the first value that diverges.
//!
//! After a deliberate change (a `libm` upgrade, a new case), regenerate the file natively with
//! `BIMCRAFT_BLESS=1 cargo test -p geom --test golden` and review the diff.

// Blessing writes the golden file; tests may opt out of the ARCH-02 lints (docs/dev/quality.md).
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::f64::consts::PI;
use std::fmt::Write as _;

use geom::{
    ArcDirection, Edge, Orientation, Point2, Point3, Profile, Solid, Vector3, incircle, insphere,
    orient2d, orient3d,
};

const GOLDEN: &str = include_str!("golden.txt");
const GOLDEN_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden.txt");

/// Random cases per function, on top of the edge cases.
const SAMPLES: usize = 32;

/// Half the side of the cube random points are drawn from, in metres.
const EXTENT: f64 = 1e3;

/// `SplitMix64`: tiny, seeded and identical on every target, unlike a library generator whose
/// algorithm may change between versions. `core-model`'s `SeededIdSource` uses the same one, but
/// only for IDs and behind its `test-support` feature.
struct Rng(u64);

impl Rng {
    /// A stream of its own per case, seeded from the case name, so adding or reordering cases
    /// leaves the inputs, and the golden lines, of every other case unchanged.
    fn for_case(name: &str) -> Self {
        // FNV-1a: a fixed hash, unlike std's `DefaultHasher`, whose algorithm is unspecified.
        let hash = name.bytes().fold(0xCBF2_9CE4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01B3)
        });
        Self(hash)
    }

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

    fn coordinate(&mut self) -> f64 {
        self.uniform(-EXTENT, EXTENT)
    }

    fn point2(&mut self) -> Point2 {
        Point2::new(self.coordinate(), self.coordinate())
    }

    fn point3(&mut self) -> Point3 {
        Point3::new(self.coordinate(), self.coordinate(), self.coordinate())
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
        let token = if v.is_nan() {
            "nan".to_owned()
        } else {
            format!("{:016x}", v.to_bits())
        };
        let _ = writeln!(self.0, "{case}[{index}] {token}");
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

fn unary(out: &mut Output) {
    for (name, f, lo, hi) in UNARY {
        let mut rng = Rng::for_case(name);
        let inputs = EDGES
            .into_iter()
            .chain((0..SAMPLES).map(|_| rng.uniform(lo, hi)));
        for (i, x) in inputs.enumerate() {
            out.value(name, i, f(x));
        }
    }
}

fn binary(out: &mut Output) {
    for (name, f, lo, hi) in BINARY {
        let mut rng = Rng::for_case(name);
        let edges = EDGES.into_iter().zip(EDGES.into_iter().rev());
        let random = (0..SAMPLES).map(|_| (rng.uniform(lo, hi), rng.uniform(lo, hi)));
        for (i, (x, y)) in edges.chain(random).enumerate() {
            out.value(name, i, f(x, y));
        }
    }
    let mut rng = Rng::for_case("mul_add");
    for i in 0..SAMPLES {
        let (a, b, c) = (rng.coordinate(), rng.coordinate(), rng.coordinate());
        out.value("mul_add", i, a.mul_add(b, c));
    }
}

/// Compositions the kernel will use: rotations, distances and unit vectors.
fn compositions(out: &mut Output) {
    let mut rng = Rng::for_case("rotate");
    for i in 0..SAMPLES {
        let point = rng.point2();
        let angle = rng.uniform(-2.0 * PI, 2.0 * PI);
        let (sin, cos) = (libm::sin(angle), libm::cos(angle));
        out.value("rotate.x", i, point.x * cos - point.y * sin);
        out.value("rotate.y", i, point.x * sin + point.y * cos);
    }

    let mut rng = Rng::for_case("distance");
    for i in 0..SAMPLES {
        let (from, to) = (rng.point2(), rng.point2());
        out.value("distance", i, libm::hypot(to.x - from.x, to.y - from.y));
    }

    let mut rng = Rng::for_case("unit");
    for i in 0..SAMPLES {
        let vector = rng.point3();
        let length = (vector.x * vector.x + vector.y * vector.y + vector.z * vector.z).sqrt();
        out.value("unit.x", i, vector.x / length);
        out.value("unit.y", i, vector.y / length);
        out.value("unit.z", i, vector.z / length);
    }
}

/// Nearly degenerate input, so the predicates take their exact stage. The orientation points
/// use only `+ - *`; the circle and sphere points are built with `libm`, so those signs also
/// depend on it being identical everywhere.
fn predicates(out: &mut Output) {
    let mut rng = Rng::for_case("orient2d");
    for i in 0..SAMPLES {
        let line = [rng.point2(), rng.point2()];
        let t = rng.uniform(-2.0, 2.0);
        let on_line = Point2::new(
            line[0].x + t * (line[1].x - line[0].x),
            line[0].y + t * (line[1].y - line[0].y),
        );
        out.sign("orient2d", i, orient2d(line[0], line[1], on_line));
    }

    let mut rng = Rng::for_case("orient3d");
    for i in 0..SAMPLES {
        let [p0, p1, p2] = [rng.point3(), rng.point3(), rng.point3()];
        let (u, v) = (rng.uniform(-2.0, 2.0), rng.uniform(-2.0, 2.0));
        let lerp = |origin: f64, first: f64, second: f64| {
            origin + u * (first - origin) + v * (second - origin)
        };
        let on_plane = Point3::new(
            lerp(p0.x, p1.x, p2.x),
            lerp(p0.y, p1.y, p2.y),
            lerp(p0.z, p1.z, p2.z),
        );
        out.sign("orient3d", i, orient3d(p0, p1, p2, on_plane));
    }

    let mut rng = Rng::for_case("incircle");
    for i in 0..SAMPLES {
        let (centre, radius) = (rng.point2(), rng.uniform(1e-3, EXTENT));
        let [p0, p1, p2, p3]: [Point2; 4] = std::array::from_fn(|_| {
            let angle = rng.uniform(0.0, 2.0 * PI);
            Point2::new(
                centre.x + radius * libm::cos(angle),
                centre.y + radius * libm::sin(angle),
            )
        });
        out.sign("incircle", i, incircle(p0, p1, p2, p3));
    }

    let mut rng = Rng::for_case("insphere");
    for i in 0..SAMPLES {
        let (centre, radius) = (rng.point3(), rng.uniform(1e-3, EXTENT));
        let [p0, p1, p2, p3, p4]: [Point3; 5] = std::array::from_fn(|_| {
            let theta = rng.uniform(0.0, 2.0 * PI);
            let phi = libm::acos(rng.uniform(-1.0, 1.0));
            Point3::new(
                centre.x + radius * libm::sin(phi) * libm::cos(theta),
                centre.y + radius * libm::sin(phi) * libm::sin(theta),
                centre.z + radius * libm::cos(phi),
            )
        });
        out.sign("insphere", i, insphere(p0, p1, p2, p3, p4));
    }
}

/// Extrusions of random circles: the chords' points come from `libm`, so the vertices and the
/// volume depend on it being identical everywhere.
fn extrusions(out: &mut Output) {
    let mut rng = Rng::for_case("extrude");
    for i in 0..SAMPLES {
        let centre = rng.point2();
        let start = Point2::new(centre.x + rng.uniform(1e-2, 10.0), centre.y);
        let circle = Edge::Arc {
            start,
            end: start,
            centre,
            direction: ArcDirection::CounterClockwise,
        };
        let direction = Vector3::new(rng.uniform(-1.0, 1.0), rng.uniform(-1.0, 1.0), 1.0);
        let solid = Profile::new(vec![circle], vec![])
            .ok()
            .and_then(|profile| Solid::extrude(&profile, direction).ok());
        let Some(solid) = solid else {
            let _ = writeln!(out.0, "extrude[{i}] error");
            continue;
        };
        let middle = solid.vertices()[solid.vertices().len() / 4];
        out.value("extrude.x", i, middle.x);
        out.value("extrude.y", i, middle.y);
        out.value("extrude.volume", i, solid.volume());
    }
}

fn compute() -> String {
    let mut out = Output::default();
    unary(&mut out);
    binary(&mut out);
    compositions(&mut out);
    predicates(&mut out);
    extrusions(&mut out);
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
    let (expected, actual): (Vec<_>, Vec<_>) =
        (expected.lines().collect(), actual.lines().collect());
    let index =
        (0..expected.len().max(actual.len())).find(|&i| expected.get(i) != actual.get(i))?;
    let side = |line: Option<&&str>| match line {
        None => "<missing>".to_owned(),
        Some(line) => {
            let (case, token) = line.split_once(' ').unwrap_or((line, ""));
            format!("{case} {}", decode(token))
        }
    };
    Some(format!(
        "line {}: expected {}, got {}",
        index + 1,
        side(expected.get(index)),
        side(actual.get(index))
    ))
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
