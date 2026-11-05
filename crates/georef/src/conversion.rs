//! The map conversion between the local engineering frame and the projected CRS (GEOREF-02),
//! with the semantics of IFC 4.3 `IfcMapConversion`.

use crate::{Georeference, LocalPoint, ProjectedPoint};

/// The conversion of a [`Georeference`] between local and projected coordinates.
///
/// With θ the grid rotation, s the scale and (E₀, N₀, H₀) the base point:
///
/// - E = s·(x·cos θ − y·sin θ) + E₀
/// - N = s·(x·sin θ + y·cos θ) + N₀
/// - H = s·z + H₀
///
/// The scale applies to all three axes, as `IfcMapConversion.Scale` does: it is the unit
/// conversion between the engineering frame and the map (ADR 0017). [`MapConversion::to_local`]
/// is the exact inverse, up to rounding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapConversion {
    origin: ProjectedPoint,
    cos: f64,
    sin: f64,
    scale: f64,
}

impl MapConversion {
    /// The map conversion of `georeference`.
    #[must_use]
    pub fn new(georeference: &Georeference) -> Self {
        let (sin, cos) = libm::sincos(georeference.grid_rotation());
        Self {
            origin: georeference.base_point(),
            cos,
            sin,
            scale: georeference.scale(),
        }
    }

    /// The projected coordinates of the local point `point`.
    #[must_use]
    pub fn to_projected(&self, point: LocalPoint) -> ProjectedPoint {
        let Self {
            origin,
            cos,
            sin,
            scale,
        } = *self;
        ProjectedPoint::new(
            scale * (point.x * cos - point.y * sin) + origin.eastings,
            scale * (point.x * sin + point.y * cos) + origin.northings,
            scale * point.z + origin.height,
        )
    }

    /// The local coordinates of the projected point `point`.
    #[must_use]
    pub fn to_local(&self, point: ProjectedPoint) -> LocalPoint {
        let Self {
            origin,
            cos,
            sin,
            scale,
        } = *self;
        let eastings = point.eastings - origin.eastings;
        let northings = point.northings - origin.northings;
        LocalPoint::new(
            (eastings * cos + northings * sin) / scale,
            (northings * cos - eastings * sin) / scale,
            (point.height - origin.height) / scale,
        )
    }

    /// The direction of the local x-axis in the projected CRS, as `IfcMapConversion` writes it:
    /// `(XAxisAbscissa, XAxisOrdinate)` = (cos θ, sin θ).
    #[must_use]
    pub const fn x_axis(&self) -> (f64, f64) {
        (self.cos, self.sin)
    }
}

/// Why an x-axis direction gives no grid rotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum XAxisError {
    /// A component is NaN or infinite.
    #[error("the x-axis direction is not finite")]
    NonFinite,
    /// Both components are zero.
    #[error("the x-axis direction is zero")]
    Zero,
}

/// The grid rotation, in radians in (−π, π], of the x-axis direction (`abscissa`, `ordinate`)
/// of an `IfcMapConversion`. The direction need not be normalised.
///
/// # Errors
///
/// An [`XAxisError`] when a component is not finite or both are zero.
pub fn rotation_from_x_axis(abscissa: f64, ordinate: f64) -> Result<f64, XAxisError> {
    if !abscissa.is_finite() || !ordinate.is_finite() {
        return Err(XAxisError::NonFinite);
    }
    if abscissa == 0.0 && ordinate == 0.0 {
        return Err(XAxisError::Zero);
    }
    Ok(libm::atan2(ordinate, abscissa))
}

impl Georeference {
    /// The local coordinates of the survey point, when there is one.
    #[must_use]
    pub fn survey_point_local(&self) -> Option<LocalPoint> {
        self.survey_point()
            .map(|point| MapConversion::new(self).to_local(point))
    }
}

#[cfg(test)]
mod tests {
    use core::f64::consts::{FRAC_PI_2, FRAC_PI_6, PI};

    use proptest::prelude::*;

    use super::*;
    use crate::EpsgCode;

    fn georef(base: ProjectedPoint, rotation: f64, scale: f64) -> Georeference {
        Georeference::new(EpsgCode::new(2154).unwrap(), base, rotation, scale, None).unwrap()
    }

    fn close(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    fn assert_projected(actual: ProjectedPoint, expected: ProjectedPoint) {
        assert!(
            close(actual.eastings, expected.eastings, 1e-9)
                && close(actual.northings, expected.northings, 1e-9)
                && close(actual.height, expected.height, 1e-9),
            "{actual:?} != {expected:?}"
        );
    }

    fn assert_local(actual: LocalPoint, expected: LocalPoint) {
        assert!(
            close(actual.x, expected.x, 1e-9)
                && close(actual.y, expected.y, 1e-9)
                && close(actual.z, expected.z, 1e-9),
            "{actual:?} != {expected:?}"
        );
    }

    const BASE: ProjectedPoint = ProjectedPoint::new(652_000.0, 6_862_000.0, 35.0);

    #[test]
    fn without_rotation_or_scale_it_translates() {
        let conversion = MapConversion::new(&georef(BASE, 0.0, 1.0));
        let projected = conversion.to_projected(LocalPoint::new(10.0, 20.0, 3.0));
        assert_eq!(projected, ProjectedPoint::new(652_010.0, 6_862_020.0, 38.0));
        assert_eq!(
            conversion.to_local(projected),
            LocalPoint::new(10.0, 20.0, 3.0)
        );
        assert_eq!(conversion.x_axis(), (1.0, 0.0));
    }

    #[test]
    fn a_quarter_turn_maps_local_x_to_grid_north() {
        let conversion = MapConversion::new(&georef(BASE, FRAC_PI_2, 1.0));
        assert_projected(
            conversion.to_projected(LocalPoint::new(1.0, 0.0, 0.0)),
            ProjectedPoint::new(652_000.0, 6_862_001.0, 35.0),
        );
        assert_projected(
            conversion.to_projected(LocalPoint::new(0.0, 1.0, 0.0)),
            ProjectedPoint::new(651_999.0, 6_862_000.0, 35.0),
        );
    }

    #[test]
    fn rotates_and_scales_by_hand_computed_values() {
        // θ = 30°: cos = √3/2, sin = 1/2; s = 2. Local (2, 4, 5):
        // E = 2·(2·√3/2 − 4·1/2) + E₀ = 2√3 − 4 + E₀
        // N = 2·(2·1/2 + 4·√3/2) + N₀ = 2 + 4√3 + N₀
        // H = 2·5 + H₀
        let sqrt3 = libm::sqrt(3.0);
        let conversion = MapConversion::new(&georef(BASE, FRAC_PI_6, 2.0));
        let local = LocalPoint::new(2.0, 4.0, 5.0);
        let expected = ProjectedPoint::new(
            652_000.0 + 2.0 * sqrt3 - 4.0,
            6_862_000.0 + 2.0 + 4.0 * sqrt3,
            45.0,
        );
        assert_projected(conversion.to_projected(local), expected);
        assert_local(conversion.to_local(expected), local);
        let (abscissa, ordinate) = conversion.x_axis();
        assert!(close(abscissa, sqrt3 / 2.0, 1e-15) && close(ordinate, 0.5, 1e-15));
    }

    #[test]
    fn the_scale_applies_to_heights() {
        let conversion = MapConversion::new(&georef(BASE, 0.0, 0.001));
        let projected = conversion.to_projected(LocalPoint::new(0.0, 0.0, 1000.0));
        assert_projected(projected, ProjectedPoint::new(652_000.0, 6_862_000.0, 36.0));
    }

    #[test]
    fn rotation_from_x_axis_covers_every_quadrant() {
        let cases = [
            ((1.0, 0.0), 0.0),
            ((0.0, 2.0), FRAC_PI_2),
            ((-3.0, 0.0), PI),
            ((0.0, -1.0), -FRAC_PI_2),
            ((1.0, 1.0), PI / 4.0),
            ((-1.0, -1.0), -3.0 * PI / 4.0),
        ];
        for ((abscissa, ordinate), expected) in cases {
            let rotation = rotation_from_x_axis(abscissa, ordinate).unwrap();
            assert!(close(rotation, expected, 1e-15), "{abscissa}, {ordinate}");
        }
    }

    #[test]
    fn rotation_from_x_axis_rejects_bad_directions() {
        assert_eq!(rotation_from_x_axis(0.0, 0.0), Err(XAxisError::Zero));
        assert_eq!(rotation_from_x_axis(-0.0, 0.0), Err(XAxisError::Zero));
        for bad in [f64::NAN, f64::INFINITY] {
            assert_eq!(rotation_from_x_axis(bad, 1.0), Err(XAxisError::NonFinite));
            assert_eq!(rotation_from_x_axis(1.0, bad), Err(XAxisError::NonFinite));
        }
        assert_eq!(
            XAxisError::NonFinite.to_string(),
            "the x-axis direction is not finite"
        );
        assert_eq!(XAxisError::Zero.to_string(), "the x-axis direction is zero");
    }

    #[test]
    fn the_x_axis_gives_back_the_rotation() {
        for rotation in [0.0, 0.3, -2.0, 3.0] {
            let (abscissa, ordinate) = MapConversion::new(&georef(BASE, rotation, 1.0)).x_axis();
            let back = rotation_from_x_axis(abscissa, ordinate).unwrap();
            assert!(close(back, rotation, 4.0 * f64::EPSILON), "{rotation}");
        }
    }

    #[test]
    fn locates_the_survey_point_in_the_local_frame() {
        let survey = ProjectedPoint::new(652_010.0, 6_862_020.0, 36.0);
        let with = Georeference::new(
            EpsgCode::new(2154).unwrap(),
            BASE,
            FRAC_PI_2,
            1.0,
            Some(survey),
        )
        .unwrap();
        assert_local(
            with.survey_point_local().unwrap(),
            LocalPoint::new(20.0, -10.0, 1.0),
        );
        assert_eq!(georef(BASE, 0.0, 1.0).survey_point_local(), None);
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        /// Local → projected → local gives the point back, within rounding, over the model
        /// extent and projected coordinates up to 10⁷ m.
        #[test]
        fn local_to_projected_and_back(
            eastings in -1e7..1e7_f64,
            northings in -1e7..1e7_f64,
            height in -1e4..1e4_f64,
            rotation in -PI..PI,
            scale in 0.5..2.0_f64,
            x in -1e4..1e4_f64,
            y in -1e4..1e4_f64,
            z in -1e3..1e3_f64,
        ) {
            let base = ProjectedPoint::new(eastings, northings, height);
            let conversion = MapConversion::new(&georef(base, rotation, scale));
            let local = LocalPoint::new(x, y, z);
            let projected = conversion.to_projected(local);
            prop_assert!(projected.eastings.is_finite() && projected.northings.is_finite());
            let back = conversion.to_local(projected);
            // Rounding of the projected coordinates, ~1e7 · 2⁻⁵², divided by the scale.
            let tolerance = 1e-8;
            prop_assert!(close(back.x, x, tolerance), "{back:?}");
            prop_assert!(close(back.y, y, tolerance), "{back:?}");
            prop_assert!(close(back.z, z, tolerance), "{back:?}");
        }
    }
}
