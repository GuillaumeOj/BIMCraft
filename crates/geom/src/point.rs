//! Points of the model space.

/// A point in the plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point2 {
    /// Abscissa.
    pub x: f64,
    /// Ordinate.
    pub y: f64,
}

impl Point2 {
    /// Creates a point from its coordinates.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// A point in space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point3 {
    /// First coordinate.
    pub x: f64,
    /// Second coordinate.
    pub y: f64,
    /// Third coordinate.
    pub z: f64,
}

impl Point3 {
    /// Creates a point from its coordinates.
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}

/// A displacement in space, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vector3 {
    /// First component.
    pub x: f64,
    /// Second component.
    pub y: f64,
    /// Third component.
    pub z: f64,
}

impl Vector3 {
    /// Creates a vector from its components.
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}
