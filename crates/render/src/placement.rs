//! Placements of meshes in the world, in `f64` (GEO-07).

/// How far the rows of a rotation may be from orthonormal, per entry of `R·Rᵀ − I`. Rotations
/// built in `f64` from unit axes are many orders of magnitude closer.
const ORTHONORMAL_TOLERANCE: f64 = 1e-9;

/// Places a mesh in the world: its coordinates are rotated, then translated, in `f64`.
///
/// A world point is `rotation · p + translation`, where `p` is a point of the mesh (its origin
/// plus a local position). The rotation may be a reflection (determinant −1), as for mirrored
/// elements.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// Orthonormal 3×3 matrix, row-major: `rotation[row][column]`.
    pub rotation: [[f64; 3]; 3],
    /// Translation, in metres.
    pub translation: [f64; 3],
}

impl Placement {
    /// The placement that leaves the mesh where it is.
    pub const IDENTITY: Self = Self::from_translation([0.0; 3]);

    /// A translation without rotation.
    #[must_use]
    pub const fn from_translation(translation: [f64; 3]) -> Self {
        Self {
            rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            translation,
        }
    }

    /// Checks that the rotation is a finite orthonormal matrix and that the translation is finite
    /// and within [`MAX_WORLD_COORDINATE`](crate::MAX_WORLD_COORDINATE).
    ///
    /// # Errors
    ///
    /// The first [`PlacementError`] found.
    pub fn validate(&self) -> Result<(), PlacementError> {
        if !self.rotation.iter().flatten().all(|v| v.is_finite()) {
            return Err(PlacementError::NonFiniteRotation);
        }
        for i in 0..3 {
            for j in 0..3 {
                let dot: f64 = (0..3)
                    .map(|k| self.rotation[i][k] * self.rotation[j][k])
                    .sum();
                let expected = if i == j { 1.0 } else { 0.0 };
                if (dot - expected).abs() > ORTHONORMAL_TOLERANCE {
                    return Err(PlacementError::NotOrthonormal);
                }
            }
        }
        if !self.translation.iter().all(|v| v.is_finite()) {
            return Err(PlacementError::NonFiniteTranslation);
        }
        if !self.translation.iter().all(|&v| crate::in_world(v)) {
            return Err(PlacementError::TranslationOutOfRange);
        }
        Ok(())
    }
}

/// Why a placement cannot be used. Messages never contain coordinates (SEC-03).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PlacementError {
    /// The rotation has a value that is not finite.
    #[error("the rotation of the placement is not finite")]
    NonFiniteRotation,
    /// The rotation is not orthonormal: it would scale or shear the mesh.
    #[error("the rotation of the placement is not orthonormal")]
    NotOrthonormal,
    /// The translation is not finite.
    #[error("the translation of the placement is not finite")]
    NonFiniteTranslation,
    /// The translation lies beyond [`MAX_WORLD_COORDINATE`](crate::MAX_WORLD_COORDINATE).
    #[error("the translation of the placement lies too far from the world origin")]
    TranslationOutOfRange,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MAX_WORLD_COORDINATE;

    #[test]
    fn identity_and_translations_are_valid() {
        assert_eq!(Placement::IDENTITY.validate(), Ok(()));
        let placement = Placement::from_translation([1e7, -1e7, MAX_WORLD_COORDINATE]);
        assert_eq!(placement.validate(), Ok(()));
        assert_eq!(placement.rotation, Placement::IDENTITY.rotation);
    }

    #[test]
    fn rotations_and_reflections_are_valid() {
        let (s, c) = (libm::sin(0.3), libm::cos(0.3));
        let rotation = Placement {
            rotation: [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
            translation: [5.0, 6.0, 7.0],
        };
        assert_eq!(rotation.validate(), Ok(()));
        let mirror = Placement {
            rotation: [[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            translation: [0.0; 3],
        };
        assert_eq!(mirror.validate(), Ok(()));
    }

    #[test]
    fn rotations_that_are_not_finite_are_rejected() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for row in 0..3 {
                for column in 0..3 {
                    let mut placement = Placement::IDENTITY;
                    placement.rotation[row][column] = bad;
                    assert_eq!(placement.validate(), Err(PlacementError::NonFiniteRotation));
                }
            }
        }
    }

    #[test]
    fn scales_and_shears_are_rejected() {
        let mut scaled = Placement::IDENTITY;
        scaled.rotation[1][1] = 2.0;
        assert_eq!(scaled.validate(), Err(PlacementError::NotOrthonormal));
        let mut sheared = Placement::IDENTITY;
        sheared.rotation[0][1] = 0.1;
        assert_eq!(sheared.validate(), Err(PlacementError::NotOrthonormal));
        let mut nearly = Placement::IDENTITY;
        nearly.rotation[2][2] = 1.0 + 1e-12;
        assert_eq!(nearly.validate(), Ok(()));
    }

    #[test]
    fn translations_that_are_not_finite_or_too_far_are_rejected() {
        for (bad, error) in [
            (f64::NAN, PlacementError::NonFiniteTranslation),
            (f64::INFINITY, PlacementError::NonFiniteTranslation),
            (
                MAX_WORLD_COORDINATE * 1.5,
                PlacementError::TranslationOutOfRange,
            ),
        ] {
            for axis in 0..3 {
                let mut translation = [0.0; 3];
                translation[axis] = bad;
                assert_eq!(
                    Placement::from_translation(translation).validate(),
                    Err(error.clone())
                );
            }
        }
    }

    #[test]
    fn messages_are_plain_text() {
        for error in [
            PlacementError::NonFiniteRotation,
            PlacementError::NotOrthonormal,
            PlacementError::NonFiniteTranslation,
            PlacementError::TranslationOutOfRange,
        ] {
            assert!(error.to_string().starts_with("the "));
        }
    }
}
