//! Checks the projections and transformations against PROJ, within 1 mm (in fact 1 µm), on the grid of
//! `proj/reference.txt` (GEOREF-03): Lambert-93, CC42 to CC50, UTM on ETRS89 and WGS 84 over the
//! full zone width, Transverse Mercator far beyond a zone, geocentric conversions and a
//! seven-parameter Helmert in both conventions.
//!
//! The reference values come from PROJ through `proj/generate.py`; the file header records the
//! versions and the command. The test never calls PROJ. The CRS parameters below are the EPSG
//! definitions, entered by hand as fixtures; the EPSG registry (#82) will provide them.

// The fixtures are valid by construction and the reference file is committed; a failure is a
// test failure.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::f64::consts::PI;

use georef::{
    Ellipsoid, GeographicPoint, Helmert, HelmertParameters, LambertConformalConic,
    LambertConic2SpParameters, ProjectedPoint, Projection, RotationConvention, TransverseMercator,
    TransverseMercatorParameters,
};

const REFERENCE: &str = include_str!("proj/reference.txt");

const DEG: f64 = PI / 180.0;

/// GEOREF-03 asks for 1 mm. The implementation agrees with PROJ to 0.1 µm on this grid, so the
/// test holds it to a micrometre: a regression fails long before it matters.
const TOLERANCE: f64 = 1e-6;

/// Tolerances on PROJ's factors, which it computes by numerical differentiation: they agree to
/// about 10⁻¹¹ rad (far beyond a zone) and better than 10⁻¹⁰ in scale.
const CONVERGENCE_TOLERANCE: f64 = 1e-10;
const SCALE_TOLERANCE: f64 = 1e-10;

fn grs80() -> Ellipsoid {
    Ellipsoid::new(6_378_137.0, 298.257_222_101).unwrap()
}

fn wgs84() -> Ellipsoid {
    Ellipsoid::new(6_378_137.0, 298.257_223_563).unwrap()
}

fn airy() -> Ellipsoid {
    Ellipsoid::new(6_377_563.396, 299.324_964_6).unwrap()
}

/// A projection case: the projection and the ellipsoid it is on.
type Case = (Projection, Ellipsoid);

fn tmerc(ellipsoid: Ellipsoid, central: f64, false_northing: f64) -> Case {
    let projection = TransverseMercator::new(
        &ellipsoid,
        &TransverseMercatorParameters {
            latitude_of_origin: 0.0,
            longitude_of_origin: central * DEG,
            scale_factor: 0.9996,
            false_easting: 500_000.0,
            false_northing,
        },
    )
    .unwrap();
    (projection.into(), ellipsoid)
}

fn utm(ellipsoid: Ellipsoid, zone: u16, false_northing: f64) -> Case {
    tmerc(ellipsoid, f64::from(zone) * 6.0 - 183.0, false_northing)
}

fn lcc(origin: f64, first: f64, second: f64, false_easting: f64, false_northing: f64) -> Case {
    let projection = LambertConformalConic::two_standard_parallels(
        &grs80(),
        &LambertConic2SpParameters {
            latitude_of_origin: origin * DEG,
            longitude_of_origin: 3.0 * DEG,
            first_parallel: first * DEG,
            second_parallel: second * DEG,
            false_easting,
            false_northing,
        },
    )
    .unwrap();
    (projection.into(), grs80())
}

/// The projection of each `project` case of the reference file, with its ellipsoid, by name.
fn projections() -> BTreeMap<String, Case> {
    let mut cases = BTreeMap::new();
    cases.insert(
        "epsg:2154".to_owned(),
        lcc(46.5, 49.0, 44.0, 700_000.0, 6_600_000.0),
    );
    for zone in 42_u16..=50 {
        let origin = f64::from(zone);
        let northing = (origin - 41.0) * 1e6 + 200_000.0;
        let projection = lcc(origin, origin - 0.75, origin + 0.75, 1_700_000.0, northing);
        cases.insert(format!("epsg:{}", 3900 + zone), projection);
    }
    for zone in 28_u16..=38 {
        cases.insert(format!("epsg:{}", 25800 + zone), utm(grs80(), zone, 0.0));
    }
    for zone in [1_u16, 31, 60] {
        cases.insert(format!("epsg:{}", 32600 + zone), utm(wgs84(), zone, 0.0));
    }
    for zone in [1_u16, 33, 60] {
        cases.insert(
            format!("epsg:{}", 32700 + zone),
            utm(wgs84(), zone, 10_000_000.0),
        );
    }
    cases.insert("tmerc_wide".to_owned(), tmerc(grs80(), 3.0, 0.0));
    cases
}

/// OSGB36 to WGS 84 (EPSG:1314) in `convention`, as `generate.py` applies it.
fn osgb36_to_wgs84(convention: RotationConvention) -> Helmert {
    Helmert::new(&HelmertParameters {
        translation: [446.448, -125.157, 542.06],
        rotation: [0.15, 0.247, 0.842],
        scale_difference: -20.489,
        convention,
    })
    .unwrap()
}

/// The ground distance, in metres, between two nearby geographic points on `ellipsoid`.
fn ground_distance(ellipsoid: &Ellipsoid, a: GeographicPoint, b: GeographicPoint) -> f64 {
    let radius = ellipsoid.semi_major_axis();
    let d_longitude = libm::remainder(a.longitude - b.longitude, 2.0 * PI);
    libm::hypot(
        (a.latitude - b.latitude) * radius,
        d_longitude * libm::cos(a.latitude) * radius,
    )
}

fn geographic_degrees(longitude: f64, latitude: f64, height: f64) -> GeographicPoint {
    GeographicPoint::new(longitude * DEG, latitude * DEG, height)
}

/// One reference line: its kind, its case and its numbers.
struct Line<'a> {
    number: usize,
    kind: &'a str,
    case: &'a str,
    values: [f64; 6],
}

fn lines() -> impl Iterator<Item = Line<'static>> {
    REFERENCE
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.starts_with('#') && !line.is_empty())
        .map(|(index, line)| {
            let mut fields = line.split(' ');
            let kind = fields.next().unwrap();
            let case = fields.next().unwrap();
            let values: Vec<f64> = fields.map(|field| field.parse().unwrap()).collect();
            let values = values.try_into().unwrap();
            Line {
                number: index + 1,
                kind,
                case,
                values,
            }
        })
}

fn check_projection(line: &Line<'_>, (projection, ellipsoid): &Case) {
    let [longitude, latitude, eastings, northings, convergence, scale] = line.values;
    let at = |what: &str| format!("line {} ({} {what})", line.number, line.case);
    let point = geographic_degrees(longitude, latitude, 0.0);
    let projected = projection.forward(point).unwrap();
    let error = libm::hypot(
        projected.eastings - eastings,
        projected.northings - northings,
    );
    assert!(error < TOLERANCE, "{}: {error} m from PROJ", at("forward"));
    let back = projection
        .inverse(ProjectedPoint::new(eastings, northings, 0.0))
        .unwrap();
    let error = ground_distance(ellipsoid, back, point);
    assert!(error < TOLERANCE, "{}: {error} m from PROJ", at("inverse"));
    let factors = projection.factors(point).unwrap();
    let error = (factors.convergence - convergence * DEG).abs();
    assert!(
        error < CONVERGENCE_TOLERANCE,
        "{}: {error} rad from PROJ",
        at("convergence")
    );
    let error = (factors.scale - scale).abs();
    assert!(
        error < SCALE_TOLERANCE,
        "{}: {error} from PROJ",
        at("scale")
    );
}

fn check_geocentric(line: &Line<'_>) {
    let [longitude, latitude, height, x, y, z] = line.values;
    let ellipsoid = match line.case {
        "GRS80" => Some(grs80()),
        "WGS84" => Some(wgs84()),
        _ => None,
    }
    .unwrap();
    let point = geographic_degrees(longitude, latitude, height);
    let geocentric = ellipsoid.to_geocentric(point).unwrap();
    let error = libm::hypot(
        libm::hypot(geocentric.x - x, geocentric.y - y),
        geocentric.z - z,
    );
    assert!(
        error < TOLERANCE,
        "line {}: {error} m from PROJ",
        line.number
    );
    let back = ellipsoid
        .to_geographic(georef::GeocentricPoint::new(x, y, z))
        .unwrap();
    let error = ground_distance(&ellipsoid, back, point).max((back.height - height).abs());
    assert!(
        error < TOLERANCE,
        "line {} (inverse): {error} m from PROJ",
        line.number
    );
}

fn check_helmert(line: &Line<'_>) {
    let [
        longitude,
        latitude,
        height,
        target_longitude,
        target_latitude,
        target_height,
    ] = line.values;
    let convention = match line.case {
        "position_vector" => Some(RotationConvention::PositionVector),
        "coordinate_frame" => Some(RotationConvention::CoordinateFrame),
        _ => None,
    }
    .unwrap();
    let helmert = osgb36_to_wgs84(convention);
    let point = geographic_degrees(longitude, latitude, height);
    let expected = geographic_degrees(target_longitude, target_latitude, target_height);
    let actual = helmert
        .forward_geographic(&airy(), &wgs84(), point)
        .unwrap();
    let error =
        ground_distance(&wgs84(), actual, expected).max((actual.height - target_height).abs());
    assert!(
        error < TOLERANCE,
        "line {}: {error} m from PROJ",
        line.number
    );
    let back = helmert
        .inverse_geographic(&airy(), &wgs84(), expected)
        .unwrap();
    let error = ground_distance(&airy(), back, point).max((back.height - height).abs());
    assert!(
        error < TOLERANCE,
        "line {} (inverse): {error} m from PROJ",
        line.number
    );
}

#[test]
fn matches_proj_within_a_micrometre() {
    let projections = projections();
    let mut counts = BTreeMap::<String, usize>::new();
    for line in lines() {
        *counts
            .entry(format!("{} {}", line.kind, line.case))
            .or_default() += 1;
        match line.kind {
            "project" => check_projection(&line, &projections[line.case]),
            "geocentric" => check_geocentric(&line),
            _ => check_helmert(&line),
        }
    }
    // Every fixture is covered, so a truncated reference file fails.
    let mut expected: Vec<String> = projections
        .keys()
        .map(|case| format!("project {case}"))
        .collect();
    expected.extend(["geocentric GRS80", "geocentric WGS84"].map(str::to_owned));
    expected.extend(["helmert coordinate_frame", "helmert position_vector"].map(str::to_owned));
    expected.sort();
    assert_eq!(counts.keys().cloned().collect::<Vec<_>>(), expected);
}

#[test]
fn records_its_provenance() {
    let header: Vec<&str> = REFERENCE
        .lines()
        .take_while(|line| line.starts_with('#'))
        .collect();
    assert!(header.iter().any(|line| line.contains("generate.py")));
    assert!(
        header
            .iter()
            .any(|line| line.contains("PROJ ") && line.contains("EPSG dataset"))
    );
}
