//! Profile edges and the plane geometry the validation and mass properties share.

use core::f64::consts::TAU;

use crate::Point2;

/// The way an arc turns around its centre, seen from above (+z).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArcDirection {
    /// Counterclockwise: the angle increases from start to end.
    CounterClockwise,
    /// Clockwise: the angle decreases from start to end.
    Clockwise,
}

impl ArcDirection {
    /// Returns the other direction.
    #[must_use]
    pub const fn reversed(self) -> Self {
        match self {
            Self::CounterClockwise => Self::Clockwise,
            Self::Clockwise => Self::CounterClockwise,
        }
    }
}

/// One edge of a profile loop, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edge {
    /// A straight segment.
    Line {
        /// First point.
        start: Point2,
        /// Last point.
        end: Point2,
    },
    /// A circular arc from `start` to `end` around `centre`, turning in `direction`.
    ///
    /// The radius is the distance from `centre` to `start`; `end` must be at the same distance,
    /// within [`LINEAR_TOLERANCE`](crate::LINEAR_TOLERANCE). An arc whose `end` equals its `start`
    /// is a full circle.
    Arc {
        /// First point.
        start: Point2,
        /// Last point.
        end: Point2,
        /// Centre of the circle.
        centre: Point2,
        /// Turning direction from `start` to `end`.
        direction: ArcDirection,
    },
}

impl Edge {
    /// First point of the edge.
    #[must_use]
    pub const fn start(&self) -> Point2 {
        match *self {
            Self::Line { start, .. } | Self::Arc { start, .. } => start,
        }
    }

    /// Last point of the edge.
    #[must_use]
    pub const fn end(&self) -> Point2 {
        match *self {
            Self::Line { end, .. } | Self::Arc { end, .. } => end,
        }
    }

    /// The same edge travelled the other way.
    #[must_use]
    pub const fn reversed(&self) -> Self {
        match *self {
            Self::Line { start, end } => Self::Line {
                start: end,
                end: start,
            },
            Self::Arc {
                start,
                end,
                centre,
                direction,
            } => Self::Arc {
                start: end,
                end: start,
                centre,
                direction: direction.reversed(),
            },
        }
    }

    /// Every point defining the edge, the centre included.
    pub(crate) fn points(&self) -> impl Iterator<Item = Point2> {
        let (start, end, centre) = match *self {
            Self::Line { start, end } => (start, end, None),
            Self::Arc {
                start, end, centre, ..
            } => (start, end, Some(centre)),
        };
        [start, end].into_iter().chain(centre)
    }

    pub(crate) fn curve(&self) -> Curve {
        match *self {
            Self::Line { start, end } => Curve::Segment(start, end),
            Self::Arc {
                start,
                end,
                centre,
                direction,
            } => Curve::Arc(Arc::new(start, end, centre, direction)),
        }
    }
}

/// The geometry of an edge, with the derived values of arcs computed once.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Curve {
    Segment(Point2, Point2),
    Arc(Arc),
}

impl Curve {
    pub(crate) fn distance_to(&self, p: Point2) -> f64 {
        match self {
            Self::Segment(a, b) => distance_to_segment(p, *a, *b),
            Self::Arc(arc) => arc.distance_to(p),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Arc {
    pub(crate) start: Point2,
    pub(crate) end: Point2,
    pub(crate) centre: Point2,
    pub(crate) radius: f64,
    pub(crate) start_angle: f64,
    /// Signed swept angle: positive counterclockwise, ±2π for a full circle.
    pub(crate) sweep: f64,
}

impl Arc {
    fn new(start: Point2, end: Point2, centre: Point2, direction: ArcDirection) -> Self {
        let start_angle = angle_of(sub(start, centre));
        let turn = if start == end {
            TAU
        } else {
            let ccw = (angle_of(sub(end, centre)) - start_angle).rem_euclid(TAU);
            match direction {
                ArcDirection::CounterClockwise => ccw,
                ArcDirection::Clockwise => TAU - ccw,
            }
        };
        let sweep = match direction {
            ArcDirection::CounterClockwise => turn,
            ArcDirection::Clockwise => -turn,
        };
        Self {
            start,
            end,
            centre,
            radius: length(sub(start, centre)),
            start_angle,
            sweep,
        }
    }

    pub(crate) fn is_full_circle(&self) -> bool {
        self.start == self.end
    }

    pub(crate) fn point_at_angle(&self, angle: f64) -> Point2 {
        Point2::new(
            libm::cos(angle).mul_add(self.radius, self.centre.x),
            libm::sin(angle).mul_add(self.radius, self.centre.y),
        )
    }

    /// Whether the ray from the centre at `angle` passes through the arc.
    fn spans(&self, angle: f64) -> bool {
        let offset = if self.sweep > 0.0 {
            angle - self.start_angle
        } else {
            self.start_angle - angle
        };
        offset.rem_euclid(TAU) <= self.sweep.abs()
    }

    fn distance_to(&self, p: Point2) -> f64 {
        let from_centre = sub(p, self.centre);
        if self.spans(angle_of(from_centre)) {
            (length(from_centre) - self.radius).abs()
        } else {
            distance(p, self.start).min(distance(p, self.end))
        }
    }

    /// The two points of the circle on the line through the centre with direction `unit`.
    pub(crate) fn antipodes(&self, unit: Point2) -> [Point2; 2] {
        let (dx, dy) = (unit.x * self.radius, unit.y * self.radius);
        [
            Point2::new(self.centre.x + dx, self.centre.y + dy),
            Point2::new(self.centre.x - dx, self.centre.y - dy),
        ]
    }
}

/// Vectors are carried as [`Point2`] holding the difference.
pub(crate) fn sub(a: Point2, b: Point2) -> Point2 {
    Point2::new(a.x - b.x, a.y - b.y)
}

pub(crate) fn dot(a: Point2, b: Point2) -> f64 {
    a.x.mul_add(b.x, a.y * b.y)
}

pub(crate) fn cross(a: Point2, b: Point2) -> f64 {
    a.x.mul_add(b.y, -(a.y * b.x))
}

pub(crate) fn length(v: Point2) -> f64 {
    libm::hypot(v.x, v.y)
}

pub(crate) fn distance(a: Point2, b: Point2) -> f64 {
    length(sub(a, b))
}

pub(crate) fn angle_of(v: Point2) -> f64 {
    libm::atan2(v.y, v.x)
}

fn distance_to_segment(p: Point2, a: Point2, b: Point2) -> f64 {
    let ab = sub(b, a);
    let t = (dot(sub(p, a), ab) / dot(ab, ab)).clamp(0.0, 1.0);
    distance(p, Point2::new(t.mul_add(ab.x, a.x), t.mul_add(ab.y, a.y)))
}

#[cfg(test)]
mod tests {
    use core::f64::consts::{FRAC_PI_2, PI};

    use super::*;

    const O: Point2 = Point2::new(0.0, 0.0);
    const X: Point2 = Point2::new(1.0, 0.0);
    const Y: Point2 = Point2::new(0.0, 1.0);

    fn arc(start: Point2, end: Point2, direction: ArcDirection) -> Arc {
        Arc::new(start, end, O, direction)
    }

    #[test]
    fn endpoints_and_points_include_the_centre_of_arcs_only() {
        let line = Edge::Line { start: O, end: X };
        assert_eq!((line.start(), line.end()), (O, X));
        assert_eq!(line.points().collect::<Vec<_>>(), [O, X]);
        let quarter = Edge::Arc {
            start: X,
            end: Y,
            centre: O,
            direction: ArcDirection::CounterClockwise,
        };
        assert_eq!((quarter.start(), quarter.end()), (X, Y));
        assert_eq!(quarter.points().collect::<Vec<_>>(), [X, Y, O]);
    }

    #[test]
    fn reversing_swaps_ends_and_flips_arc_direction() {
        let line = Edge::Line { start: O, end: X };
        assert_eq!(line.reversed(), Edge::Line { start: X, end: O });
        let quarter = Edge::Arc {
            start: X,
            end: Y,
            centre: O,
            direction: ArcDirection::CounterClockwise,
        };
        assert_eq!(
            quarter.reversed(),
            Edge::Arc {
                start: Y,
                end: X,
                centre: O,
                direction: ArcDirection::Clockwise,
            }
        );
        assert_eq!(quarter.reversed().reversed(), quarter);
    }

    #[test]
    fn sweep_follows_the_direction() {
        let ccw = arc(X, Y, ArcDirection::CounterClockwise);
        assert!((ccw.sweep - FRAC_PI_2).abs() < 1e-15);
        let cw = arc(X, Y, ArcDirection::Clockwise);
        assert!((cw.sweep + 3.0 * FRAC_PI_2).abs() < 1e-15);
        assert!(!ccw.is_full_circle());
    }

    #[test]
    fn equal_ends_make_a_full_circle_in_either_direction() {
        let ccw = arc(X, X, ArcDirection::CounterClockwise);
        assert!(ccw.is_full_circle());
        assert_eq!(ccw.sweep.to_bits(), TAU.to_bits());
        assert_eq!(
            arc(X, X, ArcDirection::Clockwise).sweep.to_bits(),
            (-TAU).to_bits()
        );
    }

    #[test]
    fn distance_to_an_arc_uses_the_circle_within_its_span_and_the_ends_outside() {
        let quarter = Curve::Arc(arc(X, Y, ArcDirection::CounterClockwise));
        assert!((quarter.distance_to(Point2::new(2.0, 2.0)) - (8f64.sqrt() - 1.0)).abs() < 1e-15);
        assert!((quarter.distance_to(Point2::new(0.5, 0.0)) - 0.5).abs() < 1e-15);
        assert!((quarter.distance_to(Point2::new(0.0, -1.0)) - 2f64.sqrt()).abs() < 1e-15);
        let clockwise = Curve::Arc(arc(X, Y, ArcDirection::Clockwise));
        assert!((clockwise.distance_to(Point2::new(0.0, -2.0)) - 1.0).abs() < 1e-15);
        assert!((clockwise.distance_to(Point2::new(2.0, 2.0)) - 5f64.sqrt()).abs() < 1e-15);
    }

    #[test]
    fn distance_to_a_segment_clamps_to_its_ends() {
        let segment = Curve::Segment(O, X);
        assert_eq!(
            segment.distance_to(Point2::new(0.5, 2.0)).to_bits(),
            2.0_f64.to_bits()
        );
        assert_eq!(
            segment.distance_to(Point2::new(-3.0, 4.0)).to_bits(),
            5.0_f64.to_bits()
        );
        assert_eq!(
            segment.distance_to(Point2::new(4.0, 4.0)).to_bits(),
            5.0_f64.to_bits()
        );
    }

    #[test]
    fn points_and_antipodes_lie_on_the_circle() {
        let half = arc(X, Point2::new(-1.0, 0.0), ArcDirection::CounterClockwise);
        let top = half.point_at_angle(FRAC_PI_2);
        assert!(top.x.abs() < 1e-15 && (top.y - 1.0).abs() < 1e-15);
        assert_eq!(half.antipodes(Y), [Y, Point2::new(0.0, -1.0)]);
        assert!((half.sweep - PI).abs() < 1e-15);
    }
}
