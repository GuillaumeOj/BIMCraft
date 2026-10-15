//! Inputs the fuzz targets of `fuzz/` once broke (ENG-02), decoded into plain tests. Each one
//! names the seed file that keeps replaying it in the fuzz run.

use geom::{ArcDirection, Edge, LoopId, Point2, Profile, ProfileError};

/// A point given in quarter metres, scaled by `2^exponent`.
fn grid(x: f64, y: f64, exponent: i32) -> Point2 {
    let scale = libm::ldexp(0.25, exponent);
    Point2::new(x * scale, y * scale)
}

/// An arc from `start` to `end` whose centre lies `offset` chords away along the chord's left
/// normal.
fn arc(start: Point2, end: Point2, offset: f64, direction: ArcDirection) -> Edge {
    let centre = Point2::new(
        f64::midpoint(start.x, end.x) + offset * (start.y - end.y),
        f64::midpoint(start.y, end.y) + offset * (end.x - start.x),
    );
    Edge::Arc {
        start,
        end,
        centre,
        direction,
    }
}

/// `fuzz/seeds/profile/near-circle-crossed-at-its-ends`: an arc of almost a full turn, its ends
/// 0.5 µm apart, crossed by the next arc within the tolerance of their common end, where the
/// check between consecutive edges does not look. The loop crossed itself and was accepted, its
/// centroid outside its bounds.
#[test]
fn an_arc_between_ends_within_the_tolerance_is_degenerate() {
    let (a, b, c) = (
        grid(-1.0, -1.0, -24),
        grid(11.0, -33.0, -24),
        grid(0.0, -49.0, -24),
    );
    let crossed = vec![
        arc(a, b, -2.125, ArcDirection::CounterClockwise),
        arc(b, c, 3.875, ArcDirection::Clockwise),
        Edge::Line { start: c, end: a },
    ];
    assert_eq!(
        Profile::new(crossed, vec![]),
        Err(ProfileError::DegenerateArc {
            loop_id: LoopId::Outer,
            edge: 0,
        })
    );
}

/// `fuzz/seeds/profile/arc-retraced-within-tolerance` and `…-chord-beyond-tolerance`: two arcs
/// of one circle running there and back enclose nothing. Every point of each arc lies within the
/// tolerance of one of its ends, even when the ends are farther apart, so the check between
/// consecutive edges skipped them all and the loop was accepted with a zero area.
#[test]
fn an_arc_within_the_tolerance_of_its_ends_is_degenerate() {
    let degenerate = Err(ProfileError::DegenerateArc {
        loop_id: LoopId::Outer,
        edge: 0,
    });
    for (a, b) in [
        // Ends 0.8 µm apart.
        (grid(-45.0, 70.0, -24), grid(7.0, 64.0, -24)),
        // Ends 1.9 µm apart.
        (grid(-119.0, -127.0, -24), grid(-119.0, -1.0, -24)),
    ] {
        let there_and_back = vec![
            arc(a, b, -3.875, ArcDirection::Clockwise),
            arc(b, a, 3.875, ArcDirection::CounterClockwise),
        ];
        assert_eq!(Profile::new(there_and_back, vec![]), degenerate);
    }
}

/// `fuzz/seeds/profile/tangent-cusp`: two arcs exactly tangent at their common end, the loop
/// turning back on itself there. They stay within the tolerance of each other for centimetres,
/// a sliver, but only one loop direction found a point of it outside the tolerance of the end.
#[test]
fn arcs_leaving_their_common_end_together_meet() {
    let [a, b, c, d] = [
        (-49.0, -65.0),
        (-105.0, -119.0),
        (-119.0, -119.0),
        (-66.0, -119.0),
    ]
    .map(|(x, y)| grid(x, y, 4));
    let cusp = vec![
        arc(a, b, -2.875, ArcDirection::Clockwise),
        arc(b, c, -3.875, ArcDirection::Clockwise),
        arc(c, d, 3.875, ArcDirection::CounterClockwise),
        Edge::Line { start: d, end: a },
    ];
    let reversed = cusp.iter().rev().map(Edge::reversed).collect();
    let crossing = |first, second| {
        Err(ProfileError::SelfIntersecting {
            loop_id: LoopId::Outer,
            first,
            second,
        })
    };
    assert_eq!(Profile::new(cusp, vec![]), crossing(1, 2));
    assert_eq!(Profile::new(reversed, vec![]), crossing(1, 2));
}
