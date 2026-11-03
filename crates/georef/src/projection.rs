//! Map projections between geographic and projected coordinates (GEOREF-03), and the true north
//! of a georeference derived from its grid rotation (spec D2).

use std::f64::consts::PI;

use crate::geodesy::{GeodesyError, GeographicPoint};
use crate::lcc::LambertConformalConic;
use crate::tmerc::TransverseMercator;
use crate::{Georeference, ProjectedPoint};

/// The local properties of a conformal projection at a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Factors {
    /// The meridian convergence γ, in radians: the angle from true north to grid north,
    /// clockwise, as PROJ's `meridian_convergence`. Equivalently, true north is γ
    /// counter-clockwise from grid north.
    pub convergence: f64,
    /// The point scale factor k: grid distance over ellipsoidal distance.
    pub scale: f64,
}

/// A map projection of the MVP set: Transverse Mercator (UTM included) or Lambert Conformal
/// Conic.
#[derive(Clone, Debug, PartialEq)]
pub enum Projection {
    /// Transverse Mercator.
    TransverseMercator(TransverseMercator),
    /// Lambert Conformal Conic, with one or two standard parallels.
    LambertConformalConic(LambertConformalConic),
}

impl Projection {
    /// The projected coordinates of `point`; the height passes through unchanged.
    ///
    /// # Errors
    ///
    /// [`GeodesyError`] for an invalid point or one outside the projection's domain.
    pub fn forward(&self, point: GeographicPoint) -> Result<ProjectedPoint, GeodesyError> {
        match self {
            Self::TransverseMercator(projection) => projection.forward(point),
            Self::LambertConformalConic(projection) => projection.forward(point),
        }
    }

    /// The geographic coordinates of `point`; the height passes through unchanged.
    ///
    /// # Errors
    ///
    /// [`GeodesyError`] for an invalid point or one outside the projection's domain.
    pub fn inverse(&self, point: ProjectedPoint) -> Result<GeographicPoint, GeodesyError> {
        match self {
            Self::TransverseMercator(projection) => projection.inverse(point),
            Self::LambertConformalConic(projection) => projection.inverse(point),
        }
    }

    /// The meridian convergence and point scale factor at `point`.
    ///
    /// # Errors
    ///
    /// [`GeodesyError`] for an invalid point or one outside the projection's domain.
    pub fn factors(&self, point: GeographicPoint) -> Result<Factors, GeodesyError> {
        match self {
            Self::TransverseMercator(projection) => projection.factors(point),
            Self::LambertConformalConic(projection) => projection.factors(point),
        }
    }

    /// The direction of true north at the base point of `georeference`, in radians,
    /// counter-clockwise from the local y-axis, in [−π, π]: γ − θ, with γ the meridian
    /// convergence there and θ the grid rotation (spec D2).
    ///
    /// `georeference` must be on the CRS this projection belongs to.
    ///
    /// # Errors
    ///
    /// [`GeodesyError`] when the base point is outside the projection's domain.
    pub fn true_north(&self, georeference: &Georeference) -> Result<f64, GeodesyError> {
        let base = self.inverse(georeference.base_point())?;
        let convergence = self.factors(base)?.convergence;
        Ok(libm::remainder(
            convergence - georeference.grid_rotation(),
            2.0 * PI,
        ))
    }
}

impl From<TransverseMercator> for Projection {
    fn from(projection: TransverseMercator) -> Self {
        Self::TransverseMercator(projection)
    }
}

impl From<LambertConformalConic> for Projection {
    fn from(projection: LambertConformalConic) -> Self {
        Self::LambertConformalConic(projection)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::test_support::{DEG, grs80, lambert93, utm};
    use crate::{Ellipsoid, EpsgCode};

    /// The convergence and scale of `projection` at `point`, from central differences of its
    /// forward projection along the meridian: independent of the analytic formulas.
    fn numerical_factors(
        projection: &Projection,
        ellipsoid: &Ellipsoid,
        point: GeographicPoint,
    ) -> Factors {
        let step = 1e-6;
        let at = |latitude: f64| {
            projection
                .forward(GeographicPoint::new(point.longitude, latitude, 0.0))
                .unwrap()
        };
        let (north, south) = (at(point.latitude + step), at(point.latitude - step));
        let d_east = (north.eastings - south.eastings) / (2.0 * step);
        let d_north = (north.northings - south.northings) / (2.0 * step);
        let w = ellipsoid.w(libm::sin(point.latitude));
        let meridian_radius =
            ellipsoid.semi_major_axis() * (1.0 - ellipsoid.eccentricity_squared()) / (w * w * w);
        Factors {
            convergence: libm::atan2(-d_east, d_north),
            scale: libm::hypot(d_east, d_north) / meridian_radius,
        }
    }

    fn assert_factors(projection: &Projection, point: GeographicPoint) {
        let analytic = projection.factors(point).unwrap();
        let numerical = numerical_factors(projection, &grs80(), point);
        assert!(
            (analytic.convergence - numerical.convergence).abs() < 1e-9,
            "{analytic:?} {numerical:?} at {point:?}"
        );
        assert!(
            (analytic.scale - numerical.scale).abs() < 1e-9,
            "{analytic:?} {numerical:?} at {point:?}"
        );
    }

    #[test]
    fn transverse_mercator_factors_match_the_projection() {
        let projection = Projection::from(utm(&grs80(), 31, false));
        for (longitude, latitude) in [
            (3.0, 0.0),
            (6.5, 45.0),
            (-0.5, 70.0),
            (-27.0, -60.0),
            (40.0, 10.0),
            (3.0, -84.0),
        ] {
            assert_factors(
                &projection,
                GeographicPoint::new(longitude * DEG, latitude * DEG, 0.0),
            );
        }
        let origin = projection
            .factors(GeographicPoint::new(3.0 * DEG, 0.0, 0.0))
            .unwrap();
        assert_eq!(origin.convergence.to_bits(), 0.0_f64.to_bits());
        assert!((origin.scale - 0.9996).abs() < 1e-15);
    }

    #[test]
    fn lambert_factors_match_the_projection() {
        let projection = Projection::from(lambert93());
        for (longitude, latitude) in [
            (3.0, 46.5),
            (-5.0, 48.0),
            (9.5, 42.0),
            (30.0, 70.0),
            (-40.0, -20.0),
        ] {
            assert_factors(
                &projection,
                GeographicPoint::new(longitude * DEG, latitude * DEG, 0.0),
            );
        }
    }

    #[test]
    fn dispatches_to_the_projection() {
        let point = GeographicPoint::new(2.35 * DEG, 48.85 * DEG, 35.0);
        let lcc = lambert93();
        let tm = utm(&grs80(), 31, false);
        let projections = [Projection::from(lcc), Projection::from(tm.clone())];
        assert_eq!(projections[0].forward(point), lcc.forward(point));
        assert_eq!(projections[1].forward(point), tm.forward(point));
        for projection in &projections {
            let projected = projection.forward(point).unwrap();
            let back = projection.inverse(projected).unwrap();
            assert!((back.latitude - point.latitude).abs() < 1e-12);
            assert_eq!(
                projection.inverse(ProjectedPoint::new(f64::NAN, 0.0, 0.0)),
                Err(GeodesyError::NonFinite)
            );
        }
    }

    fn georeference(base: ProjectedPoint, grid_rotation: f64) -> Georeference {
        Georeference::new(EpsgCode::new(2154).unwrap(), base, grid_rotation, 1.0, None).unwrap()
    }

    #[test]
    fn derives_true_north_from_the_grid_rotation() {
        let projection = Projection::from(lambert93());
        // East of the central meridian, grid north is clockwise of true north, so true north is
        // counter-clockwise of the local y-axis when the grid is not rotated.
        let strasbourg = projection
            .forward(GeographicPoint::new(7.75 * DEG, 48.58 * DEG, 0.0))
            .unwrap();
        let convergence = projection
            .factors(GeographicPoint::new(7.75 * DEG, 48.58 * DEG, 0.0))
            .unwrap()
            .convergence;
        assert!((convergence - 0.725_607_765_053_267 * 4.75 * DEG).abs() < 1e-12);
        let unrotated = projection
            .true_north(&georeference(strasbourg, 0.0))
            .unwrap();
        assert!((unrotated - convergence).abs() < 1e-12);
        let rotated = projection
            .true_north(&georeference(strasbourg, 30.0 * DEG))
            .unwrap();
        assert!((rotated - (convergence - 30.0 * DEG)).abs() < 1e-12);
        let wrapped = projection
            .true_north(&georeference(strasbourg, -179.0 * DEG))
            .unwrap();
        assert!((wrapped - (convergence + 179.0 * DEG - 360.0 * DEG)).abs() < 1e-12);
        let tm = Projection::from(utm(&grs80(), 31, false));
        assert_eq!(
            tm.true_north(&georeference(ProjectedPoint::new(3e7, 0.0, 0.0), 0.0)),
            Err(GeodesyError::OutsideDomain)
        );
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        #[test]
        fn factors_match_the_projection(longitude in -10.0..16.0_f64, latitude in -80.0..80.0_f64) {
            let point = GeographicPoint::new(longitude * DEG, latitude * DEG, 0.0);
            assert_factors(&Projection::from(utm(&grs80(), 31, false)), point);
        }
    }
}
