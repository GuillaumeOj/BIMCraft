//! Location points of profile-based elements: columns, beams (STR-14).

use super::LocationError;
use crate::tolerance::is_model_coordinate;
use crate::{Point2, Profile};

/// A point of a profile's section an element is placed by, also called its cardinal point.
///
/// The nine bounding-box points are numbered as in IFC: rows bottom, middle, top, and in each
/// row left, centre, right.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CardinalPoint {
    /// IFC 1.
    BottomLeft,
    /// IFC 2.
    BottomCentre,
    /// IFC 3.
    BottomRight,
    /// IFC 4.
    MiddleLeft,
    /// IFC 5.
    MiddleCentre,
    /// IFC 6.
    MiddleRight,
    /// IFC 7.
    TopLeft,
    /// IFC 8.
    TopCentre,
    /// IFC 9.
    TopRight,
    /// The centre of area, IFC 10.
    Centroid,
    /// The shear centre, IFC 15.
    ShearCentre,
}

/// A cardinal point read from IFC, and whether IFC's value was kept exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IfcCardinalPoint {
    /// The supported point.
    pub point: CardinalPoint,
    /// False when IFC asked for a point "in line with" the centroid or shear centre (11–14,
    /// 16–19), replaced by the nearest bounding-box point on the same side.
    pub exact: bool,
}

const GRID: [CardinalPoint; 9] = [
    CardinalPoint::BottomLeft,
    CardinalPoint::BottomCentre,
    CardinalPoint::BottomRight,
    CardinalPoint::MiddleLeft,
    CardinalPoint::MiddleCentre,
    CardinalPoint::MiddleRight,
    CardinalPoint::TopLeft,
    CardinalPoint::TopCentre,
    CardinalPoint::TopRight,
];

/// Nearest supported point for the "in line with" codes: bottom, left, right, top.
const IN_LINE: [CardinalPoint; 4] = [
    CardinalPoint::BottomCentre,
    CardinalPoint::MiddleLeft,
    CardinalPoint::MiddleRight,
    CardinalPoint::TopCentre,
];

impl CardinalPoint {
    /// The IFC `IfcCardinalPointReference` value.
    #[must_use]
    pub const fn ifc_code(self) -> u8 {
        match self {
            Self::BottomLeft => 1,
            Self::BottomCentre => 2,
            Self::BottomRight => 3,
            Self::MiddleLeft => 4,
            Self::MiddleCentre => 5,
            Self::MiddleRight => 6,
            Self::TopLeft => 7,
            Self::TopCentre => 8,
            Self::TopRight => 9,
            Self::Centroid => 10,
            Self::ShearCentre => 15,
        }
    }

    /// The point for an IFC `IfcCardinalPointReference` value.
    ///
    /// # Errors
    ///
    /// [`LocationError::UnknownCardinalPoint`] outside 1–19.
    pub const fn from_ifc_code(code: u8) -> Result<IfcCardinalPoint, LocationError> {
        let (point, exact) = match code {
            1..=9 => (GRID[code as usize - 1], true),
            10 => (Self::Centroid, true),
            15 => (Self::ShearCentre, true),
            11..=14 => (IN_LINE[code as usize - 11], false),
            16..=19 => (IN_LINE[code as usize - 16], false),
            _ => return Err(LocationError::UnknownCardinalPoint(code)),
        };
        Ok(IfcCardinalPoint { point, exact })
    }

    /// Where a grid point sits across and up the bounding box; `None` for the others. IFC
    /// numbers the grid row by row from the bottom left, so the code gives both.
    const fn grid(self) -> Option<(Side, Side)> {
        const SIDES: [Side; 3] = [Side::Low, Side::Middle, Side::High];
        let index = self.ifc_code() as usize;
        if index > GRID.len() {
            return None;
        }
        Some((SIDES[(index - 1) % 3], SIDES[(index - 1) / 3]))
    }
}

/// A position along one side of a bounding box.
#[derive(Clone, Copy)]
enum Side {
    Low,
    Middle,
    High,
}

impl Side {
    fn pick(self, min: f64, max: f64) -> f64 {
        match self {
            Self::Low => min,
            Self::Middle => f64::midpoint(min, max),
            Self::High => max,
        }
    }
}

/// Where a profile-based element sits on its axis: a cardinal point of its section plus an
/// offset along the section's y (across) and z (up) axes, in metres (STR-14).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocationPoint {
    /// The point of the section placed on the axis, before the offset.
    pub cardinal: CardinalPoint,
    /// Offset along the section's y axis (profile x), in metres.
    pub offset_y: f64,
    /// Offset along the section's z axis (profile y), in metres.
    pub offset_z: f64,
}

impl LocationPoint {
    /// A cardinal point with no offset.
    #[must_use]
    pub const fn at(cardinal: CardinalPoint) -> Self {
        Self {
            cardinal,
            offset_y: 0.0,
            offset_z: 0.0,
        }
    }

    /// The default for columns: the centroid.
    #[must_use]
    pub const fn column() -> Self {
        Self::at(CardinalPoint::Centroid)
    }

    /// The default for beams: the top centre, so the top face sits at the level.
    #[must_use]
    pub const fn beam() -> Self {
        Self::at(CardinalPoint::TopCentre)
    }

    /// Where the cardinal point is in the profile's own coordinates, before the offset.
    ///
    /// `shear_centre` is a shear centre the caller already knows, from a section catalogue or
    /// an earlier [`Profile::shear_centre`]; with `None`, it is computed when the cardinal point
    /// asks for it, which costs a mesh and a solve.
    ///
    /// # Errors
    ///
    /// [`LocationError::ShearCentreUnavailable`] when the shear centre is needed, not given, and
    /// cannot be computed.
    pub fn point_on(
        &self,
        profile: &Profile,
        shear_centre: Option<Point2>,
    ) -> Result<Point2, LocationError> {
        if let Some((across, up)) = self.cardinal.grid() {
            let [min_x, min_y, max_x, max_y] = profile.bounds();
            return Ok(Point2::new(
                across.pick(min_x, max_x),
                up.pick(min_y, max_y),
            ));
        }
        match (self.cardinal, shear_centre) {
            (CardinalPoint::Centroid, _) => Ok(profile.centroid()),
            (_, Some(known)) => Ok(known),
            (_, None) => profile
                .shear_centre()
                .map_err(LocationError::ShearCentreUnavailable),
        }
    }

    /// The translation that places the section on the element's axis: the cardinal point lands
    /// at the offset from the axis.
    ///
    /// # Errors
    ///
    /// As [`point_on`](Self::point_on), and [`LocationError::InvalidOffset`] when an offset is
    /// not a finite number within the model extent.
    pub fn resolve(
        &self,
        profile: &Profile,
        shear_centre: Option<Point2>,
    ) -> Result<Point2, LocationError> {
        if !(is_model_coordinate(self.offset_y) && is_model_coordinate(self.offset_z)) {
            return Err(LocationError::InvalidOffset);
        }
        let p = self.point_on(profile, shear_centre)?;
        Ok(Point2::new(self.offset_y - p.x, self.offset_z - p.y))
    }
}
