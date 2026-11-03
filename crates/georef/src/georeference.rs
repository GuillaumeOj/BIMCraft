//! The georeference of a project: its projected CRS, base point, grid rotation, scale and
//! optional survey point (GEOREF-01).

/// A point in the projected CRS: eastings, northings and orthogonal height, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectedPoint {
    /// Easting, in metres.
    pub eastings: f64,
    /// Northing, in metres.
    pub northings: f64,
    /// Orthogonal height, in metres.
    pub height: f64,
}

impl ProjectedPoint {
    /// The point at `eastings`, `northings` and `height`.
    #[must_use]
    pub const fn new(eastings: f64, northings: f64, height: f64) -> Self {
        Self {
            eastings,
            northings,
            height,
        }
    }

    fn is_finite(self) -> bool {
        self.eastings.is_finite() && self.northings.is_finite() && self.height.is_finite()
    }
}

/// A point in the local engineering frame of the model, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalPoint {
    /// Local x, in metres.
    pub x: f64,
    /// Local y, in metres.
    pub y: f64,
    /// Local z, in metres.
    pub z: f64,
}

impl LocalPoint {
    /// The point at `x`, `y` and `z`.
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}

/// The EPSG code of a coordinate reference system, such as 2154 for Lambert-93.
///
/// EPSG codes are positive 32-bit integers, so a code is between 1 and [`EpsgCode::MAX`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EpsgCode(u32);

impl EpsgCode {
    /// The largest EPSG code: codes are stored as positive 32-bit signed integers.
    pub const MAX: u32 = i32::MAX.unsigned_abs();

    /// The code `code`.
    ///
    /// # Errors
    ///
    /// [`GeoreferenceError::InvalidCrsCode`] when `code` is 0 or above [`EpsgCode::MAX`].
    pub const fn new(code: u32) -> Result<Self, GeoreferenceError> {
        if code == 0 || code > Self::MAX {
            return Err(GeoreferenceError::InvalidCrsCode);
        }
        Ok(Self(code))
    }

    /// The code as a number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A value of a [`Georeference`], named in errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    /// The base point.
    BasePoint,
    /// The grid rotation.
    GridRotation,
    /// The scale.
    Scale,
    /// The survey point.
    SurveyPoint,
}

impl std::fmt::Display for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::BasePoint => "base point",
            Self::GridRotation => "grid rotation",
            Self::Scale => "scale",
            Self::SurveyPoint => "survey point",
        })
    }
}

/// Why a georeference is invalid. Messages never contain the values (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GeoreferenceError {
    /// A number is NaN or infinite.
    #[error("the {field} is not a finite number")]
    NonFinite {
        /// The value holding the number.
        field: Field,
    },
    /// The scale is zero or negative.
    #[error("the scale must be greater than zero")]
    InvalidScale,
    /// The EPSG code is zero, negative or too large.
    #[error("the EPSG code is out of range")]
    InvalidCrsCode,
}

/// Where a project lies on Earth: the link between its local engineering frame and a projected
/// CRS, following IFC 4.3 `IfcMapConversion` and `IfcProjectedCRS` (GEOREF-01).
///
/// Every value is validated on construction, so a `Georeference` is always usable: its numbers
/// are finite and its scale is positive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Georeference {
    crs: EpsgCode,
    base_point: ProjectedPoint,
    grid_rotation: f64,
    scale: f64,
    survey_point: Option<ProjectedPoint>,
}

impl Georeference {
    /// A georeference on the CRS `crs`.
    ///
    /// - `base_point`: the projected coordinates of the local origin.
    /// - `grid_rotation`: the angle, in radians, from the CRS grid easting axis to the local
    ///   x-axis, counter-clockwise.
    /// - `scale`: the map conversion scale, from local metres to map metres.
    /// - `survey_point`: an optional known point (a benchmark), in projected coordinates.
    ///
    /// # Errors
    ///
    /// [`GeoreferenceError::NonFinite`] when a number is NaN or infinite, and
    /// [`GeoreferenceError::InvalidScale`] when `scale` is not greater than zero.
    pub fn new(
        crs: EpsgCode,
        base_point: ProjectedPoint,
        grid_rotation: f64,
        scale: f64,
        survey_point: Option<ProjectedPoint>,
    ) -> Result<Self, GeoreferenceError> {
        let check = |finite: bool, field| {
            if finite {
                Ok(())
            } else {
                Err(GeoreferenceError::NonFinite { field })
            }
        };
        check(base_point.is_finite(), Field::BasePoint)?;
        check(grid_rotation.is_finite(), Field::GridRotation)?;
        check(scale.is_finite(), Field::Scale)?;
        check(
            survey_point.is_none_or(ProjectedPoint::is_finite),
            Field::SurveyPoint,
        )?;
        if scale <= 0.0 {
            return Err(GeoreferenceError::InvalidScale);
        }
        Ok(Self {
            crs,
            base_point,
            grid_rotation,
            scale,
            survey_point,
        })
    }

    /// The projected CRS.
    #[must_use]
    pub const fn crs(&self) -> EpsgCode {
        self.crs
    }

    /// The projected coordinates of the local origin.
    #[must_use]
    pub const fn base_point(&self) -> ProjectedPoint {
        self.base_point
    }

    /// The angle, in radians, from the CRS grid easting axis to the local x-axis,
    /// counter-clockwise.
    #[must_use]
    pub const fn grid_rotation(&self) -> f64 {
        self.grid_rotation
    }

    /// The map conversion scale, always greater than zero.
    #[must_use]
    pub const fn scale(&self) -> f64 {
        self.scale
    }

    /// The survey point, in projected coordinates.
    #[must_use]
    pub const fn survey_point(&self) -> Option<ProjectedPoint> {
        self.survey_point
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lambert93() -> EpsgCode {
        EpsgCode::new(2154).unwrap()
    }

    const BASE: ProjectedPoint = ProjectedPoint::new(652_000.0, 6_862_000.0, 35.0);

    #[test]
    fn keeps_valid_values() {
        let survey = ProjectedPoint::new(652_010.0, 6_862_020.0, 36.5);
        let georef = Georeference::new(lambert93(), BASE, 0.5, 0.9996, Some(survey)).unwrap();
        assert_eq!(georef.crs().get(), 2154);
        assert_eq!(georef.base_point(), BASE);
        assert_eq!(georef.grid_rotation().to_bits(), 0.5_f64.to_bits());
        assert_eq!(georef.scale().to_bits(), 0.9996_f64.to_bits());
        assert_eq!(georef.survey_point(), Some(survey));
    }

    #[test]
    fn rejects_codes_out_of_range() {
        assert_eq!(EpsgCode::new(0), Err(GeoreferenceError::InvalidCrsCode));
        assert_eq!(
            EpsgCode::new(EpsgCode::MAX + 1),
            Err(GeoreferenceError::InvalidCrsCode)
        );
        assert_eq!(EpsgCode::new(EpsgCode::MAX).unwrap().get(), EpsgCode::MAX);
        assert_eq!(EpsgCode::new(1).unwrap().get(), 1);
    }

    #[test]
    fn rejects_non_finite_numbers() {
        let bad = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY];
        for value in bad {
            let cases = [
                (
                    Georeference::new(
                        lambert93(),
                        ProjectedPoint::new(value, 0.0, 0.0),
                        0.0,
                        1.0,
                        None,
                    ),
                    Field::BasePoint,
                ),
                (
                    Georeference::new(
                        lambert93(),
                        ProjectedPoint::new(0.0, value, 0.0),
                        0.0,
                        1.0,
                        None,
                    ),
                    Field::BasePoint,
                ),
                (
                    Georeference::new(
                        lambert93(),
                        ProjectedPoint::new(0.0, 0.0, value),
                        0.0,
                        1.0,
                        None,
                    ),
                    Field::BasePoint,
                ),
                (
                    Georeference::new(lambert93(), BASE, value, 1.0, None),
                    Field::GridRotation,
                ),
                (
                    Georeference::new(lambert93(), BASE, 0.0, value, None),
                    Field::Scale,
                ),
                (
                    Georeference::new(
                        lambert93(),
                        BASE,
                        0.0,
                        1.0,
                        Some(ProjectedPoint::new(0.0, value, 0.0)),
                    ),
                    Field::SurveyPoint,
                ),
            ];
            for (result, field) in cases {
                assert_eq!(result, Err(GeoreferenceError::NonFinite { field }));
            }
        }
    }

    #[test]
    fn rejects_a_scale_that_is_not_positive() {
        for scale in [0.0, -0.0, -1.0] {
            assert_eq!(
                Georeference::new(lambert93(), BASE, 0.0, scale, None),
                Err(GeoreferenceError::InvalidScale)
            );
        }
    }

    #[test]
    fn errors_name_the_field_but_not_the_value() {
        let messages = [
            GeoreferenceError::NonFinite {
                field: Field::BasePoint,
            }
            .to_string(),
            GeoreferenceError::NonFinite {
                field: Field::GridRotation,
            }
            .to_string(),
            GeoreferenceError::NonFinite {
                field: Field::Scale,
            }
            .to_string(),
            GeoreferenceError::NonFinite {
                field: Field::SurveyPoint,
            }
            .to_string(),
            GeoreferenceError::InvalidScale.to_string(),
            GeoreferenceError::InvalidCrsCode.to_string(),
        ];
        assert_eq!(
            messages,
            [
                "the base point is not a finite number",
                "the grid rotation is not a finite number",
                "the scale is not a finite number",
                "the survey point is not a finite number",
                "the scale must be greater than zero",
                "the EPSG code is out of range",
            ]
        );
    }
}
