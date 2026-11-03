//! Fixtures shared by the integration tests: EPSG values, entered by hand. The unit tests have
//! their own in `src/lib.rs`, which integration tests cannot see.

use std::f64::consts::PI;

use georef::{Ellipsoid, Helmert, HelmertParameters, RotationConvention};

/// Degrees to radians.
pub const DEG: f64 = PI / 180.0;

/// GRS 1980 (EPSG:7019).
pub fn grs80() -> Ellipsoid {
    Ellipsoid::new(6_378_137.0, 298.257_222_101).unwrap()
}

/// WGS 84 (EPSG:7030).
pub fn wgs84() -> Ellipsoid {
    Ellipsoid::new(6_378_137.0, 298.257_223_563).unwrap()
}

/// OSGB36 to WGS 84 (EPSG:1314) in `convention`, as `tests/proj/generate.py` applies it.
pub fn osgb36_to_wgs84(convention: RotationConvention) -> Helmert {
    Helmert::new(&HelmertParameters {
        translation: [446.448, -125.157, 542.06],
        rotation: [0.15, 0.247, 0.842],
        scale_difference: -20.489,
        convention,
    })
    .unwrap()
}
