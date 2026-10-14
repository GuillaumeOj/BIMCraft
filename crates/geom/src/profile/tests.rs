use core::f64::consts::PI;

use super::*;
use crate::MODEL_EXTENT;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

pub(super) fn polygon(points: &[(f64, f64)]) -> Vec<Edge> {
    (0..points.len())
        .map(|i| {
            let (x0, y0) = points[i];
            let (x1, y1) = points[(i + 1) % points.len()];
            Edge::Line {
                start: p(x0, y0),
                end: p(x1, y1),
            }
        })
        .collect()
}

fn rectangle(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Edge> {
    polygon(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
}

fn ccw(start: Point2, end: Point2, centre: Point2) -> Edge {
    Edge::Arc {
        start,
        end,
        centre,
        direction: ArcDirection::CounterClockwise,
    }
}

pub(super) fn circle(cx: f64, cy: f64, radius: f64) -> Vec<Edge> {
    let start = p(cx + radius, cy);
    vec![ccw(start, start, p(cx, cy))]
}

fn profile(outer: Vec<Edge>, holes: Vec<Vec<Edge>>) -> Profile {
    Profile::new(outer, holes).expect("valid profile")
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-12 * expected.abs().max(1.0),
        "{actual} != {expected}"
    );
}

fn assert_mass(profile: &Profile, area: f64, cx: f64, cy: f64) {
    assert_close(profile.area(), area);
    let c = profile.centroid();
    assert_close(c.x, cx);
    assert_close(c.y, cy);
}

#[test]
fn rectangle_has_hand_area_and_centroid() {
    assert_mass(
        &profile(rectangle(0.0, 0.0, 2.0, 1.0), vec![]),
        2.0,
        1.0,
        0.5,
    );
}

#[test]
fn rectangular_hole_is_subtracted() {
    let outer = rectangle(0.0, 0.0, 4.0, 3.0);
    let hole = rectangle(2.0, 1.0, 3.0, 2.0);
    // 12 m² centred at (2, 1.5) minus 1 m² centred at (2.5, 1.5).
    assert_mass(&profile(outer, vec![hole]), 11.0, 21.5 / 11.0, 1.5);
}

#[test]
fn circle_as_one_arc_or_two_half_arcs() {
    assert_mass(&profile(circle(1.0, 1.0, 2.0), vec![]), 4.0 * PI, 1.0, 1.0);
    let (east, west, centre) = (p(3.0, 1.0), p(-1.0, 1.0), p(1.0, 1.0));
    let halves = vec![ccw(east, west, centre), ccw(west, east, centre)];
    assert_mass(&profile(halves, vec![]), 4.0 * PI, 1.0, 1.0);
}

#[test]
fn square_topped_with_a_half_disc() {
    let edges = vec![
        Edge::Line {
            start: p(0.0, 0.0),
            end: p(2.0, 0.0),
        },
        Edge::Line {
            start: p(2.0, 0.0),
            end: p(2.0, 2.0),
        },
        ccw(p(2.0, 2.0), p(0.0, 2.0), p(1.0, 2.0)),
        Edge::Line {
            start: p(0.0, 2.0),
            end: p(0.0, 0.0),
        },
    ];
    let half = PI / 2.0;
    let area = 4.0 + half;
    let cy = (4.0 * 1.0 + half * (2.0 + 4.0 / (3.0 * PI))) / area;
    assert_mass(&profile(edges, vec![]), area, 1.0, cy);
}

#[test]
fn rounded_rectangle_loses_its_corners() {
    let (a, b, r) = (4.0, 2.0, 0.5);
    let corner = |cx: f64, cy: f64, from: (f64, f64), to: (f64, f64)| {
        ccw(p(from.0, from.1), p(to.0, to.1), p(cx, cy))
    };
    let line = |x0: f64, y0: f64, x1: f64, y1: f64| Edge::Line {
        start: p(x0, y0),
        end: p(x1, y1),
    };
    let edges = vec![
        line(r, 0.0, a - r, 0.0),
        corner(a - r, r, (a - r, 0.0), (a, r)),
        line(a, r, a, b - r),
        corner(a - r, b - r, (a, b - r), (a - r, b)),
        line(a - r, b, r, b),
        corner(r, b - r, (r, b), (0.0, b - r)),
        line(0.0, b - r, 0.0, r),
        corner(r, r, (0.0, r), (r, 0.0)),
    ];
    assert_mass(
        &profile(edges, vec![]),
        a * b - (4.0 - PI) * r * r,
        a / 2.0,
        b / 2.0,
    );
}

#[test]
fn square_with_an_off_centre_round_hole() {
    let outer = rectangle(0.0, 0.0, 4.0, 4.0);
    let area = 16.0 - PI;
    let cx = (16.0 * 2.0 - PI * 1.5) / area;
    assert_mass(&profile(outer, vec![circle(1.5, 2.0, 1.0)]), area, cx, 2.0);
}

#[test]
fn three_quarter_disc_uses_a_major_arc() {
    let o = p(0.0, 0.0);
    let edges = vec![
        Edge::Line {
            start: o,
            end: p(1.0, 0.0),
        },
        ccw(p(1.0, 0.0), p(0.0, -1.0), o),
        Edge::Line {
            start: p(0.0, -1.0),
            end: o,
        },
    ];
    // Sector of half-angle α = 3π/4: centroid at 2 r sin α / (3 α) along the bisector (135°).
    let c = 4.0 / (9.0 * PI);
    assert_mass(&profile(edges, vec![]), 3.0 * PI / 4.0, -c, c);
}

#[test]
fn thin_sector_uses_the_small_angle_series() {
    let o = p(0.0, 0.0);
    let theta = 0.05_f64;
    let tip = p(libm::cos(theta), libm::sin(theta));
    let edges = vec![
        Edge::Line {
            start: o,
            end: p(1.0, 0.0),
        },
        ccw(p(1.0, 0.0), tip, o),
        Edge::Line { start: tip, end: o },
    ];
    let d = 2.0 * libm::sin(theta / 2.0) / (3.0 * theta / 2.0);
    let cx = d * libm::cos(theta / 2.0);
    let cy = d * libm::sin(theta / 2.0);
    assert_mass(&profile(edges, vec![]), theta / 2.0, cx, cy);
}

#[test]
fn loops_are_reoriented_whatever_the_input_direction() {
    let outer = rectangle(0.0, 0.0, 4.0, 3.0);
    let hole = rectangle(2.0, 1.0, 3.0, 2.0);
    let canonical = profile(outer.clone(), vec![reversed(&hole)]);
    let flipped = profile(reversed(&outer), vec![hole.clone()]);
    assert_eq!(canonical, flipped);
    assert_eq!(flipped.outer(), outer.as_slice());
    assert_eq!(flipped.holes().len(), 1);
    assert_eq!(flipped.holes().next(), Some(reversed(&hole).as_slice()));
    let clockwise_circle = reversed(&circle(0.0, 0.0, 1.0));
    assert_close(profile(clockwise_circle, vec![]).area(), PI);
}

#[test]
fn line_tangent_to_an_arc_is_valid() {
    // A stadium: two half circles joined by tangent lines.
    let edges = vec![
        Edge::Line {
            start: p(0.0, -1.0),
            end: p(2.0, -1.0),
        },
        ccw(p(2.0, -1.0), p(2.0, 1.0), p(2.0, 0.0)),
        Edge::Line {
            start: p(2.0, 1.0),
            end: p(0.0, 1.0),
        },
        ccw(p(0.0, 1.0), p(0.0, -1.0), p(0.0, 0.0)),
    ];
    assert_mass(&profile(edges, vec![]), 4.0 + PI, 1.0, 0.0);
}

#[test]
fn rejects_too_many_edges_before_anything_else() {
    let edges = vec![
        Edge::Line {
            start: p(0.0, 0.0),
            end: p(f64::NAN, 0.0),
        };
        MAX_PROFILE_EDGES + 1
    ];
    assert_eq!(Profile::new(edges, vec![]), Err(ProfileError::TooManyEdges));
}

#[test]
fn rejects_empty_loops() {
    assert_eq!(
        Profile::new(vec![], vec![]),
        Err(ProfileError::EmptyLoop(LoopId::Outer))
    );
    assert_eq!(
        Profile::new(rectangle(0.0, 0.0, 1.0, 1.0), vec![vec![]]),
        Err(ProfileError::EmptyLoop(LoopId::Hole(0)))
    );
}

#[test]
fn rejects_coordinates_that_are_not_finite_or_out_of_range() {
    let mut edges = rectangle(0.0, 0.0, 1.0, 1.0);
    edges[2] = ccw(p(1.0, 1.0), p(0.0, 1.0), p(f64::INFINITY, 0.0));
    assert_eq!(
        Profile::new(edges, vec![]),
        Err(ProfileError::NonFinite {
            loop_id: LoopId::Outer,
            edge: 2
        })
    );
    for far in [2.0 * MODEL_EXTENT, -2.0 * MODEL_EXTENT, 1e-35] {
        let hole = rectangle(0.25, 0.25, 0.5, far);
        assert_eq!(
            Profile::new(rectangle(0.0, 0.0, 1.0, 1.0), vec![hole]),
            Err(ProfileError::OutOfRange {
                loop_id: LoopId::Hole(0),
                edge: 1
            })
        );
    }
}

#[test]
fn rejects_open_loops() {
    let mut edges = rectangle(0.0, 0.0, 1.0, 1.0);
    edges[1] = Edge::Line {
        start: p(1.0, 0.0),
        end: p(1.0, 0.999),
    };
    assert_eq!(
        Profile::new(edges, vec![]),
        Err(ProfileError::Open {
            loop_id: LoopId::Outer,
            edge: 1
        })
    );
    let half = vec![ccw(p(1.0, 0.0), p(-1.0, 0.0), p(0.0, 0.0))];
    assert_eq!(
        Profile::new(half, vec![]),
        Err(ProfileError::Open {
            loop_id: LoopId::Outer,
            edge: 0
        })
    );
}

#[test]
fn rejects_degenerate_edges() {
    let mut edges = rectangle(0.0, 0.0, 1.0, 1.0);
    edges.insert(
        1,
        Edge::Line {
            start: p(1.0, 0.0),
            end: p(1.0, 0.0),
        },
    );
    let zero_line = ProfileError::ZeroLengthLine {
        loop_id: LoopId::Outer,
        edge: 1,
    };
    assert_eq!(Profile::new(edges, vec![]), Err(zero_line));

    let line_back = |from: Point2, to: Point2| Edge::Line {
        start: from,
        end: to,
    };
    let lopsided = vec![
        ccw(p(1.0, 0.0), p(-1.1, 0.0), p(0.0, 0.0)),
        line_back(p(-1.1, 0.0), p(1.0, 0.0)),
    ];
    let mismatch = ProfileError::ArcRadiusMismatch {
        loop_id: LoopId::Outer,
        edge: 0,
    };
    assert_eq!(Profile::new(lopsided, vec![]), Err(mismatch));

    let degenerate = ProfileError::DegenerateArc {
        loop_id: LoopId::Outer,
        edge: 0,
    };
    let tiny = vec![
        ccw(p(1e-7, 0.0), p(-1e-7, 0.0), p(0.0, 0.0)),
        line_back(p(-1e-7, 0.0), p(1e-7, 0.0)),
    ];
    assert_eq!(Profile::new(tiny, vec![]), Err(degenerate));
    // Distinct ends on the same ray from the centre: no angle to sweep.
    let flat = vec![
        ccw(p(1.0, 0.0), p(1.000_000_5, 0.0), p(0.0, 0.0)),
        line_back(p(1.000_000_5, 0.0), p(1.0, 0.0)),
    ];
    assert_eq!(Profile::new(flat.clone(), vec![]), Err(degenerate));
    let clockwise_flat = vec![
        Edge::Arc {
            start: p(1.0, 0.0),
            end: p(1.000_000_5, 0.0),
            centre: p(0.0, 0.0),
            direction: ArcDirection::Clockwise,
        },
        flat[1],
    ];
    assert_eq!(Profile::new(clockwise_flat, vec![]), Err(degenerate));
}

#[test]
fn rejects_self_intersecting_loops() {
    let bow_tie = polygon(&[(0.0, 0.0), (1.0, 1.0), (1.0, 0.0), (0.0, 1.0)]);
    assert_eq!(
        Profile::new(bow_tie, vec![]),
        Err(ProfileError::SelfIntersecting {
            loop_id: LoopId::Outer,
            first: 0,
            second: 2
        })
    );
    let back_and_forth = polygon(&[(0.0, 0.0), (1.0, 0.0)]);
    assert_eq!(
        Profile::new(back_and_forth, vec![]),
        Err(ProfileError::SelfIntersecting {
            loop_id: LoopId::Outer,
            first: 0,
            second: 1
        })
    );
    // A triangle hanging off a full circle at one point.
    let s = p(1.0, 0.0);
    let mut pinched = circle(0.0, 0.0, 1.0);
    pinched.extend(polygon(&[(1.0, 0.0), (2.0, 0.0), (2.0, 1.0)]));
    assert_eq!(pinched[1].start(), s);
    assert_eq!(
        Profile::new(pinched, vec![]),
        Err(ProfileError::SelfIntersecting {
            loop_id: LoopId::Outer,
            first: 0,
            second: 1
        })
    );
    // A figure eight of two full circles touching at their common start.
    let mut eight = circle(0.0, 0.0, 1.0);
    eight.push(ccw(s, s, p(2.0, 0.0)));
    assert_eq!(
        Profile::new(eight, vec![]),
        Err(ProfileError::SelfIntersecting {
            loop_id: LoopId::Outer,
            first: 0,
            second: 1
        })
    );
    // Half a circle, then back along the same half: no area at all.
    let (west, o) = (p(-1.0, 0.0), p(0.0, 0.0));
    let retraced = vec![
        ccw(s, west, o),
        Edge::Arc {
            start: west,
            end: s,
            centre: o,
            direction: ArcDirection::Clockwise,
        },
    ];
    assert_eq!(
        Profile::new(retraced, vec![]),
        Err(ProfileError::SelfIntersecting {
            loop_id: LoopId::Outer,
            first: 0,
            second: 1
        })
    );
}

#[test]
fn rejects_arcs_closer_than_the_tolerance() {
    let outer = rectangle(0.0, 0.0, 4.0, 4.0);
    let near = circle(2.0, 2.0, 2.0 - 5e-7);
    assert_eq!(
        Profile::new(outer.clone(), vec![near]),
        Err(ProfileError::LoopsIntersect {
            first: LoopId::Outer,
            second: LoopId::Hole(0)
        })
    );
    let apart = circle(2.0, 2.0, 2.0 - 2e-6);
    assert!(Profile::new(outer, vec![apart]).is_ok());
}

#[test]
fn rejects_holes_that_cross_lie_outside_or_nest() {
    let outer = || rectangle(0.0, 0.0, 10.0, 10.0);
    let crossing = rectangle(8.0, 8.0, 12.0, 9.0);
    assert_eq!(
        Profile::new(outer(), vec![crossing]),
        Err(ProfileError::LoopsIntersect {
            first: LoopId::Outer,
            second: LoopId::Hole(0)
        })
    );
    let holes = vec![
        rectangle(1.0, 1.0, 2.0, 2.0),
        rectangle(3.0, 3.0, 4.0, 4.0),
        rectangle(3.5, 1.0, 4.0, 2.0),
    ];
    assert!(Profile::new(outer(), holes.clone()).is_ok());
    assert_eq!(
        Profile::new(
            outer(),
            vec![holes[0].clone(), holes[1].clone(), holes[1].clone()]
        ),
        Err(ProfileError::LoopsIntersect {
            first: LoopId::Hole(1),
            second: LoopId::Hole(2)
        })
    );
    let outside = rectangle(11.0, 1.0, 12.0, 2.0);
    assert_eq!(
        Profile::new(outer(), vec![holes[0].clone(), outside]),
        Err(ProfileError::HoleOutsideOuter(1))
    );
    let nested = vec![rectangle(1.0, 1.0, 5.0, 5.0), circle(3.0, 3.0, 1.0)];
    assert_eq!(
        Profile::new(outer(), nested),
        Err(ProfileError::NestedHoles { outer: 0, inner: 1 })
    );
}

#[test]
fn errors_name_the_loop_and_edges_but_no_coordinates() {
    let message = |e: ProfileError| e.to_string();
    assert_eq!(
        message(ProfileError::TooManyEdges),
        "the profile has more than 4096 edges"
    );
    assert_eq!(
        message(ProfileError::EmptyLoop(LoopId::Hole(2))),
        "hole 2 has no edges"
    );
    assert_eq!(
        message(ProfileError::Open {
            loop_id: LoopId::Outer,
            edge: 3
        }),
        "the outer loop is open: edge 3 does not end where the next edge starts"
    );
    let edge_errors = [
        ProfileError::NonFinite {
            loop_id: LoopId::Outer,
            edge: 0,
        },
        ProfileError::OutOfRange {
            loop_id: LoopId::Outer,
            edge: 0,
        },
        ProfileError::ZeroLengthLine {
            loop_id: LoopId::Outer,
            edge: 0,
        },
        ProfileError::ArcRadiusMismatch {
            loop_id: LoopId::Outer,
            edge: 0,
        },
        ProfileError::DegenerateArc {
            loop_id: LoopId::Outer,
            edge: 0,
        },
    ];
    for error in edge_errors {
        assert!(message(error).starts_with("edge 0 of the outer loop "));
    }
    assert_eq!(
        message(ProfileError::SelfIntersecting {
            loop_id: LoopId::Hole(0),
            first: 1,
            second: 4
        }),
        "edges 1 and 4 of hole 0 intersect"
    );
    assert_eq!(
        message(ProfileError::LoopsIntersect {
            first: LoopId::Outer,
            second: LoopId::Hole(1)
        }),
        "the outer loop and hole 1 intersect"
    );
    assert_eq!(
        message(ProfileError::HoleOutsideOuter(0)),
        "hole 0 is outside the outer loop"
    );
    assert_eq!(
        message(ProfileError::NestedHoles { outer: 0, inner: 1 }),
        "hole 1 is inside hole 0"
    );
}
