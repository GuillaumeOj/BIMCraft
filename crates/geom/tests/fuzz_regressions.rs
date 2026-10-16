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

/// `fuzz/seeds/extruded_boolean/needle-on-a-lens-corner-e` to `…-g`: the needle of
/// `a_needle_on_a_corner_splits_the_edges_there_alike` on the corner of larger lenses. Each cut
/// leaves a triangle a few micrometres wide that was classified from a point inside it, a
/// micrometre or so from the other solid's faces, and came out on the wrong side of them.
#[test]
fn a_thin_piece_takes_its_side_from_its_edges() {
    for (lens, exponent, direction, needle_direction) in [
        // `…-e` (`crash-546eb2a8…`): a triangle 2.5 to 5 µm wide on a side of the needle, which
        // a face of the prism cuts along two of its edges with the triangle outside; put inside,
        // it left the union with an open edge.
        (
            [(28, -1, 28), (-95, 44, 122), (0, 0, 0)],
            -4,
            (42, -1, 28),
            (90, 45, 90),
        ),
        // `…-f` (the input `scripts/check-all.sh fuzz` found after the shared noding): a
        // triangle 2 µm wide in a face of the prism, beside a piece 40 µm long inside the needle
        // across an edge no face of the needle holds; put outside, it made an edge of the union
        // non-manifold.
        (
            [(71, -95, 165), (35, 122, 37), (0, 0, 0)],
            -6,
            (91, -1, 28),
            (90, 45, 56),
        ),
        // `…-g` (the input CI found on #270): a triangle 2 to 4 µm wide on a side of the
        // needle, which a face of the prism cuts along its longest edge with the triangle
        // inside; put outside, it made an edge of the union non-manifold.
        (
            [(64, -95, 165), (35, 122, 37), (0, 0, 0)],
            -6,
            (91, -1, 28),
            (90, 45, 55),
        ),
    ] {
        let prism = raw_prism(&lens, exponent, direction).unwrap();
        let needle = raw_prism(&[(0, -1, 255)], -18, needle_direction).unwrap();
        assert_eq!(check_booleans(&prism, &needle), Ok(()));
    }
}

/// A lens of the needle family, its exponent and direction, then the needle's start, exponent
/// and direction, as in `a_needle_on_a_corner_splits_the_edges_there_alike`.
type NeedleCase = (
    [(i8, i8, u8); 3],
    i32,
    (i8, i8, i8),
    (i8, i8, u8),
    i32,
    (i8, i8, i8),
);

/// Checks every operation of each needle on its lens, as the fuzz target does.
fn check_needles(cases: &[NeedleCase]) {
    for &(lens, exponent, direction, needle, needle_exponent, needle_direction) in cases {
        let prism = raw_prism(&lens, exponent, direction);
        let needle = raw_prism(&[needle], needle_exponent, needle_direction);
        let checked = prism.zip(needle).map(|(a, b)| check_booleans(&a, &b));
        assert_eq!(checked, Some(Ok(())));
    }
}

/// `fuzz/seeds/extruded_boolean/needle-on-a-lens-corner-h` to `…-o`: needles on lens corners
/// again. Lines were split at every vertex within the tolerance of them, though only a vertex
/// made on a line lies on it: one up to the tolerance off it bent the line there, after the
/// crossings had been decided on the straight line, and the cuts near the corner went missing.
/// A line is now split only at the vertices made on it, an edge only at the points made on that
/// edge.
#[test]
fn a_line_is_split_only_at_the_vertices_made_on_it() {
    check_needles(&[
        // `…-h` (`ci-pr257-needle`, the input CI found on #257): the union had a degenerate
        // ring.
        (
            [(35, 41, 5), (-95, 65, 122), (0, 0, 0)],
            -6,
            (42, -1, 28),
            (0, -1, 255),
            -18,
            (90, 45, 54),
        ),
        // `…-i` (`crash-41d06295…`): the union had a non-manifold edge.
        (
            [(65, -93, 41), (49, -1, 5), (0, 0, 0)],
            -6,
            (42, 48, 28),
            (0, -1, 255),
            -18,
            (90, -1, 55),
        ),
        // `…-j` (`crash-7218ba65c72b2f1dff605043c880c302c20bbc3c`): the union had a non-manifold edge.
        (
            [(65, -95, 165), (35, 0, 255), (0, 0, 122)],
            -6,
            (42, -1, 28),
            (0, -1, 255),
            -18,
            (90, 45, 54),
        ),
        // `…-k` (`crash-8bd71d27…`): the union had an open edge.
        (
            [(-63, -95, 165), (35, 122, 1), (0, 0, 1)],
            -6,
            (42, 0, 28),
            (-1, -1, 255),
            -18,
            (76, -1, 52),
        ),
        // `…-l` (`crash-b6fd7d0c…`): a needle four times wider; the union had a non-manifold
        // edge.
        (
            [(65, -95, 165), (31, 126, 254), (0, 0, 0)],
            -6,
            (42, -1, 28),
            (0, -1, 255),
            -16,
            (74, 45, 53),
        ),
        // `…-m` (`old-46d30597`): the union had a non-manifold edge.
        (
            [(65, -95, 165), (35, 122, 255), (0, 0, 0)],
            -6,
            (42, -1, 28),
            (0, -1, 255),
            -18,
            (90, 45, 51),
        ),
        // `…-n` (`old-cdc324fc`): the intersection had an open edge.
        (
            [(65, -95, 28), (35, 122, 255), (0, 0, 0)],
            -6,
            (42, -1, 28),
            (0, -1, 255),
            -18,
            (-1, 45, 55),
        ),
        // `…-o` (`old-fc2e4f67`): the difference had a non-manifold edge.
        (
            [(64, -95, 122), (-1, 35, 5), (0, 0, 0)],
            -6,
            (42, -1, 28),
            (0, -1, 255),
            -16,
            (90, 28, 55),
        ),
    ]);
}

/// `fuzz/seeds/extruded_boolean/half-lens-across-a-lens` (`ci-pr268-half-lens`, the input CI
/// found on #268): two lenses a quarter of the grid size, slanted, an arc of one ending within
/// micrometres of an edge of the other. The edge was split at a vertex 1 µm off it, made where
/// the two planes along it meet a third, and the difference had a non-manifold edge.
#[test]
fn an_edge_is_not_bent_to_a_vertex_beside_it() {
    let a = raw_prism(&[(50, 41, 5), (65, -95, 255)], -2, (28, 90, 84)).unwrap();
    let b = raw_prism(&[(8, -95, 255), (-1, -1, 41)], -2, (90, 45, 39)).unwrap();
    assert_eq!(check_booleans(&a, &b), Ok(()));
}

/// `fuzz/seeds/extruded_boolean/needle-on-a-lens-corner-p` (`crash-556988cc…`): a triangle a
/// micrometre or two wide lies in a face of each solid at once, all its vertices on both planes.
/// Each copy was classified inside the other solid and both were dropped, leaving the union
/// with a non-manifold edge. Pieces of the two solids bounded by the same vertices lie on each
/// other.
#[test]
fn pieces_bounded_alike_in_both_solids_lie_on_each_other() {
    check_needles(&[(
        [(65, -91, 122), (41, 5, 35), (0, 0, 0)],
        -6,
        (81, -1, 28),
        (0, -1, 255),
        -18,
        (90, 45, 48),
    )]);
}

/// `fuzz/seeds/extruded_boolean/needle-on-a-lens-corner-q` to `…-t`: two cuts of a face were
/// tested for crossing as straight lines, then split at vertices a tolerance off them; the
/// pieces then crossed with no vertex between them, the face's pieces did not close, and the
/// union had an open edge. Crossings are now decided on the pieces as split.
#[test]
fn cuts_cross_where_their_pieces_do() {
    check_needles(&[
        // `…-q` (`crash-c0c3507c…`).
        (
            [(65, 33, 167), (35, 122, 254), (0, 0, 0)],
            -6,
            (42, -1, 28),
            (0, -1, 255),
            -18,
            (90, 45, 52),
        ),
        // `…-r` (`crash-ff2d6e95…`).
        (
            [(65, 33, 38), (35, 122, 191), (0, 0, 0)],
            -6,
            (10, -1, 28),
            (0, -1, 255),
            -18,
            (90, 45, 52),
        ),
        // `…-s` (`old-4b735e4c`).
        (
            [(65, -31, 229), (35, 122, 255), (0, 0, 0)],
            -6,
            (42, 0, 28),
            (-1, -1, 255),
            -18,
            (90, 45, 54),
        ),
        // `…-t` (`old-abce4d70`).
        (
            [(64, 33, 165), (35, 123, 255), (0, 0, 0)],
            -6,
            (42, -1, 28),
            (0, -1, 255),
            -18,
            (90, 52, 56),
        ),
    ]);
}

/// `fuzz/seeds/extruded_boolean/needle-on-a-lens-corner-u` to `…-w`: where two planes meet,
/// a stretch of their line was kept if its middle lay inside both faces or within the
/// tolerance of their boundaries; near the corner, a stretch a few micrometres long lying beyond
/// a face was kept, or one inside dropped, and the union had a non-manifold edge. The stretches
/// inside both faces are now counted exactly from where their boundaries cross the line.
#[test]
fn a_cut_runs_where_its_line_lies_in_both_faces() {
    check_needles(&[
        // `…-u` (`crash-0ccbe827…`).
        (
            [(65, -95, 245), (35, 0, 41), (0, 0, 122)],
            -6,
            (42, -4, 28),
            (1, -1, 255),
            -18,
            (82, -1, 52),
        ),
        // `…-v` (`crash-e1fd6331…`).
        (
            [(65, -95, 165), (35, 122, 254), (0, 0, 0)],
            -6,
            (58, -1, 28),
            (0, -1, 255),
            -18,
            (90, 45, 52),
        ),
        // `…-w` (the input `scripts/check-all.sh fuzz` found after stage 2).
        (
            [(71, -95, 165), (35, 122, 37), (0, 0, 0)],
            -6,
            (91, -1, 28),
            (0, -1, 255),
            -18,
            (90, 57, 56),
        ),
    ]);
}

/// `fuzz/seeds/extruded_boolean/needle-on-a-lens-corner-x` (the input CI found on #271): the
/// needle of `…-g` on a prism slanted along x and z only. Stage 2's noding near the corner left
/// the union with a non-manifold edge; it closes once lines are split at the vertices made on
/// them and their pieces are crossed and kept exactly.
#[test]
fn a_needle_on_a_lens_slanted_in_its_plane_closes() {
    check_needles(&[(
        [(64, -95, 165), (35, 122, 37), (0, 0, 0)],
        -6,
        (28, 0, 28),
        (0, -1, 255),
        -18,
        (90, 45, 56),
    )]);
}
