//! Rings and regions in the plane of one face.
//!
//! Edges are directed vertex pairs; the region they bound lies on their left. Points come from a
//! projection that keeps the face's orientation, so outer rings have a positive area.

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;

use super::face::ring_contains;
use super::snap::{find, union};
use crate::Point2;

/// Outside every region.
pub(super) const OUTSIDE: usize = usize::MAX;

/// A region of the plane: one outer ring and its holes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Region {
    pub(super) outer: Vec<usize>,
    pub(super) holes: Vec<Vec<usize>>,
}

/// Rings and the regions they bound.
pub(super) struct Regions {
    pub(super) regions: Vec<Region>,
    /// For each directed edge, the region on its left, or [`OUTSIDE`].
    pub(super) left: BTreeMap<(usize, usize), usize>,
}

/// Follows directed edges into closed rings. At a vertex with several ways out, it takes the one
/// met first turning clockwise from the way back, so each ring bounds a single region and rings
/// that touch at a vertex stay apart. Chains that do not close are dropped, and their edges left
/// free for the rings that need them.
pub(super) fn chain(
    edges: &BTreeSet<(usize, usize)>,
    at: impl Fn(usize) -> Point2,
) -> Vec<Vec<usize>> {
    let mut outgoing: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &(u, v) in edges {
        outgoing.entry(u).or_default().push(v);
    }
    let turn = |u: usize, v: usize| {
        let (from, here) = (at(u), at(v));
        let back = libm::atan2(from.y - here.y, from.x - here.x);
        outgoing.get(&v)?.iter().copied().min_by(|&w, &x| {
            clockwise(back, at(w), here)
                .total_cmp(&clockwise(back, at(x), here))
                .then(w.cmp(&x))
        })
    };
    let mut used = BTreeSet::new();
    let mut rings = Vec::new();
    for &start in edges {
        let mut walked = Vec::new();
        let mut edge = start;
        // Closed only when the walk comes back to its first edge, not when it stops on one used.
        let closed = loop {
            if !used.insert(edge) {
                break edge == start && !walked.is_empty();
            }
            walked.push(edge);
            let Some(next) = turn(edge.0, edge.1) else {
                break false;
            };
            edge = (edge.1, next);
        };
        if closed {
            rings.push(walked.iter().map(|&(u, _)| u).collect());
        } else {
            for edge in &walked {
                used.remove(edge);
            }
        }
    }
    rings
}

/// The clockwise angle from direction `back` to the direction from `here` to `to`, in (0, 2π].
fn clockwise(back: f64, to: Point2, here: Point2) -> f64 {
    let angle = back - libm::atan2(to.y - here.y, to.x - here.x);
    if angle <= 0.0 { angle + TAU } else { angle }
}

/// Twice the signed area of a ring (shoelace formula).
fn area(ring: &[usize], at: &impl Fn(usize) -> Point2) -> f64 {
    let points: Vec<Point2> = ring.iter().map(|&v| at(v)).collect();
    crate::vector::edges(&points)
        .map(|(p, q)| p.x * q.y - q.x * p.y)
        .sum()
}

/// Groups rings into regions: each ring with a positive area is an outer ring; every other ring
/// is a hole of the smallest outer ring of another connected part that contains it.
pub(super) fn regions(rings: Vec<Vec<usize>>, at: impl Fn(usize) -> Point2) -> Regions {
    let part = connected_parts(&rings);
    let areas: Vec<f64> = rings.iter().map(|ring| area(ring, &at)).collect();
    let outers: Vec<usize> = (0..rings.len()).filter(|&r| areas[r] > 0.0).collect();
    let mut region_of = vec![OUTSIDE; rings.len()];
    for (region, &r) in outers.iter().enumerate() {
        region_of[r] = region;
    }
    for r in (0..rings.len()).filter(|&r| areas[r] <= 0.0) {
        let probe = at(rings[r][0]);
        region_of[r] = outers
            .iter()
            .enumerate()
            .filter(|&(_, &o)| {
                part[&rings[o][0]] != part[&rings[r][0]]
                    && ring_contains(rings[o].iter().map(|&v| at(v)), probe)
            })
            .min_by(|&(_, &o), &(_, &p)| areas[o].total_cmp(&areas[p]))
            .map_or(OUTSIDE, |(region, _)| region);
    }
    let mut left = BTreeMap::new();
    for (r, ring) in rings.iter().enumerate() {
        for (u, v) in crate::vector::edges(ring) {
            left.insert((u, v), region_of[r]);
        }
    }
    let mut regions = vec![Region::default(); outers.len()];
    for (r, ring) in rings.into_iter().enumerate() {
        match region_of[r] {
            OUTSIDE => {}
            region if areas[r] > 0.0 => regions[region].outer = ring,
            region => regions[region].holes.push(ring),
        }
    }
    Regions { regions, left }
}

/// The faces bounded by directed edges with the inside on their left: each region as its outer
/// ring, then its holes.
pub(super) fn faces_of(
    boundary: &BTreeSet<(usize, usize)>,
    at: impl Fn(usize) -> Point2 + Copy,
) -> Vec<Vec<Vec<usize>>> {
    regions(chain(boundary, at), at)
        .regions
        .into_iter()
        .map(|region| std::iter::once(region.outer).chain(region.holes).collect())
        .collect()
}

/// Labels the vertices of the rings by connected part of the graph their edges form.
fn connected_parts(rings: &[Vec<usize>]) -> BTreeMap<usize, usize> {
    let index: BTreeMap<usize, usize> = rings
        .iter()
        .flatten()
        .copied()
        .collect::<BTreeSet<usize>>()
        .into_iter()
        .enumerate()
        .map(|(i, v)| (v, i))
        .collect();
    let mut parent: Vec<usize> = (0..index.len()).collect();
    for ring in rings {
        for (u, v) in crate::vector::edges(ring) {
            union(&mut parent, index[&u], index[&v]);
        }
    }
    index
        .into_iter()
        .map(|(v, i)| (v, find(&mut parent, i)))
        .collect()
}

/// A point well inside a region: the middle of a span of a horizontal line through it, among
/// lines halfway between consecutive vertex heights. The span kept is the one farthest from the
/// boundary by a simple measure: the smaller of its width and the gap between the heights its
/// line runs between. A line squeezed between two nearly equal heights may run along an edge, so
/// its spans never win. `None` for a region too thin to have one.
pub(super) fn interior_point(region: &Region, at: impl Fn(usize) -> Point2) -> Option<Point2> {
    let rings: Vec<Vec<Point2>> = std::iter::once(&region.outer)
        .chain(&region.holes)
        .map(|ring| ring.iter().map(|&v| at(v)).collect())
        .collect();
    let mut heights: Vec<f64> = rings.iter().flatten().map(|p| p.y).collect();
    heights.sort_by(f64::total_cmp);
    heights.dedup();
    let mut best: Option<(f64, Point2)> = None;
    let mut xs = Vec::new();
    for pair in heights.windows(2) {
        let (y, gap) = (f64::midpoint(pair[0], pair[1]), pair[1] - pair[0]);
        xs.clear();
        xs.extend(
            rings
                .iter()
                .flat_map(|ring| crate::vector::edges(ring))
                .filter(|(a, b)| (a.y > y) != (b.y > y))
                .map(|(a, b)| a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y)),
        );
        xs.sort_by(f64::total_cmp);
        for span in xs.as_chunks::<2>().0 {
            let room = (span[1] - span[0]).min(gap);
            if best.is_none_or(|(most, _)| room > most) {
                best = Some((room, Point2::new(f64::midpoint(span[0], span[1]), y)));
            }
        }
    }
    best.filter(|&(room, _)| room > 0.0).map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUARE: [Point2; 8] = [
        Point2::new(0.0, 0.0),
        Point2::new(4.0, 0.0),
        Point2::new(4.0, 4.0),
        Point2::new(0.0, 4.0),
        Point2::new(1.0, 1.0),
        Point2::new(3.0, 1.0),
        Point2::new(3.0, 3.0),
        Point2::new(1.0, 3.0),
    ];

    fn at(v: usize) -> Point2 {
        SQUARE[v]
    }

    fn both_ways(edges: &[(usize, usize)]) -> BTreeSet<(usize, usize)> {
        edges.iter().flat_map(|&(u, v)| [(u, v), (v, u)]).collect()
    }

    #[test]
    fn a_square_with_a_square_hole_is_one_region() {
        let edges = both_ways(&[
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
        ]);
        let found = regions(chain(&edges, at), at);
        // The band between the squares, and the inner square.
        assert_eq!(found.regions.len(), 2);
        let band = found.regions.iter().find(|r| r.outer.contains(&0)).unwrap();
        assert_eq!(band.holes.len(), 1);
        assert_eq!(found.left[&(1, 0)], OUTSIDE);
        let point = interior_point(band, at).unwrap();
        assert!(point.x < 1.0 || point.x > 3.0 || point.y < 1.0 || point.y > 3.0);
    }

    #[test]
    fn open_chains_are_dropped() {
        let edges: BTreeSet<_> = [(0, 1), (1, 2)].into_iter().collect();
        assert!(chain(&edges, at).is_empty());
    }

    #[test]
    fn a_flat_region_has_no_interior_point() {
        let region = Region {
            outer: vec![0, 1],
            holes: Vec::new(),
        };
        assert_eq!(interior_point(&region, at), None);
    }
}
