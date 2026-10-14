//! Arcs replaced by chords, for operations that build planar faces only. Rationale:
//! `docs/dev/adr/0008-arc-faceting.md`.

use std::borrow::Cow;

use crate::ARC_CHORD_TOLERANCE;
use crate::profile::edge::{Arc, Curve, Edge, point_on_circle};
use crate::profile::{MAX_PROFILE_EDGES, Profile, ProfileError};

impl Profile {
    /// The same profile with every arc replaced by chords at most [`ARC_CHORD_TOLERANCE`] away
    /// from it, validated again as a straight-edged profile; the profile itself when it has no
    /// arcs.
    ///
    /// # Errors
    ///
    /// [`ProfileError::TooManyEdges`] when the chords exceed [`MAX_PROFILE_EDGES`], or any error
    /// of [`Profile::new`] when the chords make loops meet.
    pub(crate) fn faceted(&self) -> Result<Cow<'_, Self>, ProfileError> {
        let mut edges = self.outer.iter().chain(self.holes.iter().flatten());
        if edges.all(|edge| matches!(edge, Edge::Line { .. })) {
            // Nothing to replace, and the profile is already valid.
            return Ok(Cow::Borrowed(self));
        }
        let mut budget = MAX_PROFILE_EDGES;
        let outer = facet_loop(&self.outer, &mut budget)?;
        let holes = self
            .holes
            .iter()
            .map(|hole| facet_loop(hole, &mut budget))
            .collect::<Result<_, _>>()?;
        Self::new(outer, holes).map(Cow::Owned)
    }
}

/// The loop with its arcs replaced by chords, taking the edges it makes out of `budget`.
fn facet_loop(edges: &[Edge], budget: &mut usize) -> Result<Vec<Edge>, ProfileError> {
    let mut faceted = Vec::with_capacity(edges.len());
    for edge in edges {
        let Curve::Arc(arc) = edge.curve() else {
            *budget = budget.checked_sub(1).ok_or(ProfileError::TooManyEdges)?;
            faceted.push(*edge);
            continue;
        };
        let count = chord_count(&arc, *budget).ok_or(ProfileError::TooManyEdges)?;
        *budget -= count;
        let mut start = arc.start;
        for i in 1..count {
            #[expect(
                clippy::cast_precision_loss,
                reason = "counts are at most MAX_PROFILE_EDGES"
            )]
            let fraction = i as f64 / count as f64;
            let end = point_on_circle(
                arc.centre,
                arc.radius,
                arc.sweep.mul_add(fraction, arc.start_angle),
            );
            faceted.push(Edge::Line { start, end });
            start = end;
        }
        // The last chord ends exactly at the arc's end, so the loop stays exactly closed.
        faceted.push(Edge::Line {
            start,
            end: arc.end,
        });
    }
    Ok(faceted)
}

/// The fewest chords of equal angle whose sagitta is within [`ARC_CHORD_TOLERANCE`]: at least
/// one, and three for a full circle. `None` when more than `budget` are needed.
fn chord_count(arc: &Arc, budget: usize) -> Option<usize> {
    // Half the largest angle a chord may span; the clamp covers radii below half the tolerance.
    let half_angle = libm::acos((1.0 - ARC_CHORD_TOLERANCE / arc.radius).max(-1.0));
    let minimum = if arc.is_full_circle() { 3.0 } else { 1.0 };
    let count = (arc.sweep.abs() / (2.0 * half_angle)).ceil().max(minimum);
    #[expect(
        clippy::cast_precision_loss,
        reason = "the budget is at most MAX_PROFILE_EDGES"
    )]
    let fits = count <= budget as f64;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a whole number between 1 and the budget"
    )]
    fits.then_some(count as usize)
}

#[cfg(test)]
mod tests {
    use core::f64::consts::PI;

    use super::*;
    use crate::Point2;
    use crate::profile::edge::{distance, sub};
    use crate::profile::tests::{bulged_square, ccw, circle, line, p, polygon};

    fn arc_of(edge: Edge) -> Arc {
        let Curve::Arc(arc) = edge.curve() else {
            unreachable!("an arc was expected")
        };
        arc
    }

    fn chord_ends(edges: &[Edge]) -> Vec<Point2> {
        edges.iter().map(Edge::end).collect()
    }

    /// The sagitta of every chord: the distance from its midpoint's projection on the circle.
    fn max_sagitta(edges: &[Edge], centre: Point2, radius: f64) -> f64 {
        edges
            .iter()
            .map(|e| {
                let mid = Point2::new(
                    f64::midpoint(e.start().x, e.end().x),
                    f64::midpoint(e.start().y, e.end().y),
                );
                radius - distance(mid, centre)
            })
            .fold(0.0, f64::max)
    }

    #[test]
    fn chord_count_follows_the_sagitta_formula() {
        // r = 0.5 m: acos(1 − 0.002) ≈ 0.0633, so a full turn needs ⌈2π / 0.1265⌉ = 50 chords.
        let column = arc_of(circle(0.0, 0.0, 0.5)[0]);
        assert_eq!(chord_count(&column, MAX_PROFILE_EDGES), Some(50));
        let quarter = arc_of(ccw(p(1.0, 0.0), p(0.0, 1.0), p(0.0, 0.0)));
        // r = 1 m: acos(0.999) ≈ 0.04473, ⌈(π/2) / 0.08946⌉ = 18.
        assert_eq!(chord_count(&quarter, MAX_PROFILE_EDGES), Some(18));
    }

    #[test]
    fn small_arcs_keep_a_minimum_of_chords() {
        let tiny_circle = arc_of(circle(0.0, 0.0, 1e-4)[0]);
        assert_eq!(chord_count(&tiny_circle, MAX_PROFILE_EDGES), Some(3));
        let short = arc_of(ccw(
            p(1.0, 0.0),
            p(libm::cos(0.01), libm::sin(0.01)),
            p(0.0, 0.0),
        ));
        assert_eq!(chord_count(&short, MAX_PROFILE_EDGES), Some(1));
    }

    #[test]
    fn chord_count_respects_the_budget() {
        let column = arc_of(circle(0.0, 0.0, 0.5)[0]);
        assert_eq!(chord_count(&column, 50), Some(50));
        assert_eq!(chord_count(&column, 49), None);
    }

    #[test]
    fn a_circle_becomes_chords_within_the_tolerance_closing_exactly() {
        let profile = Profile::new(circle(2.0, 1.0, 0.5), vec![]).unwrap();
        let faceted = profile.faceted().unwrap();
        let edges = faceted.outer();
        assert_eq!(edges.len(), 50);
        assert!(edges.iter().all(|e| matches!(e, Edge::Line { .. })));
        assert_eq!(edges[0].start(), p(2.5, 1.0));
        assert_eq!(edges[49].end(), p(2.5, 1.0));
        for e in edges {
            assert!((distance(e.end(), p(2.0, 1.0)) - 0.5).abs() < 1e-12);
        }
        let sagitta = max_sagitta(edges, p(2.0, 1.0), 0.5);
        assert!(sagitta > 0.0 && sagitta <= ARC_CHORD_TOLERANCE);
        // The area of a regular 50-gon inscribed in the circle.
        let expected = 25.0 * 0.25 * libm::sin(2.0 * PI / 50.0);
        assert!((faceted.area() - expected).abs() < 1e-12);
    }

    #[test]
    fn clockwise_arcs_and_holes_are_faceted_in_their_direction() {
        let mut hole = circle(1.0, 1.0, 0.25);
        hole[0] = hole[0].reversed();
        let profile = Profile::new(
            polygon(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]),
            vec![hole],
        )
        .unwrap();
        let faceted = profile.faceted().unwrap();
        let hole = faceted.holes().next().unwrap();
        assert_eq!(hole.len(), 36);
        assert!(faceted.area() < 4.0 - 0.99 * PI * 0.0625);
        assert!(faceted.area() > 4.0 - PI * 0.0625);
        let ends = chord_ends(hole);
        // Clockwise: each chord turns the angle the negative way around the centre.
        let turn =
            crate::profile::edge::cross(sub(ends[0], p(1.0, 1.0)), sub(ends[1], p(1.0, 1.0)));
        assert!(turn < 0.0);
    }

    #[test]
    fn straight_profiles_are_unchanged_and_mixed_ones_keep_their_lines() {
        let square = Profile::new(
            polygon(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]),
            vec![],
        )
        .unwrap();
        assert!(matches!(square.faceted(), Ok(Cow::Borrowed(same)) if *same == square));
        let bulged = Profile::new(bulged_square(), vec![]).unwrap();
        let bulged = bulged.faceted().unwrap();
        // Three lines and a half circle of radius 1 m: ⌈π / 0.08946⌉ = 36 chords.
        assert_eq!(bulged.outer().len(), 3 + 36);
        assert_eq!(bulged.outer()[0], line(p(0.0, 0.0), p(2.0, 0.0)));
    }

    #[test]
    fn chords_that_would_meet_another_loop_are_rejected() {
        // The hole's corner is 0.1 mm inside the circle, halfway between two chord ends, where
        // the chord cuts almost 1 mm into the circle.
        let outer = circle(0.0, 0.0, 1.0);
        let chords = chord_count(&arc_of(outer[0]), MAX_PROFILE_EDGES).unwrap();
        #[expect(clippy::cast_precision_loss, reason = "a small count")]
        let angle = PI / chords as f64;
        let corner = (
            (1.0 - 1e-4) * libm::cos(angle),
            (1.0 - 1e-4) * libm::sin(angle),
        );
        let hole = polygon(&[corner, (0.5, -0.1), (0.5, 0.1)]);
        let profile = Profile::new(outer, vec![hole]).unwrap();
        assert!(matches!(
            profile.faceted(),
            Err(ProfileError::LoopsIntersect { .. } | ProfileError::HoleOutsideOuter(0))
        ));
    }

    #[test]
    fn too_many_chords_are_rejected() {
        // A 1 km radius circle needs over 2,000 chords; two of them pass the edge limit.
        let profile = Profile::new(circle(0.0, 0.0, 1e3), vec![circle(0.0, 0.0, 999.0)]);
        let profile = profile.unwrap();
        assert_eq!(profile.faceted(), Err(ProfileError::TooManyEdges));
        let lines = polygon(&[(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)]);
        assert_eq!(facet_loop(&lines, &mut 2), Err(ProfileError::TooManyEdges));
    }
}
