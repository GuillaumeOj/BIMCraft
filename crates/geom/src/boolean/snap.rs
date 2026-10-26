//! Vertices that are one point, and the union–find the merge pass groups with.

use std::collections::BTreeMap;

use super::face::Pool;

/// For each vertex, the vertex standing for the point it is: vertices at exactly the same
/// position, within rounding of each other, or made on three of the same planes within the
/// tolerance of each other are one point (ADR 0001, constructed geometry), so faces meeting
/// there share it. The smallest number stands for each point, and takes every plane of the
/// others. No vertex moves by more than rounding, or by more than the tolerance along planes it
/// was made on.
pub(super) fn coincident(pool: &mut Pool) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..pool.len()).collect();
    let mut first: BTreeMap<[u64; 3], usize> = BTreeMap::new();
    for v in 0..pool.len() {
        // Adding zero turns −0 into +0, so both are one position.
        let at = *first
            .entry(pool.get(v).map(|c| (c + 0.0).to_bits()))
            .or_insert(v);
        union(&mut parent, v, at);
        let made = pool.made_on(pool.get(v), pool.planes(v));
        for same in [made, pool.at(pool.get(v))].into_iter().flatten() {
            union(&mut parent, v, same);
        }
    }
    let rename: Vec<usize> = (0..pool.len()).map(|v| find(&mut parent, v)).collect();
    for (v, &to) in rename.iter().enumerate() {
        if to != v {
            let planes: Vec<usize> = pool.planes(v).iter().copied().collect();
            pool.lies_on(to, planes.into_iter());
        }
    }
    rename
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
    fn vertices_that_are_one_point_become_one() {
        // 0 and 2 share a position; 3 was made on three of 1's planes; 4 is apart.
        let mut pool = Pool::of(vec![
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [2.0, 0.3e-6, 0.0],
            [5.0, 0.0, 0.0],
        ]);
        pool.lies_on(1, [0, 1, 2].into_iter());
        pool.lies_on(3, [0, 1, 2, 9].into_iter());
        pool.place_all();
        assert_eq!(coincident(&mut pool), [0, 1, 0, 1, 4]);
        assert!(pool.planes(1).contains(&9));
    }
}
