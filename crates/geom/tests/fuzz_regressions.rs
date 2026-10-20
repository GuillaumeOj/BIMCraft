//! Inputs the fuzz targets of `fuzz/` once broke (ENG-02), decoded into plain tests. Each one
//! names the seed file that keeps replaying it in the fuzz run.

use geom::{
    ArcDirection, BooleanError, Edge, LoopId, Point2, Profile, ProfileError, Solid, Vector3,
};

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

/// `fuzz/seeds/extruded_boolean/micro-triangle-beside-an-edge`: two slanted prisms over arcs
/// turned into chords, a quarter of the grid size. Near (3.6, 0.24, 0) their cuts leave three
/// vertices a few micrometres apart, one of them 2.4 µm beside an edge: a triangle a few
/// tolerances wide whose sides were not all shared, so every operation failed with an open edge.
#[test]
fn a_triangle_a_few_tolerances_wide_beside_an_edge_closes() {
    let prism = |edges, (x, y, z): (f64, f64, f64)| {
        let profile = Profile::new(edges, vec![]).unwrap();
        Solid::extrude(&profile, Vector3::new(x / 4.0, y / 4.0, z / 4.0)).unwrap()
    };
    let (a0, a1) = (grid(50.0, 41.0, -2), grid(65.0, -95.0, -2));
    let a = prism(
        vec![
            Edge::Line { start: a0, end: a1 },
            arc(a1, a0, 3.875, ArcDirection::CounterClockwise),
        ],
        (90.0, 90.0, 84.0),
    );
    let (b0, b1) = (grid(45.0, 41.0, -2), grid(-1.0, -1.0, -2));
    let b = prism(
        vec![
            arc(b0, b1, 0.125, ArcDirection::Clockwise),
            arc(b1, b0, 3.875, ArcDirection::CounterClockwise),
        ],
        (90.0, 45.0, 57.0),
    );
    let (va, vb) = (a.volume(), b.volume());
    let slack = 1e-6 * (va + vb);
    let union = a.union(&b).unwrap().unwrap();
    assert_eq!(union.validate(), Ok(()));
    assert!(union.volume() <= va + vb + slack && union.volume() >= va.max(vb) - slack);
    let cut = a.difference(&b).unwrap().unwrap();
    assert_eq!(cut.validate(), Ok(()));
    assert!(cut.volume() <= va + slack && cut.volume() >= va - vb - slack);
    let shared = a.intersection(&b).unwrap().unwrap();
    assert_eq!(shared.validate(), Ok(()));
    assert!(shared.volume() <= va.min(vb) + slack);
    // Inclusion–exclusion: the three results account for the same volume.
    assert!((union.volume() - (cut.volume() + vb)).abs() <= slack);
    assert!((va - (cut.volume() + shared.volume())).abs() <= slack);
}

/// A loop decoded as the `extruded_boolean` fuzz target decodes one (`edge_loop` in
/// `fuzz/src/lib.rs`, which these tests cannot depend on; keep the two alike): points in quarter metres
/// scaled by `2^exponent`, each edge a line for a kind below 128, otherwise an arc whose centre
/// lies `kind & 0x3f` minus 32 eighths of the chord away, counterclockwise when `kind & 0x40` is
/// set.
fn raw_loop(raw: &[(i8, i8, u8)], exponent: i32) -> Vec<Edge> {
    let point = |(x, y, _): (i8, i8, u8)| grid(f64::from(x), f64::from(y), exponent);
    (0..raw.len())
        .map(|i| {
            let (start, end) = (point(raw[i]), point(raw[(i + 1) % raw.len()]));
            let kind = raw[i].2;
            if kind < 128 {
                return Edge::Line { start, end };
            }
            let offset = (f64::from(kind & 0x3f) - 32.0) / 8.0;
            let direction = if kind & 0x40 == 0 {
                ArcDirection::Clockwise
            } else {
                ArcDirection::CounterClockwise
            };
            if start == end {
                // A full circle has no chord: its centre goes along x instead.
                let centre = Point2::new(start.x + offset * libm::ldexp(1.0, exponent), start.y);
                return Edge::Arc {
                    start,
                    end,
                    centre,
                    direction,
                };
            }
            arc(start, end, offset, direction)
        })
        .collect()
}

/// The prism over a decoded loop, extruded along a direction in quarter metres.
fn raw_prism(raw: &[(i8, i8, u8)], exponent: i32, (x, y, z): (i8, i8, i8)) -> Option<Solid> {
    let profile = Profile::new(raw_loop(raw, exponent), vec![]).ok()?;
    let direction = Vector3::new(f64::from(x) / 4.0, f64::from(y) / 4.0, f64::from(z) / 4.0);
    Solid::extrude(&profile, direction).ok()
}

/// What the `extruded_boolean` fuzz target checks: every operation succeeds and gives valid
/// solids within the volume bounds of its operands. An operation that fails returns its error.
fn check_booleans(a: &Solid, b: &Solid) -> Result<(), BooleanError> {
    let (va, vb) = (a.volume(), b.volume());
    let slack = 1e-6 * (va + vb).max(1.0);
    let volume = |solid: Option<Solid>| {
        solid.map_or(0.0, |solid| {
            assert_eq!(solid.validate(), Ok(()));
            solid.volume()
        })
    };
    let union = a.union(b)?;
    assert!(union.is_some(), "a union is never empty");
    let union = volume(union);
    assert!(union <= va + vb + slack && union >= va.max(vb) - slack);
    let cut = volume(a.difference(b)?);
    assert!(cut <= va + slack && cut >= va - vb - slack);
    let shared = volume(a.intersection(b)?);
    assert!(shared <= va.min(vb) + slack);
    Ok(())
}

/// `fuzz/seeds/extruded_boolean/sliver-along-a-shared-edge` (the input CI found on #255): the
/// profiles of `a_triangle_a_few_tolerances_wide_beside_an_edge_closes`, extruded in other
/// directions. Each face split the lines it shares with its neighbours on its own, at the
/// vertices near them in that face only, so two faces along one line could split it at
/// different vertices; their pieces' edges did not match and the union was left with an open
/// edge.
#[test]
fn faces_along_a_line_split_it_alike() {
    let a = raw_prism(&[(50, 41, 5), (65, -95, 255)], -2, (-90, -101, 84)).unwrap();
    let b = raw_prism(&[(45, 41, 161), (-1, -1, 255)], -2, (90, 60, 48)).unwrap();
    assert_eq!(check_booleans(&a, &b), Ok(()));
}

/// `fuzz/seeds/extruded_boolean/needle-on-a-lens-corner-*`: a slanted needle 30 µm wide, a
/// circle turned into chords, standing on a corner of a prism over a lens. Its vertices lie
/// within a few tolerances of the prism's edges at that corner, and the faces along those edges
/// split them at different vertices of the needle, as above: a ring degenerated or an edge was
/// left open.
#[test]
fn a_needle_on_a_corner_splits_the_edges_there_alike() {
    for (lens, exponent, direction, needle, needle_direction) in [
        // `…-a` (`crash-4b9eadbb…`): the union had a degenerate ring.
        (
            [(65, -95, 38), (35, 122, 255), (0, 0, 0)],
            -21,
            (42, -1, 28),
            (0, -1, 255),
            (90, 45, 56),
        ),
        // `…-b` (`crash-6104a9fe…`): the intersection had an open edge.
        (
            [(64, -95, 165), (35, 122, 37), (0, 0, 0)],
            -6,
            (91, -1, 28),
            (0, -1, 255),
            (90, 45, 56),
        ),
        // `…-c` (`old-12c7eeca`): a needle a little narrower, starting beside the corner; the
        // union had a degenerate ring.
        (
            [(69, -95, 229), (59, 122, 255), (0, 0, 0)],
            -6,
            (42, -1, 28),
            (-1, 0, 253),
            (82, 45, 55),
        ),
        // `…-d` (`old-eede88c0`): the needle starts 1.9 µm lower; the union had a degenerate
        // ring.
        (
            [(69, -95, 229), (59, 122, 255), (0, 0, 0)],
            -6,
            (42, -1, 28),
            (0, -3, 255),
            (82, 45, 52),
        ),
    ] {
        let prism = raw_prism(&lens, exponent, direction).unwrap();
        let needle = raw_prism(&[needle], -18, needle_direction).unwrap();
        assert_eq!(check_booleans(&prism, &needle), Ok(()));
    }
}
