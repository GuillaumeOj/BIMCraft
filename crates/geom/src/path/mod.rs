//! Open 3D paths of lines and circular arcs, along which profiles are swept (GEO-02).
//!
//! Rationale: `docs/dev/adr/0020-sweeps-and-revolutions.md`.

mod validate;

use crate::Point3;
use crate::vector::Vector;

/// Largest number of segments a path may have.
///
/// Sweeping compares pieces of the path pairwise; the bound, with the sweep's own piece limit,
/// keeps hostile input from making it run for minutes.
pub const MAX_PATH_SEGMENTS: usize = 256;

/// One segment of a path, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathSegment {
    /// A straight segment.
    Line {
        /// First point.
        start: Point3,
        /// Last point.
        end: Point3,
    },
    /// The circular arc from `start` to `end` through `through`. Three distinct points that are
    /// not collinear fix the arc's plane, radius and sense, so a full circle cannot be given.
    Arc {
        /// First point.
        start: Point3,
        /// A point of the arc between its ends.
        through: Point3,
        /// Last point.
        end: Point3,
    },
}

impl PathSegment {
    /// First point of the segment.
    #[must_use]
    pub const fn start(&self) -> Point3 {
        match *self {
            Self::Line { start, .. } | Self::Arc { start, .. } => start,
        }
    }

    /// Last point of the segment.
    #[must_use]
    pub const fn end(&self) -> Point3 {
        match *self {
            Self::Line { end, .. } | Self::Arc { end, .. } => end,
        }
    }

    /// Every point that defines the segment (a line's end twice).
    const fn points(&self) -> [Point3; 3] {
        match *self {
            Self::Line { start, end } => [start, end, end],
            Self::Arc {
                start,
                through,
                end,
            } => [start, through, end],
        }
    }
}

/// Why a path was rejected. Segments are identified by their position in the input (from 0);
/// coordinates are model content and are not carried (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum PathError {
    /// The path has no segment.
    #[error("the path has no segments")]
    Empty,
    /// The path has more than [`MAX_PATH_SEGMENTS`] segments.
    #[error("the path has more than {MAX_PATH_SEGMENTS} segments")]
    TooManySegments,
    /// A coordinate is NaN or infinite.
    #[error("segment {segment} has a coordinate that is not a finite number")]
    NonFinite {
        /// The segment.
        segment: usize,
    },
    /// A coordinate is outside the model extent, or so close to zero that exact predicates
    /// cannot handle it.
    #[error("segment {segment} has a coordinate outside the model extent")]
    OutOfRange {
        /// The segment.
        segment: usize,
    },
    /// A segment does not end exactly where the next one starts.
    #[error("segment {segment} does not end where the next segment starts")]
    Gap {
        /// The segment whose end is not the next segment's start.
        segment: usize,
    },
    /// A line is no longer than [`LINEAR_TOLERANCE`](crate::LINEAR_TOLERANCE).
    #[error("segment {segment} is a line of zero length")]
    ZeroLengthLine {
        /// The segment.
        segment: usize,
    },
    /// Two points of an arc are within [`LINEAR_TOLERANCE`](crate::LINEAR_TOLERANCE) of each
    /// other, or its middle point is within it of the line through its ends.
    #[error("segment {segment} is a degenerate arc")]
    DegenerateArc {
        /// The segment.
        segment: usize,
    },
    /// The path ends where it starts, within [`LINEAR_TOLERANCE`](crate::LINEAR_TOLERANCE).
    #[error("the path is closed")]
    Closed,
}

/// A valid open path: segments each ending exactly where the next one starts, none degenerate,
/// the last not ending where the first starts.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    segments: Vec<PathSegment>,
    curves: Vec<Curve3>,
}

impl Path {
    /// Validates the segments and builds the path.
    ///
    /// # Errors
    ///
    /// [`PathError`] when the path is empty, too long or closed, has a coordinate that is not
    /// finite or out of range, a gap between two segments, a line of zero length or a degenerate
    /// arc.
    pub fn new(segments: Vec<PathSegment>) -> Result<Self, PathError> {
        let curves = validate::check(&segments)?;
        Ok(Self { segments, curves })
    }

    /// The segments, as given.
    #[must_use]
    pub fn segments(&self) -> &[PathSegment] {
        &self.segments
    }

    /// The geometry of each segment.
    pub(crate) fn curves(&self) -> &[Curve3] {
        &self.curves
    }
}

/// The geometry of a path segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Curve3 {
    /// A straight segment, by its end: it starts where the previous segment ends.
    Line(Vector),
    /// A circular arc.
    Arc(Arc3),
}

/// A circular arc of a path, from `start` to `end`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Arc3 {
    /// First point.
    pub(crate) start: Vector,
    /// Last point.
    pub(crate) end: Vector,
    /// Radius, in metres.
    pub(crate) radius: f64,
    /// Unit vector from the centre to `start`.
    pub(crate) outward: Vector,
    /// Unit tangent at `start`, in the direction of travel.
    pub(crate) tangent: Vector,
    /// Angle from `start` to `end`, in radians, in (0, 2π).
    pub(crate) sweep: f64,
}

#[cfg(test)]
pub(crate) mod tests;
