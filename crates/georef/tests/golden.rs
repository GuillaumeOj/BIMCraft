//! Computes a fixed set of projection and transformation results and compares them bit for bit
//! with `golden.txt` (ENG-04). The same test runs natively and under `wasm32-wasip1`, so any
//! target that rounds differently fails with the first value that diverges.
//!
//! After a deliberate change (a `libm` upgrade, a new case), regenerate the file natively with
//! `BIMCRAFT_BLESS=1 cargo test -p georef --test golden` and review the diff.

// Blessing writes the golden file; tests may opt out of the ARCH-02 lints (docs/dev/quality.md).
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]
// The fixtures are valid by construction; a failure is a test failure.
#![allow(clippy::unwrap_used)]

use std::f64::consts::PI;
use std::fmt::Write as _;

use georef::{
    Ellipsoid, GeocentricPoint, GeodesyError, GeographicPoint, Helmert, HelmertParameters,
    LambertConformalConic, LambertConic1SpParameters, LambertConic2SpParameters, ProjectedPoint,
    Projection, RotationConvention, TransverseMercator, TransverseMercatorParameters,
};

const GOLDEN: &str = include_str!("golden.txt");
const GOLDEN_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden.txt");

/// Random cases per operation.
const SAMPLES: usize = 32;

const DEG: f64 = PI / 180.0;

/// `SplitMix64`, seeded per case from its name, as in `crates/geom/tests/golden.rs`: adding or
/// reordering cases leaves the inputs, and the golden lines, of every other case unchanged.
struct Rng(u64);

impl Rng {
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

    /// A geographic point within the given ranges, in degrees, and up to 5 km high.
    fn geographic(&mut self, longitudes: (f64, f64), latitudes: (f64, f64)) -> GeographicPoint {
        GeographicPoint::new(
            self.uniform(longitudes.0, longitudes.1) * DEG,
            self.uniform(latitudes.0, latitudes.1) * DEG,
            self.uniform(-100.0, 5000.0),
        )
    }
}

/// One line per result: `<case>[<index>] <bits as 16 hex digits>…`, or the error.
#[derive(Default)]
struct Output(String);

impl Output {
    fn values(&mut self, case: &str, index: usize, values: Result<[f64; 3], GeodesyError>) {
        let _ = write!(self.0, "{case}[{index}]");
        match values {
            Ok(values) => {
                for value in values {
                    let _ = write!(self.0, " {:016x}", value.to_bits());
                }
            }
            Err(error) => {
                let _ = write!(self.0, " error: {error}");
            }
        }
        self.0.push('\n');
    }
}

fn projected(point: Result<ProjectedPoint, GeodesyError>) -> Result<[f64; 3], GeodesyError> {
    point.map(|p| [p.eastings, p.northings, p.height])
}

fn geographic(point: Result<GeographicPoint, GeodesyError>) -> Result<[f64; 3], GeodesyError> {
    point.map(|p| [p.longitude, p.latitude, p.height])
}

fn grs80() -> Ellipsoid {
    Ellipsoid::new(6_378_137.0, 298.257_222_101).unwrap()
}

fn wgs84() -> Ellipsoid {
    Ellipsoid::new(6_378_137.0, 298.257_223_563).unwrap()
}

/// A projection case: its name, the projection, and the longitude and latitude ranges its random
/// points are drawn from, in degrees.
type Case = (&'static str, Projection, (f64, f64), (f64, f64));

fn projections() -> Vec<Case> {
    let utm31 = TransverseMercator::new(
        &grs80(),
        &TransverseMercatorParameters {
            latitude_of_origin: 0.0,
            longitude_of_origin: 3.0 * DEG,
            scale_factor: 0.9996,
            false_easting: 500_000.0,
            false_northing: 0.0,
        },
    )
    .unwrap();
    let utm33s = TransverseMercator::new(
        &wgs84(),
        &TransverseMercatorParameters {
            latitude_of_origin: 0.0,
            longitude_of_origin: 15.0 * DEG,
            scale_factor: 0.9996,
            false_easting: 500_000.0,
            false_northing: 10_000_000.0,
        },
    )
    .unwrap();
    let lambert93 = LambertConformalConic::two_standard_parallels(
        &grs80(),
        &LambertConic2SpParameters {
            latitude_of_origin: 46.5 * DEG,
            longitude_of_origin: 3.0 * DEG,
            first_parallel: 49.0 * DEG,
            second_parallel: 44.0 * DEG,
            false_easting: 700_000.0,
            false_northing: 6_600_000.0,
        },
    )
    .unwrap();
    let southern = LambertConformalConic::one_standard_parallel(
        &grs80(),
        &LambertConic1SpParameters {
            latitude_of_origin: -30.0 * DEG,
            longitude_of_origin: 135.0 * DEG,
            scale_factor: 0.9999,
            false_easting: 1_000_000.0,
            false_northing: 5_000_000.0,
        },
    )
    .unwrap();
    vec![
        ("utm31", utm31.into(), (-40.0, 46.0), (-85.0, 85.0)),
        ("utm33s", utm33s.into(), (11.5, 18.5), (-80.0, 0.0)),
        ("lambert93", lambert93.into(), (-10.0, 16.0), (40.0, 52.0)),
        ("lcc_south", southern.into(), (110.0, 160.0), (-45.0, -10.0)),
    ]
}

fn project(out: &mut Output) {
    for (name, projection, longitudes, latitudes) in projections() {
        let mut rng = Rng::for_case(name);
        for index in 0..SAMPLES {
            let point = rng.geographic(longitudes, latitudes);
            let forward = projection.forward(point);
            out.values(&format!("{name}.forward"), index, projected(forward));
            let inverse = forward.and_then(|p| projection.inverse(p));
            out.values(&format!("{name}.inverse"), index, geographic(inverse));
            let factors = projection
                .factors(point)
                .map(|f| [f.convergence, f.scale, 0.0]);
            out.values(&format!("{name}.factors"), index, factors);
        }
    }
}

fn geocentric(out: &mut Output) {
    let mut rng = Rng::for_case("geocentric");
    let ellipsoid = grs80();
    for index in 0..SAMPLES {
        let point = rng.geographic((-180.0, 180.0), (-90.0, 90.0));
        let cartesian = ellipsoid.to_geocentric(point);
        out.values(
            "geocentric.forward",
            index,
            cartesian.map(|c| [c.x, c.y, c.z]),
        );
        let back = cartesian.and_then(|c| ellipsoid.to_geographic(c));
        out.values("geocentric.inverse", index, geographic(back));
    }
}

fn helmert(out: &mut Output) {
    for (name, convention) in [
        (
            "helmert.position_vector",
            RotationConvention::PositionVector,
        ),
        (
            "helmert.coordinate_frame",
            RotationConvention::CoordinateFrame,
        ),
    ] {
        let helmert = Helmert::new(&HelmertParameters {
            translation: [446.448, -125.157, 542.06],
            rotation: [0.15, 0.247, 0.842],
            scale_difference: -20.489,
            convention,
        })
        .unwrap();
        let mut rng = Rng::for_case(name);
        for index in 0..SAMPLES {
            let point = GeocentricPoint::new(
                rng.uniform(-7e6, 7e6),
                rng.uniform(-7e6, 7e6),
                rng.uniform(-7e6, 7e6),
            );
            let forward = helmert.forward(point);
            out.values(
                &format!("{name}.forward"),
                index,
                Ok([forward.x, forward.y, forward.z]),
            );
            let back = helmert.inverse(forward);
            out.values(
                &format!("{name}.inverse"),
                index,
                Ok([back.x, back.y, back.z]),
            );
        }
    }
}

fn compute() -> String {
    let mut out = Output::default();
    project(&mut out);
    geocentric(&mut out);
    helmert(&mut out);
    out.0
}

/// The first line where `actual` departs from `expected`.
fn first_divergence(expected: &str, actual: &str) -> Option<String> {
    let (expected, actual): (Vec<_>, Vec<_>) =
        (expected.lines().collect(), actual.lines().collect());
    let index =
        (0..expected.len().max(actual.len())).find(|&i| expected.get(i) != actual.get(i))?;
    let side =
        |line: Option<&&str>| line.map_or_else(|| "<missing>".to_owned(), |l| (*l).to_owned());
    Some(format!(
        "line {}: expected {}, got {}",
        index + 1,
        side(expected.get(index)),
        side(actual.get(index))
    ))
}

#[test]
fn projection_results_match_golden_output() {
    let actual = compute();
    if std::env::var_os("BIMCRAFT_BLESS").is_some() {
        std::fs::write(GOLDEN_PATH, &actual).unwrap();
        return;
    }
    if let Some(divergence) = first_divergence(GOLDEN, &actual) {
        panic!(
            "projection results differ from golden.txt on this target (ENG-04), first at {divergence}. \
             If the change is deliberate, regenerate with BIMCRAFT_BLESS=1 (see this file's docs)."
        );
    }
}

#[test]
fn divergence_reports_the_first_differing_line() {
    let expected = "a[0] 3ff0000000000000\na[1] 0000000000000000\n";
    assert_eq!(first_divergence(expected, expected), None);
    assert_eq!(
        first_divergence(expected, "a[0] 3ff0000000000000\na[1] 8000000000000000\n").as_deref(),
        Some("line 2: expected a[1] 0000000000000000, got a[1] 8000000000000000")
    );
    assert_eq!(
        first_divergence(expected, "a[0] 3ff0000000000000\n").as_deref(),
        Some("line 2: expected a[1] 0000000000000000, got <missing>")
    );
}
