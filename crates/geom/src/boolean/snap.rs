//! Merging vertices within the tolerance (ADR 0001, merge pass step 2).

use std::collections::HashMap;

use super::BooleanError;
use super::face::Pool;
use crate::LINEAR_TOLERANCE;
use crate::tolerance::{WITHIN_TOLERANCE, WITHIN_TOLERANCE_SQUARED};
use crate::vector::{Vector, bounds, distance_squared};

/// The widest a group of merged vertices may be.
const CHAIN_LIMIT: f64 = 10.0 * LINEAR_TOLERANCE;

/// Groups vertices within the tolerance with a union–find and returns, for each vertex, the
/// representative of its group: the smallest by x, then y, then z among the group's operand
/// vertices if it has any, otherwise among all of them. Positions are never averaged, and the
/// choice depends only on positions, so swapping the operands gives the same result.
///
/// Only the vertices in `among` take part; every other vertex keeps its own name. Candidates
/// come from a hash of cells one tolerance wide, never from comparing all pairs.
pub(super) fn snap(pool: &Pool, among: &[usize]) -> Result<Vec<usize>, BooleanError> {
    let mut parent: Vec<usize> = (0..pool.len()).collect();
    let mut cells: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    for &v in among {
        let p = pool.get(v);
        let cell = cell_of(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let near = [cell[0] + dx, cell[1] + dy, cell[2] + dz];
                    for &w in cells.get(&near).into_iter().flatten() {
                        if distance_squared(p, pool.get(w)) <= WITHIN_TOLERANCE_SQUARED {
                            union(&mut parent, v, w);
                        }
                    }
                }
            }
        }
        cells.entry(cell).or_default().push(v);
    }
    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); pool.len()];
    for &v in among {
        groups[find(&mut parent, v)].push(v);
    }
    let mut rename: Vec<usize> = (0..pool.len()).collect();
    for group in groups.iter().filter(|group| group.len() > 1) {
        let Some((lo, hi)) = bounds(group.iter().map(|&v| pool.get(v))) else {
            continue;
        };
        if (0..3).any(|i| hi[i] - lo[i] > CHAIN_LIMIT) {
            return Err(BooleanError::MergeChainTooWide);
        }
        // Operand vertices first, then by position, then by index: one total order.
        let representative = group.iter().copied().fold(group[0], |best, v| {
            let order = pool
                .is_input(best)
                .cmp(&pool.is_input(v))
                .then(lexicographic(pool.get(v), pool.get(best)))
                .then(v.cmp(&best));
            if order.is_lt() { v } else { best }
        });
        for &v in group {
            rename[v] = representative;
        }
    }
    Ok(rename)
}

fn lexicographic(a: Vector, b: Vector) -> std::cmp::Ordering {
    a[0].total_cmp(&b[0])
        .then(a[1].total_cmp(&b[1]))
        .then(a[2].total_cmp(&b[2]))
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "model coordinates over the tolerance stay within ±1e10"
)]
fn cell_of(p: Vector) -> [i64; 3] {
    p.map(|c| (c / WITHIN_TOLERANCE).floor() as i64)
}

/// Joins the sets of `a` and `b` in a union–find. The smaller root stays root, so the result
/// depends only on the order of the items.
pub(super) fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    parent[ra.max(rb)] = ra.min(rb);
}

/// The representative of a set in a union–find, compressing the path to it.
pub(super) fn find(parent: &mut [usize], item: usize) -> usize {
    let mut root = item;
    while parent[root] != root {
        root = parent[root];
    }
    parent[item] = root;
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_vertices_merge_onto_the_smallest_operand_vertex() {
        let mut pool = Pool::of(vec![
            [1.0, 0.0, 0.0],
            [1.0 - 0.5e-6, 0.0, 0.0],
            [5.0, 0.0, 0.0],
        ]);
        let constructed = pool.push([1.0 - 0.9e-6, 0.0, 0.0]);
        let rename = snap(&pool, &[0, 1, 2, 3]).unwrap();
        // The constructed vertex is smaller, but an operand vertex is preferred.
        assert_eq!(rename, [1, 1, 2, 1]);
        assert_eq!(constructed, 3);
    }

    #[test]
    fn a_chain_wider_than_ten_tolerances_is_refused() {
        // Twelve gaps of 0.9 µm: 10.8 µm from end to end.
        let pool = Pool::of((0..13).map(|i| [f64::from(i) * 0.9e-6, 0.0, 0.0]).collect());
        let all: Vec<usize> = (0..13).collect();
        assert_eq!(snap(&pool, &all), Err(BooleanError::MergeChainTooWide));
        // Every other vertex alone is too far apart to merge.
        let some: Vec<usize> = (0..13).step_by(2).collect();
        assert_eq!(snap(&pool, &some), Ok(all));
    }
}
