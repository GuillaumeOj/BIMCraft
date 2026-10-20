//! Inputs the fuzz targets of `fuzz/` once broke (ENG-02), decoded into plain tests. Each one
//! names the seed file that keeps replaying it in the fuzz run.

use geom::{
    ArcDirection, BooleanError, Containment, Edge, LINEAR_TOLERANCE, LoopId, MERGE_CHAIN_WIDTH,
    Point2, Point3, Profile, ProfileError, Solid, Vector3,
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
    assert_eq!(check_booleans(&a, &b), Ok(()));
    let (va, vb) = (a.volume(), b.volume());
    let slack = volume_slack(&a, &b);
    let volume = |solid: Result<Option<Solid>, BooleanError>| {
        solid.ok().flatten().map_or(0.0, |solid| solid.volume())
    };
    let (union, cut, shared) = (
        volume(a.union(&b)),
        volume(a.difference(&b)),
        volume(a.intersection(&b)),
    );
    // Inclusion–exclusion: the three results account for the same volume.
    assert!((union - (cut + vb)).abs() <= slack);
    assert!((va - (cut + shared)).abs() <= slack);
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

/// The total area of a solid's faces, as `surface_area` in `fuzz/src/lib.rs` works it out (keep
/// the two alike).
fn surface_area(solid: &Solid) -> f64 {
    let points = solid.vertices();
    solid
        .to_faces()
        .iter()
        .map(|face| {
            let mut area = [0.0_f64; 3];
            for ring in std::iter::once(&face.outer).chain(&face.holes) {
                for (i, &start) in ring.iter().enumerate() {
                    let (p, q) = (points[start], points[ring[(i + 1) % ring.len()]]);
                    area[0] += (p.y - q.y) * (p.z + q.z);
                    area[1] += (p.z - q.z) * (p.x + q.x);
                    area[2] += (p.x - q.x) * (p.y + q.y);
                }
            }
            area.iter().map(|c| c * c).sum::<f64>().sqrt() / 2.0
        })
        .sum()
}

/// The farthest the merge pass may move a vertex, as `MERGE_REACH` in `fuzz/src/lib.rs` (keep
/// the two alike): across a group of merged vertices at most `MERGE_CHAIN_WIDTH` wide along each
/// axis (ADR 0001, merge pass step 2).
const MERGE_REACH: f64 = MERGE_CHAIN_WIDTH * 1.732_050_807_568_877_2;

/// How far a boolean's volume may lie from its bounds, as `volume_slack` in `fuzz/src/lib.rs`
/// (keep the two alike): the tolerance times the operands' area.
fn volume_slack(a: &Solid, b: &Solid) -> f64 {
    LINEAR_TOLERANCE * (surface_area(a) + surface_area(b))
}

/// Whether every point lies with respect to `solid` as `allowed` says, or within
/// [`MERGE_REACH`] of its boundary, as `all_located` in `fuzz/src/lib.rs` (keep the two alike).
fn all_located(points: &[Point3], solid: &Solid, allowed: impl Fn(Containment) -> bool) -> bool {
    (points.iter()).all(|&p| allowed(solid.locate(p)) || solid.boundary_distance(p) <= MERGE_REACH)
}

/// What the `extruded_boolean` fuzz target checks of each operation, in the order union,
/// difference, intersection (keep the two alike): it gives a valid solid within the volume
/// bounds of its operands, give or take the tolerance times their area, which is what the merge
/// pass may move (ADR 0001), with its vertices where the operation puts them: every operand
/// vertex inside or on the union, every vertex of the difference inside or on the first operand
/// and not inside the second, every vertex of the intersection inside or on both, "on" meaning
/// within the reach of a vertex merge ([`MERGE_REACH`]). An operation that fails gives its
/// error; the fuzz target accepts only [`BooleanError::FeaturesTooClose`].
fn check_each(a: &Solid, b: &Solid) -> [Result<(), BooleanError>; 3] {
    let (va, vb) = (a.volume(), b.volume());
    let slack = volume_slack(a, b);
    let not_outside = |c| c != Containment::Outside;
    let union = a.union(b).map(|union| {
        assert!(union.is_some(), "a union is never empty");
        let volume = union.map_or(0.0, |union| {
            assert_eq!(union.validate(), Ok(()));
            assert!(all_located(a.vertices(), &union, not_outside));
            assert!(all_located(b.vertices(), &union, not_outside));
            union.volume()
        });
        assert!(volume <= va + vb + slack && volume >= va.max(vb) - slack);
    });
    let cut = a.difference(b).map(|cut| {
        let volume = cut.map_or(0.0, |cut| {
            assert_eq!(cut.validate(), Ok(()));
            assert!(all_located(cut.vertices(), a, not_outside));
            assert!(all_located(cut.vertices(), b, |c| c != Containment::Inside));
            cut.volume()
        });
        assert!(volume <= va + slack && volume >= va - vb - slack);
    });
    let shared = a.intersection(b).map(|shared| {
        let volume = shared.map_or(0.0, |shared| {
            assert_eq!(shared.validate(), Ok(()));
            assert!(all_located(shared.vertices(), a, not_outside));
            assert!(all_located(shared.vertices(), b, not_outside));
            shared.volume()
        });
        assert!(volume <= va.min(vb) + slack);
    });
    [union, cut, shared]
}

/// [`check_each`] for operands every operation must succeed on: the first error, if any.
fn check_booleans(a: &Solid, b: &Solid) -> Result<(), BooleanError> {
    check_each(a, b).into_iter().collect()
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
        // ring, then kept a face of the needle's tip 0.82 µm wide, which is now zipped onto
        // the face across its longest edge.
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
        // it left the union with an open edge. Its union is valid, but the corner of the lens
        // merges 0.95 µm onto a vertex of the needle, on a face of about 10 m², and adds
        // 1.1e-6 m³: within the tolerance times the operands' area, not within 1e-6 of their
        // volume.
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

/// Faces narrower than the tolerance, which the solid's checks reject: each face's vertices lie
/// within the tolerance of its longest edge, in order along it, so the merge pass zips it shut
/// onto the face across that edge, which then runs through them. Before, the result kept the
/// sliver and was rejected.
#[test]
fn a_sliver_is_zipped_onto_the_face_across_its_longest_edge() {
    check_needles(&[
        // `fuzz/seeds/extruded_boolean/sliver-zipped-a` (`crash-0384fb71…`): the union.
        (
            [(65, -31, 229), (35, 122, 255), (0, 0, 0)],
            -6,
            (0, 42, 28),
            (0, -1, 255),
            -18,
            (90, 45, 50),
        ),
        // `…-b` (`crash-3f3f6b44…`): the union.
        (
            [(65, -95, 165), (35, 0, 255), (0, 0, 122)],
            -10,
            (-1, -1, 28),
            (0, -1, 255),
            -18,
            (90, 45, 54),
        ),
        // `…-c` (`crash-4d5c925e…`): a lens a hundred times smaller; the union.
        (
            [(65, -93, 165), (31, 126, 254), (0, 0, 0)],
            -18,
            (0, -68, 28),
            (0, -1, 255),
            -16,
            (90, 45, 55),
        ),
        // `…-d` (`crash-f3581a16…`): the union.
        (
            [(65, -95, 165), (122, 0, 35), (-2, 0, 0)],
            -6,
            (42, -1, 28),
            (0, -1, 255),
            -18,
            (90, 45, 54),
        ),
    ]);
    // `…-e` (`mainfix-1b3d348f`): a half disc and a circle a few micrometres across; the
    // difference kept a sliver of the circle's side.
    let a = raw_prism(&[(-1, 41, 65), (-1, -1, 188)], -16, (-1, -68, 42)).unwrap();
    let b = raw_prism(&[(-1, 90, 188)], -17, (-77, -68, 38)).unwrap();
    assert_eq!(check_booleans(&a, &b), Ok(()));
}

/// A face narrower than the tolerance lying back to back with the face across its longest edge,
/// which also runs along another of its edges: zipped onto it, the two cancel, and the vertex
/// they shared goes. Before, the difference kept the sliver and was rejected.
#[test]
fn a_sliver_folded_back_on_a_face_cancels_with_it() {
    check_needles(&[
        // `fuzz/seeds/extruded_boolean/sliver-folded-back-a` (`crash-033923b4…`).
        (
            [(64, 33, 165), (35, 123, 255), (0, 0, 0)],
            -6,
            (42, -1, 28),
            (0, -1, 255),
            -18,
            (26, 52, 57),
        ),
        // `…-b` (`crash-44f51d94…`): a lens as small as the needle.
        (
            [(64, 33, 165), (35, 123, 254), (0, -1, 255)],
            -18,
            (90, 57, 52),
            (0, -1, 255),
            -18,
            (90, 52, 56),
        ),
        // `…-c` (`crash-5b85f1f1…`).
        (
            [(64, 33, 165), (35, 123, 254), (0, -1, 255)],
            -18,
            (90, 57, 54),
            (0, -1, 255),
            -18,
            (90, 52, 54),
        ),
        // `…-d` (`crash-5ce1e111…`).
        (
            [(122, 71, 5), (-27, -95, 35), (0, 0, 0)],
            -6,
            (91, -1, 28),
            (0, -1, 255),
            -18,
            (90, 65, 54),
        ),
    ]);
}

/// `fuzz/seeds/extruded_boolean/vertex-kept-for-a-wide-face` (`crash-36dc90d3…`): a vertex
/// between two edges within the tolerance of a line was removed from both its faces, which left
/// one no wider than the tolerance, and the difference was rejected. A vertex now goes only if
/// its faces still pass the solid's checks without it.
#[test]
fn a_vertex_goes_only_if_its_faces_stay_valid() {
    check_needles(&[(
        [(-63, -95, 165), (35, 122, 1), (0, 0, 1)],
        -6,
        (42, 0, 28),
        (0, 5, 255),
        -18,
        (76, -1, 51),
    )]);
}

/// `fuzz/seeds/extruded_boolean/pinched-face-split-wide` (`crash-154eee8e…`): a merged face
/// passing a vertex twice, a long thin triangle with a small one at its tip, was split along the
/// chord farthest from the other vertices, which ran 12 m down the long side and left a part
/// 0.87 µm wide. The chord now leaves the narrower part widest, as the solid's checks measure
/// width.
#[test]
fn a_pinched_face_is_split_where_both_parts_stay_wide() {
    let a = raw_prism(
        &[(-95, -95, 28), (35, -1, 0), (41, 122, 0)],
        -21,
        (42, -1, 28),
    )
    .unwrap();
    let b = raw_prism(&[(-1, 0, 255)], -18, (90, 0, 90)).unwrap();
    assert_eq!(check_booleans(&a, &b), Ok(()));
}

/// `fuzz/seeds/extruded_boolean/features-too-close-micro-triangle-a` (`crash-2eea7226…`) and
/// `…-b` (`crash-b162b855…`, found by the stage-4 survey): the union leaves a triangle a
/// micrometre or two across where the needle meets the lens's bottom, its apex 0.5 to 1 µm above
/// it: narrower than the tolerance, with an edge no longer than it. Collapsing that edge would
/// cross the faces around it, its other edges are longer than the tolerance, and the triangle
/// does not lie along any of its edges, so no operation removes it and the union fails with the
/// typed error instead of returning an invalid solid; the other operations hold. Before the cut
/// was made on the operands as they are, a vertex merge bent the lens's bottom in the first,
/// and the second's union had a non-manifold edge.
#[test]
fn a_micro_triangle_above_a_face_is_too_close() {
    let cases = [
        (
            raw_prism(
                &[(65, -95, 157), (35, 122, 255), (0, 3, 73)],
                -19,
                (36, -110, 74),
            ),
            raw_prism(&[(0, -1, 255)], -18, (90, 45, 50)),
        ),
        (
            raw_prism(&[(64, 89, 5), (28, -91, 0), (0, 35, 254)], -19, (0, 0, 28)),
            raw_prism(&[(8, -1, 255)], -20, (90, 45, 51)),
        ),
    ];
    for (a, b) in cases {
        assert_eq!(
            check_each(&a.unwrap(), &b.unwrap()),
            [Err(BooleanError::FeaturesTooClose), Ok(()), Ok(())]
        );
    }
}

/// `fuzz/seeds/extruded_boolean/sliver-off-a-face-cut-exactly` (`crash-a39e9827…`, found by the
/// stage-4 survey): the difference left a triangle 2.75 µm long and narrower than the tolerance
/// at the lens corner, one of its vertices 1.08 µm above the lens's bottom, the face across its
/// longest edge, and failed with the typed error, as no operation could remove it. The cut now
/// decides on the operands as they are and merges no vertex before it, and the difference no
/// longer leaves the triangle: every operation holds.
#[test]
fn a_sliver_off_the_plane_of_the_face_across_is_cut_away() {
    let a = raw_prism(&[(35, 41, 161), (67, -1, 122), (0, 0, 0)], -6, (42, -1, 28)).unwrap();
    let b = raw_prism(&[(1, -1, 255)], -18, (90, 45, 54)).unwrap();
    assert_eq!(check_each(&a, &b), [Ok(()), Ok(()), Ok(())]);
}

/// `fuzz/seeds/extruded_boolean/corner-merged-on-a-large-face-a` to `…-f`: needles on lens
/// corners whose results are valid, but whose volume moved by 1e-6 to 4e-6 m³ against operands
/// of about 1 m³: the lens corner merges up to a tolerance onto a vertex of the needle, on faces
/// of square metres, and the volume of a face is measured from the centre of its vertices. The
/// merge pass may move vertices by the tolerance, so a volume within the tolerance times the
/// operands' area of its bounds is right (ADR 0001); the fuzz target checked 1e-6 of their
/// volume only.
#[test]
fn a_corner_merged_on_a_large_face_moves_the_volume_by_its_area() {
    check_needles(&[
        // `…-a` (`crash-10a6d57f…`): the union, 1.0e-6 m³ short.
        (
            [(64, -120, 0), (-95, 65, 122), (0, 0, 0)],
            -6,
            (42, -1, 28),
            (0, -1, 255),
            -18,
            (76, 45, 53),
        ),
        // `…-b` (`crash-8adf1d03…`): the union, 3.0e-6 m³ over.
        (
            [(35, 41, 28), (-95, 44, 122), (0, 0, 0)],
            -6,
            (42, -1, 99),
            (0, -1, 255),
            -18,
            (90, 45, 90),
        ),
        // `…-c` (`crash-9bcdc7c4…`): the difference, 1.2e-6 m³ over.
        (
            [(65, -95, 229), (35, 122, 127), (0, 0, 0)],
            -6,
            (42, -1, 28),
            (-1, 0, 255),
            -18,
            (90, 45, 52),
        ),
        // `…-d` (`crash-2f9ae4ef…`): the union, 1.0e-6 m³ over.
        (
            [(65, -31, 28), (35, 122, 255), (0, 0, 0)],
            -6,
            (-118, 0, 28),
            (-3, -1, 255),
            -18,
            (90, 45, 54),
        ),
        // `…-e` (`crash-e8f929ec…`): the union, 1.6e-6 m³ short.
        (
            [(65, 41, 237), (35, 0, 255), (0, 0, 122)],
            -4,
            (42, 50, 28),
            (0, -1, 255),
            -18,
            (90, 45, 49),
        ),
    ]);
    // `…-f` (`crash-b03f9bc1…`): a lens of two arcs; the union, 4.1e-6 m³ short.
    let a = raw_prism(&[(-87, 122, 255), (0, 0, 0)], -3, (42, -1, 28)).unwrap();
    let b = raw_prism(&[(0, -1, 255)], -18, (90, 45, 42)).unwrap();
    assert_eq!(check_booleans(&a, &b), Ok(()));
}

/// The inputs of [`a_cut_on_the_operands_as_they_are_closes`] that are needles on three-edge
/// lenses, by seed.
const CUT_ON_THE_OPERANDS: [NeedleCase; 23] = [
    // `…-a` (`crash-1d765262…`): the union had a non-manifold edge.
    (
        [(65, -31, 255), (-1, -68, 255), (0, 0, 0)],
        -6,
        (0, 0, 28),
        (-1, -1, 255),
        -18,
        (90, 45, 28),
    ),
    // `…-b` (`crash-1e3cfdaf…`): the union had a non-manifold edge.
    (
        [(65, -95, 255), (122, -91, 35), (0, 0, 0)],
        -6,
        (42, -1, 28),
        (0, -1, 255),
        -18,
        (90, 45, 51),
    ),
    // `…-c` (`crash-1e659169…`): the union and the difference had a degenerate ring.
    (
        [(65, -95, 165), (63, 122, 255), (0, 0, 0)],
        -6,
        (42, -1, 28),
        (0, -1, 255),
        -18,
        (90, -68, 42),
    ),
    // `…-d` (`crash-7abd432747cd7c823ad88d4f9d4b0744ee7e4e6f`): the union had a non-manifold
    // edge.
    (
        [(63, -8, 115), (35, 122, 37), (0, 0, 0)],
        -6,
        (91, -1, 28),
        (0, -1, 255),
        -18,
        (90, 45, 56),
    ),
    // `…-e` (`crash-ef3bd05a…`): every operation had an open or non-manifold edge.
    (
        [(64, 33, 165), (123, 35, 41), (0, 0, 0)],
        -6,
        (42, -1, 28),
        (0, -1, 255),
        -18,
        (90, 52, 57),
    ),
    // `…-f` (`crash-15919eff…`): every operation had an open or non-manifold edge.
    (
        [(106, -95, 28), (35, 123, 37), (0, 0, 0)],
        -6,
        (52, -1, 28),
        (-1, 0, 255),
        -18,
        (90, 45, 56),
    ),
    // `…-g` (`crash-5f688e70…`): a lens a hundred thousand times smaller; the difference had
    // a degenerate ring.
    (
        [(-95, -95, 28), (-84, -1, 0), (41, 122, 0)],
        -21,
        (42, -1, 28),
        (-1, 0, 255),
        -18,
        (90, 0, 90),
    ),
    // `…-h` (`crash-6e0ec3bc…`): every operation had an open or non-manifold edge.
    (
        [(64, 33, 165), (123, 35, 41), (0, 0, 0)],
        -6,
        (42, -1, 28),
        (0, -1, 255),
        -18,
        (59, 50, 53),
    ),
    // `…-i` (`crash-781a2aff…`): every operation had an open edge.
    (
        [(71, -95, 37), (122, -91, 35), (0, 0, 0)],
        -6,
        (91, -1, 21),
        (0, -1, 255),
        -18,
        (0, 45, 56),
    ),
    // `…-j` (`crash-868df505…`): a needle four times wider; the union was too close and the
    // difference and the intersection had an open edge.
    (
        [(65, -95, 165), (121, 0, 35), (0, 0, 254)],
        -6,
        (42, -65, 28),
        (0, -1, 255),
        -16,
        (116, 45, 51),
    ),
    // `…-k` (`crash-930a4c59…`): the union had a non-manifold edge.
    (
        [(42, -1, 99), (-95, 44, 122), (0, 0, 0)],
        -6,
        (42, -1, 99),
        (0, -1, 255),
        -18,
        (-1, 0, 90),
    ),
    // `…-l` (`crash-94cb21e3…`): the difference had a non-manifold edge.
    (
        [(-126, -70, 174), (35, 123, 255), (0, 0, 0)],
        -6,
        (42, -1, 28),
        (0, -1, 255),
        -18,
        (26, 53, 48),
    ),
    // `…-m` (`crash-97f5a439…`): the union had a non-manifold edge, the intersection an open
    // one.
    (
        [(64, 33, 165), (35, -3, 255), (0, 0, 0)],
        -6,
        (42, -1, 28),
        (0, -1, 255),
        -18,
        (26, 52, 57),
    ),
    // `…-n` (`crash-c38347c1…`): the union had a non-manifold edge.
    (
        [(65, -95, 165), (35, 0, 255), (0, 0, 122)],
        -10,
        (-1, -1, 28),
        (0, -1, 255),
        -18,
        (59, 45, 54),
    ),
    // `…-p` (`crash-e25522f6…`): every operation had a non-manifold edge.
    (
        [(64, -91, 35), (5, 41, 33), (0, 0, 0)],
        -10,
        (42, -1, 28),
        (0, -1, 188),
        -18,
        (90, -1, 56),
    ),
    // `…-q` (`crash-c5cb99793b…`): every operation had an open edge, the union one 12.6 m
    // long, a whole face of the lens dropped.
    (
        [(69, 33, 231), (35, 122, 254), (0, 0, 0)],
        -6,
        (42, -1, 28),
        (0, -1, 255),
        -18,
        (90, -81, 56),
    ),
    // `…-r` (`crash-f01740a7…`): the union had a degenerate ring.
    (
        [(65, -95, 165), (35, 58, 254), (0, 0, 0)],
        -6,
        (58, -1, 28),
        (0, -1, 255),
        -18,
        (90, 45, 50),
    ),
    // `…-s` (`crash-f1addc4c…`): the union had a non-manifold edge.
    (
        [(69, 33, 239), (35, -1, 28), (0, 0, 0)],
        -6,
        (42, 40, 28),
        (0, -1, 255),
        -18,
        (94, 45, 56),
    ),
    // `…-t` (`crash-ffa57750…`): the union had a non-manifold edge.
    (
        [(35, 41, 161), (67, -1, 122), (0, 0, 0)],
        -6,
        (58, -1, 28),
        (0, -1, 255),
        -18,
        (90, 13, 54),
    ),
    // `…-u` (`ci-pr275-crash-06fca011`, the input CI found on #275): the difference had a
    // non-manifold edge.
    (
        [(-11, -95, 165), (35, 122, 37), (0, 0, 0)],
        -6,
        (83, -1, 28),
        (0, -1, 255),
        -18,
        (90, 57, 56),
    ),
    // `…-v` (`crash-ffc80604…`, the input `scripts/check-all.sh fuzz` found on stage 4): the
    // union had a non-manifold edge.
    (
        [(65, 92, 122), (41, 5, 35), (0, 0, 0)],
        -6,
        (81, -1, 28),
        (0, -1, 255),
        -18,
        (90, -4, 48),
    ),
    // `…-w` (`stage5-fuzz-crash-c2e99f34`, the input `scripts/check-all.sh fuzz` found on
    // stage 5): the difference and the intersection had an open edge.
    (
        [(-63, -95, 165), (35, 122, 1), (0, 0, 1)],
        -6,
        (0, 42, 28),
        (0, 5, 255),
        -18,
        (76, -1, 51),
    ),
    // `…-x` (`ci-pr276-crash-7ac25ad5`, the input CI found on #276): the union had a
    // non-manifold edge.
    (
        [(65, 33, 38), (35, 122, 197), (0, 0, 0)],
        -6,
        (10, -1, 28),
        (0, -1, 255),
        -18,
        (90, 45, 52),
    ),
];

/// `fuzz/seeds/extruded_boolean/needle-cut-exactly-a` to `…-x`: needles on lens corners that
/// the fuzz runs of stages 3 to 5 and CI found failing, every one at the first pairing of the kept
/// pieces, before the merge pass. Operand vertices were merged within the tolerance before the
/// cut, after the sides of the cut had been decided on their own positions: the lens corner moved
/// up to a micrometre onto a vertex of the needle. A piece classified on one side of a face then
/// lay on it, back to back with it (`…-v`, `crash-ffc80604…`), a sharp corner merged into a fin
/// shared by two faces (`…-b`, `crash-1e3cfdaf…`), or a misread side spread over a whole face of
/// the lens (`…-q`, `crash-c5cb9979…`), and the result had a non-manifold or open edge or a
/// degenerate ring. The cut now decides on the operands as they are, every vertex lying on the
/// planes it was made on; the merge pass collapses what is left within the tolerance.
#[test]
fn a_cut_on_the_operands_as_they_are_closes() {
    check_needles(&CUT_ON_THE_OPERANDS);
    // `…-o` (`crash-855b3ba8a4e598f12b0254158028736285d42a71`): a lens of an arc and a line;
    // the difference and the intersection had an open edge.
    let a = raw_prism(&[(65, -95, 255), (0, 0, 122)], -6, (42, -1, 28)).unwrap();
    let b = raw_prism(&[(0, -1, 255)], -18, (90, 45, 52)).unwrap();
    assert_eq!(check_booleans(&a, &b), Ok(()));
}
