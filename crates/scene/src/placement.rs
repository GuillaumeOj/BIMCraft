//! The rigid placement of an element's local frame in the world (D3 of the 3D viewport feature).

/// A rigid transform: where an element's local frame lies in the world frame.
///
/// Generators return solids in the element's local frame; the placement carries them to the
/// world, so elements with identical local geometry share one mesh and a move changes only the
/// placement (GEO-05). Values are `f64`, metres and radians, z up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// Rotation matrix, column-major.
    pub rotation: [f64; 9],
    /// Translation, in metres.
    pub translation: [f64; 3],
}

impl Placement {
    /// The local frame is the world frame.
    pub const IDENTITY: Self = Self {
        rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        translation: [0.0; 3],
    };

    /// A rotation of `angle` radians counter-clockwise about the vertical axis, then a
    /// translation by `translation`.
    #[must_use]
    pub fn about_z(angle: f64, translation: [f64; 3]) -> Self {
        let (sin, cos) = (libm::sin(angle), libm::cos(angle));
        Self {
            rotation: [cos, sin, 0.0, -sin, cos, 0.0, 0.0, 0.0, 1.0],
            translation,
        }
    }

    /// The world position of the local point `point`.
    #[must_use]
    pub fn apply(&self, point: [f64; 3]) -> [f64; 3] {
        let r = &self.rotation;
        let t = &self.translation;
        [
            r[0] * point[0] + r[3] * point[1] + r[6] * point[2] + t[0],
            r[1] * point[0] + r[4] * point[1] + r[7] * point[2] + t[1],
            r[2] * point[0] + r[5] * point[1] + r[8] * point[2] + t[2],
        ]
    }
}

impl Default for Placement {
    fn default() -> Self {
        Self::IDENTITY
    }
}
