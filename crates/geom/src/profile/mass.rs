//! Area and first moments of profile loops, from Green's theorem over each edge.
//!
//! A line contributes the triangle it forms with the reference origin. An arc contributes the
//! triangle of its chord plus the circular segment between chord and arc, both in closed form, so
//! the result is exact up to floating-point rounding.

use crate::Point2;
use crate::profile::edge::{Arc, Curve, Edge, cross, sub};

/// Signed area and first moments of a region, relative to a reference origin.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Moments {
    /// Positive when the boundary turns counterclockwise.
    pub(crate) area: f64,
    pub(crate) x: f64,
    pub(crate) y: f64,
}

impl Moments {
    pub(crate) fn add(self, other: Self) -> Self {
        Self {
            area: self.area + other.area,
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

/// Signed moments of the region enclosed by a closed loop, with coordinates taken from `origin`.
pub(crate) fn loop_moments(edges: &[Edge], origin: Point2) -> Moments {
    edges
        .iter()
        .map(|edge| match edge.curve() {
            Curve::Segment(a, b) => triangle(sub(a, origin), sub(b, origin)),
            Curve::Arc(arc) => triangle(sub(arc.start, origin), sub(arc.end, origin))
                .add(circular_segment(&arc, origin)),
        })
        .fold(Moments::default(), Moments::add)
}

/// The triangle (origin, a, b).
fn triangle(a: Point2, b: Point2) -> Moments {
    let area = cross(a, b) / 2.0;
    Moments {
        area,
        x: (a.x + b.x) * area / 3.0,
        y: (a.y + b.y) * area / 3.0,
    }
}

/// The region between an arc and its chord, signed like the arc's sweep.
fn circular_segment(arc: &Arc, origin: Point2) -> Moments {
    let theta = arc.sweep;
    let r2 = arc.radius * arc.radius;
    let area = r2 / 2.0 * theta_minus_sin(theta);
    // The centroid lies on the bisector at 4 r sin³(θ/2) / (3 (θ − sin θ)) from the centre; times
    // the area this is (2/3) r³ sin³(θ/2), which avoids dividing by a vanishing θ − sin θ.
    let half_sine = libm::sin(theta / 2.0);
    let offset = 2.0 / 3.0 * r2 * arc.radius * half_sine * half_sine * half_sine;
    let bisector = arc.start_angle + theta / 2.0;
    let centre = sub(arc.centre, origin);
    Moments {
        area,
        x: area.mul_add(centre.x, offset * libm::cos(bisector)),
        y: area.mul_add(centre.y, offset * libm::sin(bisector)),
    }
}

/// θ − sin θ without the cancellation of the direct formula for small angles.
fn theta_minus_sin(theta: f64) -> f64 {
    if theta.abs() < 0.1 {
        // Taylor series; the first omitted term is below 2⁻⁵³ of the result for |θ| < 0.1.
        let t2 = theta * theta;
        theta * t2 / 6.0
            * (1.0 - t2 / 20.0 * (1.0 - t2 / 42.0 * (1.0 - t2 / 72.0 * (1.0 - t2 / 110.0))))
    } else {
        theta - libm::sin(theta)
    }
}

#[cfg(test)]
mod tests {
    use core::f64::consts::PI;

    use super::*;
    use crate::profile::tests::{assert_close as close, ccw as arc, line, p, polygon};

    #[test]
    fn series_matches_the_direct_formula_where_both_are_accurate() {
        for theta in [0.099_999, -0.099_999] {
            close(theta_minus_sin(theta), theta - libm::sin(theta));
        }
        let tiny = 1e-6_f64;
        close(theta_minus_sin(tiny), tiny * tiny * tiny / 6.0);
    }

    #[test]
    fn a_square_has_its_area_and_first_moments() {
        let edges = polygon(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]);
        let m = loop_moments(&edges, p(0.0, 0.0));
        assert_eq!(
            m,
            Moments {
                area: 4.0,
                x: 4.0,
                y: 4.0
            }
        );
        let reversed: Vec<Edge> = edges.iter().rev().map(Edge::reversed).collect();
        let area = loop_moments(&reversed, p(0.0, 0.0)).area;
        assert_eq!(area.to_bits(), (-4.0_f64).to_bits());
    }

    #[test]
    fn a_full_circle_is_pi_r_squared_at_its_centre() {
        let centre = p(3.0, -1.0);
        let start = p(5.0, -1.0);
        let m = loop_moments(&[arc(start, start, centre)], p(0.0, 0.0));
        close(m.area, 4.0 * PI);
        close(m.x / m.area, 3.0);
        close(m.y / m.area, -1.0);
    }

    #[test]
    fn a_half_disc_has_its_centroid_at_four_r_over_three_pi() {
        let left = p(-1.0, 0.0);
        let right = p(1.0, 0.0);
        let edges = [arc(right, left, p(0.0, 0.0)), line(left, right)];
        let m = loop_moments(&edges, right);
        close(m.area, PI / 2.0);
        close(m.x / m.area, -1.0);
        close(m.y / m.area, 4.0 / (3.0 * PI));
    }
}
