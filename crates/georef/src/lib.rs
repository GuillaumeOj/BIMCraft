//! Georeferencing for `BIMCraft` (spec §7): where a project lies on Earth.
//!
//! A [`Georeference`] links the local engineering frame of the model to a projected coordinate
//! reference system, named by its [`EpsgCode`] (GEOREF-01): the projected coordinates of the
//! local origin (the base point), the grid rotation, the scale and an optional survey point.
//! [`MapConversion`] converts points between the two systems with the semantics of IFC 4.3
//! `IfcMapConversion` (GEOREF-02). Projected coordinates never enter the geometry kernel
//! (ADR 0001): modelling stays in the local frame.
//!
//! Projections convert between geographic and projected coordinates (GEOREF-03): Transverse
//! Mercator ([`TransverseMercator`], UTM included) and Lambert Conformal Conic
//! ([`LambertConformalConic`]), on an [`Ellipsoid`], with a seven-parameter [`Helmert`]
//! transformation between datums. They also give the meridian convergence and the point scale
//! factor ([`Factors`]). Their parameters are plain values; the [`registry`] fills them from the
//! embedded EPSG definitions, and [`ProjectedCrs::true_north`] derives true north from the
//! grid rotation with the projection of the georeference's own CRS.
//!
//! The georeference is stored on the project's single `Site` object; the [`site`] module reads
//! and writes it there. Decisions: `docs/dev/adr/0017-georeference-storage-and-ifc-mapping.md`.
//!
//! This crate builds for native targets and `wasm32-unknown-unknown` (ARCH-01), never touches the
//! platform directly (ARCH-02) and computes trigonometry with `libm`, so results are identical
//! on every target (ENG-04).

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

mod conversion;
mod geodesy;
mod georeference;
mod helmert;
mod lcc;
mod projection;
pub mod registry;
pub mod site;
mod tmerc;

pub use conversion::{MapConversion, XAxisError, rotation_from_x_axis};
pub use geodesy::{Ellipsoid, GeocentricPoint, GeodesyError, GeographicPoint, Parameter};
pub use georeference::{
    EpsgCode, Field, Georeference, GeoreferenceError, LocalPoint, ProjectedPoint,
};
pub use helmert::{Helmert, HelmertParameters, RotationConvention};
pub use lcc::{LambertConformalConic, LambertConic1SpParameters, LambertConic2SpParameters};
pub use projection::{Factors, Projection};
pub use registry::{
    AreaOfUse, BasePointError, CrsStatus, Datum, DatumShift, GeographicCrs, NamedEllipsoid,
    ParseError, PrimeMeridian, ProjectedCrs, Registry, RegistryError,
};
pub use tmerc::{TransverseMercator, TransverseMercatorParameters};

/// Property-test settings shared by the crate. A fixed seed keeps runs reproducible; with no
/// persistence, no regression files are written next to the sources.
#[cfg(test)]
fn proptest_config() -> proptest::test_runner::Config {
    proptest::test_runner::Config {
        rng_seed: proptest::test_runner::RngSeed::Fixed(0x00C0_FFEE),
        failure_persistence: None,
        ..proptest::test_runner::Config::default()
    }
}

/// Ellipsoids and projections of the tests: EPSG values, entered by hand as fixtures.
#[cfg(test)]
mod test_support {
    use crate::{
        Ellipsoid, LambertConformalConic, LambertConic2SpParameters, TransverseMercator,
        TransverseMercatorParameters,
    };

    /// Degrees to radians.
    pub const DEG: f64 = std::f64::consts::PI / 180.0;

    /// GRS 1980 (EPSG:7019).
    pub fn grs80() -> Ellipsoid {
        Ellipsoid::new(6_378_137.0, 298.257_222_101).unwrap()
    }

    /// WGS 84 (EPSG:7030).
    pub fn wgs84() -> Ellipsoid {
        Ellipsoid::new(6_378_137.0, 298.257_223_563).unwrap()
    }

    /// RGF93 v1 / Lambert-93 (EPSG:2154).
    pub fn lambert93() -> LambertConformalConic {
        LambertConformalConic::two_standard_parallels(
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
        .unwrap()
    }

    /// UTM zone `zone`, north or south, on `ellipsoid`.
    pub fn utm(ellipsoid: &Ellipsoid, zone: u8, south: bool) -> TransverseMercator {
        TransverseMercator::new(
            ellipsoid,
            &TransverseMercatorParameters {
                latitude_of_origin: 0.0,
                longitude_of_origin: (f64::from(zone) * 6.0 - 183.0) * DEG,
                scale_factor: 0.9996,
                false_easting: 500_000.0,
                false_northing: if south { 10_000_000.0 } else { 0.0 },
            },
        )
        .unwrap()
    }
}
