use super::bridge::bridge_holes;
use super::ear::clip;
use super::polygon::{Chain, Failed, is_sliver, triangulate};
use super::search::{MAX_SEARCH_NODES, search};
use crate::{Orientation, Point2, orient2d};

fn points(coordinates: &[(f64, f64)]) -> Vec<Point2> {
    coordinates
        .iter()
        .map(|&(x, y)| Point2::new(x, y))
        .collect()
}

/// Twice the area of a triangle.
fn doubled_area(p: &[Point2], [a, b, c]: [usize; 3]) -> f64 {
    (p[b].x - p[a].x) * (p[c].y - p[a].y) - (p[c].x - p[a].x) * (p[b].y - p[a].y)
}

/// Triangulates and checks the count, that every triangle turns strictly counterclockwise,
/// and that together they cover `area`.
fn triangulates(p: &[Point2], ends: &[usize], area: f64) -> Vec<[usize; 3]> {
    let triangles = triangulate(p, ends).expect("a triangulation");
    let holes = ends.len() - 1;
    assert_eq!(triangles.len(), p.len() + 2 * holes - 2);
    for &[a, b, c] in &triangles {
        assert_eq!(orient2d(p[a], p[b], p[c]), Ok(Orientation::Positive));
    }
    let covered: f64 = triangles.iter().map(|&t| doubled_area(p, t)).sum();
    assert!((covered / 2.0 - area).abs() <= 1e-12 * area);
    triangles
}

/// The 10 m square, then holes, each listed clockwise.
fn square_with(holes: &[&[(f64, f64)]]) -> (Vec<Point2>, Vec<usize>) {
    let mut all = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
    let mut ends = vec![4];
    for hole in holes {
        all.extend_from_slice(hole);
        ends.push(all.len());
    }
    (points(&all), ends)
}

#[test]
fn a_square_gives_two_triangles() {
    let (p, ends) = square_with(&[]);
    triangulates(&p, &ends, 100.0);
}

#[test]
fn vertices_along_straight_runs_never_make_flat_triangles() {
    let p = points(&[
        (0.0, 0.0),
        (1.0, 0.0),
        (2.0, 0.0),
        (3.0, 0.0),
        (3.0, 1.0),
        (3.0, 2.0),
        (1.5, 2.0),
        (0.0, 2.0),
        (0.0, 1.0),
    ]);
    triangulates(&p, &[9], 6.0);
}

#[test]
fn a_node_refused_at_its_first_visit_is_clipped_at_a_later_one() {
    // Found by search: the walk starts at node 0, which is no ear then, and clips it later,
    // once clips elsewhere changed its triangle; ear status is never cached.
    let p = points(&[
        (3.0, 0.0),
        (7.0, 6.0),
        (1.0, 2.0),
        (0.0, 3.0),
        (-2.0, 2.0),
        (-8.0, 3.0),
        (-3.0, 0.0),
        (-2.0, -2.0),
        (-3.0, -7.0),
        (1.0, -8.0),
        (1.0, -2.0),
        (6.0, -2.0),
    ]);
    let area: f64 = (0..p.len())
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        / 2.0;
    let triangles = triangulates(&p, &[p.len()], area);
    let (last, earlier) = triangles.split_last().expect("triangles");
    assert_ne!(
        triangles[0][1], 0,
        "node 0 is not clipped at the first visit"
    );
    assert!(earlier.iter().any(|t| t[1] == 0) && last[1] != 0);
}

#[test]
fn a_hole_is_bridged_and_clipped_around() {
    let hole: &[(f64, f64)] = &[(4.0, 4.0), (4.0, 6.0), (6.0, 6.0), (6.0, 4.0)];
    let (p, ends) = square_with(&[hole]);
    triangulates(&p, &ends, 96.0);
}

#[test]
fn two_holes_bridge_to_the_same_outer_vertex() {
    // Both holes are nearest the corner (10, 10), point 2, so each bridge starts there and the
    // corner ends up with three nodes; clipping must tell its copies apart by their edges.
    let first: &[(f64, f64)] = &[(9.6, 8.4), (9.6, 8.0), (9.2, 8.0), (9.2, 8.4)];
    let second: &[(f64, f64)] = &[(9.0, 9.6), (9.0, 9.0), (8.0, 9.0), (8.0, 9.6)];
    let (p, ends) = square_with(&[first, second]);
    let (mut chain, heads) = Chain::new(&p, &ends).expect("rings");
    let start = bridge_holes(&mut chain, &heads).expect("bridges");
    let corner = chain
        .ring(start)
        .filter(|&node| chain.nodes[node].point == 2)
        .count();
    assert_eq!(corner, 3);
    triangulates(&p, &ends, 100.0 - 0.16 - 0.6);
}

#[test]
fn holes_in_a_row_bridge_through_each_other() {
    // The left hole sees the outer ring only past the right one, or through its bridge.
    let right: &[(f64, f64)] = &[(9.0, 0.5), (7.0, 0.5), (7.0, 1.5), (9.0, 1.5)];
    let left: &[(f64, f64)] = &[(6.0, 0.5), (4.0, 0.5), (4.0, 1.5), (6.0, 1.5)];
    let mut all = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 2.0), (0.0, 2.0)];
    all.extend_from_slice(left);
    all.extend_from_slice(right);
    triangulates(&points(&all), &[4, 8, 12], 20.0 - 4.0);
    // Reversed: the right hole comes first in the input and is still bridged first.
    let mut all = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 2.0), (0.0, 2.0)];
    all.extend_from_slice(right);
    all.extend_from_slice(left);
    triangulates(&points(&all), &[4, 8, 12], 20.0 - 4.0);
}

#[test]
fn malformed_rings_are_refused() {
    let p = points(&[(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)]);
    assert_eq!(triangulate(&p, &[]), Err(Failed));
    assert_eq!(triangulate(&p, &[2]), Err(Failed));
    assert_eq!(triangulate(&p, &[4]), Err(Failed));
}

#[test]
fn coordinates_the_predicates_refuse_fail_the_triangulation() {
    let p = points(&[(0.0, 0.0), (1.0, 0.0), (f64::NAN, 1.0)]);
    assert_eq!(triangulate(&p, &[3]), Err(Failed));
}

#[test]
fn clockwise_outer_rings_have_no_ear() {
    let square = points(&[(0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (1.0, 0.0)]);
    assert_eq!(triangulate(&square, &[4]), Err(Failed));
    let triangle = points(&[(0.0, 0.0), (0.0, 1.0), (1.0, 0.0)]);
    assert_eq!(triangulate(&triangle, &[3]), Err(Failed));
}

#[test]
fn a_hole_around_its_outer_ring_has_no_bridge() {
    let around: &[(f64, f64)] = &[(-1.0, -1.0), (-1.0, 11.0), (11.0, 11.0), (11.0, -1.0)];
    let (p, ends) = square_with(&[around]);
    assert_eq!(triangulate(&p, &ends), Err(Failed));
}

#[test]
fn crossing_rings_are_refused() {
    // A hole across the outer boundary: a bridge may be found, but no ear clipping completes.
    let across: &[(f64, f64)] = &[(8.0, 4.0), (8.0, 6.0), (12.0, 6.0), (12.0, 4.0)];
    let (p, ends) = square_with(&[across]);
    assert_eq!(triangulate(&p, &ends), Err(Failed));
}

/// A face of a boolean result, projected: a sliver 10 m long whose end crosses itself about a
/// micrometre across (`fuzz/seeds/extruded_boolean/tessellated-self-crossing-face`). No
/// triangulation of it turns every triangle counterclockwise.
const SELF_CROSSING: [(f64, f64); 6] = [
    (-0.086_466_713_668_102_59, 0.072_283_623_988_374_04),
    (-1.843_089_541_167_519_7e-6, 1.540_768_530_677_860_7e-6),
    (-2.525_774_161_563_016_7e-7, 1.402_193_800_499_896_7e-6),
    (-9.536_743_164_062_5e-7, 0.0),
    (-0.25, 10.5),
    (-0.336_466_713_668_102_6, 10.572_283_623_988_374),
];

/// The triangles of a fallback: as many as for an exact face, each counterclockwise or a
/// sliver, and at least one sliver.
fn assert_slivers_only_where_needed(p: &[Point2], ends: &[usize], triangles: &[[usize; 3]]) {
    assert_eq!(triangles.len(), p.len() + 2 * (ends.len() - 1) - 2);
    let mut slivers = 0;
    for &[a, b, c] in triangles {
        if orient2d(p[a], p[b], p[c]) != Ok(Orientation::Positive) {
            assert!(is_sliver(p[a], p[b], p[c]));
            slivers += 1;
        }
    }
    assert!(slivers > 0);
}

#[test]
fn a_ring_crossing_itself_within_the_tolerance_is_searched() {
    let p = points(&SELF_CROSSING);
    let (chain, heads) = Chain::new(&p, &[6]).expect("a ring");
    assert_eq!(clip(&mut chain.clone(), heads[0], false), Err(Failed));
    let found = search(&chain, heads[0]).expect("exact predicates");
    assert_eq!(found, triangulate(&p, &[6]).ok());
    assert_slivers_only_where_needed(&p, &[6], &found.expect("a triangulation"));
}

#[test]
fn a_hole_poking_out_of_its_face_within_the_tolerance_is_searched() {
    // `fuzz/seeds/extruded_boolean/tessellated-hole-poking-out-a`: a hole 30 µm wide, one of its
    // corners 0.34 µm outside the outer ring. The search also sees the copies of the bridge.
    let p = points(&[
        (0.253_906_25, 0.160_156_25),
        (0.0, 0.0),
        (10.5, -27.75),
        (10.753_906_25, -27.589_843_75),
        (0.0, -9.536_743_164_062_5e-7),
        (2.217_292_785_644_532e-5, 1.184_787_155_023_46e-5),
        (2.281_940_221_877_870_6e-5, 1.259_490_859_115_317_8e-5),
        (3.056_062_907_497_021_5e-5, -4.062_765_441_640_551_5e-6),
    ]);
    let triangles = triangulate(&p, &[4, 8]).expect("a triangulation");
    assert_slivers_only_where_needed(&p, &[4, 8], &triangles);
}

#[test]
fn a_ring_too_large_to_search_is_clipped_with_slivers() {
    // A half disc whose straight side twists twice into a clockwise loop 2 µm across: covering
    // each loop takes a triangle turning the wrong way, and the arc has more nodes than the
    // search takes.
    let d = 1e-6;
    let mut ring = vec![(0.0, 0.0)];
    for x in [0.3, 0.7] {
        ring.extend([(x + d, d), (x - d, -d), (x + 2.0 * d, 2.0 * d)]);
    }
    ring.push((1.0, 0.0));
    for step in 1..MAX_SEARCH_NODES {
        #[expect(clippy::cast_precision_loss, reason = "a few hundred steps")]
        let angle = std::f64::consts::PI * step as f64 / MAX_SEARCH_NODES as f64;
        ring.push((0.5 + 0.5 * libm::cos(angle), 0.5 * libm::sin(angle)));
    }
    let p = points(&ring);
    let (chain, heads) = Chain::new(&p, &[p.len()]).expect("a ring");
    assert_eq!(clip(&mut chain.clone(), heads[0], false), Err(Failed));
    assert_eq!(search(&chain, heads[0]), Ok(None));
    let triangles = triangulate(&p, &[p.len()]).expect("a triangulation");
    assert_slivers_only_where_needed(&p, &[p.len()], &triangles);
}

#[test]
fn a_ring_without_ears_takes_slivers_when_relaxed() {
    // Clockwise and a micrometre wide: no node is an ear, every triangle is a sliver.
    let p = points(&[(0.0, 0.0), (0.0, 1e-6), (1e-6, 1e-6), (1e-6, 0.0)]);
    let (chain, heads) = Chain::new(&p, &[4]).expect("a ring");
    assert_eq!(clip(&mut chain.clone(), heads[0], false), Err(Failed));
    let triangles = clip(&mut chain.clone(), heads[0], true).expect("slivers");
    assert_eq!(triangles.len(), 2);
    // Wider, the same ring has no sliver either.
    let wide = points(&[(0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (1.0, 0.0)]);
    let (chain, heads) = Chain::new(&wide, &[4]).expect("a ring");
    assert_eq!(clip(&mut chain.clone(), heads[0], true), Err(Failed));
}
