//! A bounding volume hierarchy over the world bounds of the objects, updated incrementally
//! (VP-02, `docs/dev/adr/0024-instancing-culling-and-level-of-detail.md`).
//!
//! It is the dynamic tree of `Box2D` (Erin Catto) in three dimensions: each leaf holds one item,
//! an insertion descends towards the sibling whose enlarged surface area costs least, and the
//! nodes on the way back up are refitted and rotated when one child is more than a level deeper
//! than the other, which keeps the height near logarithmic whatever the order of the insertions.
//! Moving an item removes and reinserts its leaf, in `O(log n)`. Nodes live in one vector with a
//! free list, so no update allocates once the tree has grown.
//!
//! Queries walk the tree with a caller's test on each box ([`Bvh::query`]): the frustum test of
//! the culling today, a ray test for picking tomorrow.

use crate::bounds::Aabb;

/// The index of no node.
const NONE: u32 = u32::MAX;

/// A leaf of the tree, as returned by [`Bvh::insert`]: valid until it is removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Leaf(u32);

/// How a box relates to the volume a query looks for, given the state of its parent.
pub(crate) enum Overlap<S> {
    /// Nothing in the box can match: skip it.
    Outside,
    /// Part of the box may match: test its children, with this state.
    Partial(S),
    /// The whole box matches: every item below is reported without further tests.
    Inside,
}

struct Node {
    bounds: Aabb,
    /// The parent, or the next free node of a free node.
    parent: u32,
    /// Both [`NONE`] for a leaf.
    children: [u32; 2],
    /// 0 for a leaf.
    height: u32,
    /// The caller's item of a leaf.
    item: u32,
}

impl Node {
    fn is_leaf(&self) -> bool {
        self.children[0] == NONE
    }
}

/// A dynamic tree of boxes, each carrying a caller's `u32` item.
pub(crate) struct Bvh {
    nodes: Vec<Node>,
    root: u32,
    free: u32,
    /// The nodes below an [`Overlap::Inside`] box, kept to avoid an allocation per query.
    stack: Vec<u32>,
}

impl Bvh {
    pub(crate) const fn new() -> Self {
        Self {
            nodes: Vec::new(),
            root: NONE,
            free: NONE,
            stack: Vec::new(),
        }
    }

    /// Adds `item` with its `bounds`.
    pub(crate) fn insert(&mut self, bounds: Aabb, item: u32) -> Leaf {
        let leaf = self.allocate(Node {
            bounds,
            parent: NONE,
            children: [NONE; 2],
            height: 0,
            item,
        });
        self.insert_leaf(leaf);
        Leaf(leaf)
    }

    /// Removes a leaf.
    pub(crate) fn remove(&mut self, Leaf(leaf): Leaf) {
        self.remove_leaf(leaf);
        self.release(leaf);
    }

    /// Gives a leaf new bounds; the leaf keeps its handle.
    pub(crate) fn update(&mut self, Leaf(leaf): Leaf, bounds: Aabb) {
        self.remove_leaf(leaf);
        self.node_mut(leaf).bounds = bounds;
        self.insert_leaf(leaf);
    }

    /// Calls `found` with the item of every leaf that `classify` does not put outside, starting
    /// from `state` at the root. A child is classified with the state its parent's
    /// [`Overlap::Partial`] gave; below an [`Overlap::Inside`], nothing more is classified.
    /// `pending` holds the nodes left to classify: the caller keeps it between queries, so a
    /// query allocates nothing once it has grown.
    pub(crate) fn query<S: Copy>(
        &mut self,
        pending: &mut Vec<(u32, S)>,
        state: S,
        mut classify: impl FnMut(&Aabb, S) -> Overlap<S>,
        mut found: impl FnMut(u32),
    ) {
        pending.clear();
        if self.root != NONE {
            pending.push((self.root, state));
        }
        while let Some((index, state)) = pending.pop() {
            let node = self.node(index);
            match classify(&node.bounds, state) {
                Overlap::Outside => {}
                Overlap::Inside => self.report_all(index, &mut found),
                Overlap::Partial(_) if node.is_leaf() => found(node.item),
                Overlap::Partial(inner) => {
                    pending.extend(node.children.map(|child| (child, inner)));
                }
            }
        }
    }

    /// Reports every item below `index`.
    fn report_all(&mut self, index: u32, found: &mut impl FnMut(u32)) {
        let mut stack = core::mem::take(&mut self.stack);
        stack.clear();
        stack.push(index);
        while let Some(index) = stack.pop() {
            let node = self.node(index);
            if node.is_leaf() {
                found(node.item);
            } else {
                stack.extend(node.children);
            }
        }
        self.stack = stack;
    }

    fn node(&self, index: u32) -> &Node {
        &self.nodes[index as usize]
    }

    fn node_mut(&mut self, index: u32) -> &mut Node {
        &mut self.nodes[index as usize]
    }

    fn allocate(&mut self, node: Node) -> u32 {
        if self.free == NONE {
            // The renderer bounds the objects far below 2³¹, and the tree has 2n − 1 nodes.
            let index = u32::try_from(self.nodes.len()).unwrap_or(NONE);
            self.nodes.push(node);
            index
        } else {
            let index = self.free;
            self.free = self.node(index).parent;
            *self.node_mut(index) = node;
            index
        }
    }

    fn release(&mut self, index: u32) {
        let free = self.free;
        let node = self.node_mut(index);
        node.parent = free;
        node.children = [NONE; 2];
        self.free = index;
    }

    fn insert_leaf(&mut self, leaf: u32) {
        if self.root == NONE {
            self.root = leaf;
            self.node_mut(leaf).parent = NONE;
            return;
        }
        let bounds = self.node(leaf).bounds;
        let sibling = self.best_sibling(&bounds);
        let old_parent = self.node(sibling).parent;
        let parent = self.allocate(Node {
            bounds: bounds.union(&self.node(sibling).bounds),
            parent: old_parent,
            children: [sibling, leaf],
            height: self.node(sibling).height + 1,
            item: NONE,
        });
        self.replace_child(old_parent, sibling, parent);
        self.node_mut(sibling).parent = parent;
        self.node_mut(leaf).parent = parent;
        self.refit_from(old_parent);
    }

    /// The node to pair a new leaf of `bounds` with: the descent stops where enlarging the
    /// current node costs less than going down either child (`Box2D`'s heuristic).
    fn best_sibling(&self, bounds: &Aabb) -> u32 {
        let mut index = self.root;
        loop {
            let node = self.node(index);
            if node.is_leaf() {
                return index;
            }
            let area = node.bounds.surface_area();
            let combined = node.bounds.union(bounds).surface_area();
            // Pairing here creates a node of the combined area.
            let cost = 2.0 * combined;
            // Going down enlarges this node anyway.
            let inheritance = 2.0 * (combined - area);
            let child_cost = |child: u32| {
                let child = self.node(child);
                let enlarged = child.bounds.union(bounds).surface_area();
                if child.is_leaf() {
                    enlarged + inheritance
                } else {
                    enlarged - child.bounds.surface_area() + inheritance
                }
            };
            let [first, second] = node.children;
            let (first_cost, second_cost) = (child_cost(first), child_cost(second));
            if cost < first_cost && cost < second_cost {
                return index;
            }
            index = if first_cost < second_cost {
                first
            } else {
                second
            };
        }
    }

    fn remove_leaf(&mut self, leaf: u32) {
        if leaf == self.root {
            self.root = NONE;
            return;
        }
        let parent = self.node(leaf).parent;
        let grandparent = self.node(parent).parent;
        let [first, second] = self.node(parent).children;
        let sibling = if first == leaf { second } else { first };
        self.replace_child(grandparent, parent, sibling);
        self.node_mut(sibling).parent = grandparent;
        self.release(parent);
        self.refit_from(grandparent);
    }

    /// Makes `new` the child of `parent` that `old` was, or the root when `parent` is none.
    fn replace_child(&mut self, parent: u32, old: u32, new: u32) {
        if parent == NONE {
            self.root = new;
        } else {
            for child in &mut self.node_mut(parent).children {
                if *child == old {
                    *child = new;
                }
            }
        }
    }

    /// Rebalances, then recomputes the bounds and height of every node from `index` to the root.
    fn refit_from(&mut self, mut index: u32) {
        while index != NONE {
            index = self.balance(index);
            self.refit(index);
            index = self.node(index).parent;
        }
    }

    fn refit(&mut self, index: u32) {
        let [first, second] = self.node(index).children;
        let (first, second) = (self.node(first), self.node(second));
        let bounds = first.bounds.union(&second.bounds);
        let height = 1 + first.height.max(second.height);
        let node = self.node_mut(index);
        node.bounds = bounds;
        node.height = height;
    }

    /// Rotates the deeper child of `a` above it when the heights of its children differ by
    /// more than one, and returns the node now at the place of `a`.
    fn balance(&mut self, a: u32) -> u32 {
        let node = self.node(a);
        if node.is_leaf() || node.height < 2 {
            return a;
        }
        let [b, c] = node.children;
        let (height_b, height_c) = (self.node(b).height, self.node(c).height);
        if height_c > height_b + 1 {
            self.rotate_up(a, 1);
            c
        } else if height_b > height_c + 1 {
            self.rotate_up(a, 0);
            b
        } else {
            a
        }
    }

    /// Moves the child `side` of `a` (call it `c`) into the place of `a`. `a` becomes a child of
    /// `c` and takes the lower of the two children of `c`; `c` keeps the higher one.
    fn rotate_up(&mut self, a: u32, side: usize) {
        let c = self.node(a).children[side];
        let [f, g] = self.node(c).children;
        let (keep, give) = if self.node(f).height > self.node(g).height {
            (f, g)
        } else {
            (g, f)
        };
        let grandparent = self.node(a).parent;
        self.replace_child(grandparent, a, c);
        self.node_mut(c).parent = grandparent;
        self.node_mut(c).children = [a, keep];
        self.node_mut(a).parent = c;
        self.node_mut(a).children[side] = give;
        self.node_mut(give).parent = a;
        self.refit(a);
        self.refit(c);
    }

    /// Checks the structure: parents and children agree, every parent encloses its children with
    /// the right height, the free list and the
    /// tree share no node, and every item is reachable once. Returns the items found.
    #[cfg(test)]
    fn check(&self) -> Vec<u32> {
        let mut items = Vec::new();
        let mut reachable = 0;
        let mut stack = Vec::new();
        if self.root != NONE {
            assert_eq!(self.node(self.root).parent, NONE);
            stack.push(self.root);
        }
        while let Some(index) = stack.pop() {
            reachable += 1;
            let node = self.node(index);
            if node.is_leaf() {
                assert_eq!(node.height, 0);
                items.push(node.item);
                continue;
            }
            let [first, second] = node.children.map(|child| self.node(child));
            for child in node.children {
                assert_eq!(self.node(child).parent, index);
            }
            assert_eq!(node.bounds, first.bounds.union(&second.bounds));
            assert_eq!(node.height, 1 + first.height.max(second.height));
            stack.extend(node.children);
        }
        let mut free = 0;
        let mut index = self.free;
        while index != NONE {
            free += 1;
            index = self.node(index).parent;
        }
        assert_eq!(reachable + free, self.nodes.len());
        items
    }

    #[cfg(test)]
    fn height(&self) -> u32 {
        if self.root == NONE {
            0
        } else {
            self.node(self.root).height
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use proptest::prelude::*;

    use super::*;

    fn cube(at: [f64; 3], side: f64) -> Aabb {
        Aabb {
            min: at,
            max: at.map(|v| v + side),
        }
    }

    /// The items whose box overlaps `region`, as the tree finds them, sorted.
    fn overlapping(tree: &mut Bvh, region: &Aabb) -> Vec<u32> {
        let mut found = Vec::new();
        tree.query(
            &mut Vec::new(),
            (),
            |bounds, ()| {
                if !region.overlaps(bounds) {
                    Overlap::Outside
                } else if region.contains(bounds) {
                    Overlap::Inside
                } else {
                    Overlap::Partial(())
                }
            },
            |item| found.push(item),
        );
        found.sort_unstable();
        found
    }

    #[test]
    fn an_empty_tree_finds_nothing() {
        let mut tree = Bvh::new();
        assert!(overlapping(&mut tree, &cube([0.0; 3], 1e9)).is_empty());
        assert!(tree.check().is_empty());
    }

    #[test]
    fn a_row_inserted_in_order_stays_balanced_and_is_found() {
        let mut tree = Bvh::new();
        let leaves: Vec<Leaf> = (0..1000_u32)
            .map(|i| tree.insert(cube([f64::from(i) * 2.0, 0.0, 0.0], 1.0), i))
            .collect();
        assert_eq!(tree.check().len(), 1000);
        // A balanced binary tree of 1000 leaves is 10 levels deep; AVL rotations keep it within
        // about 1.44 log₂ n.
        assert!(tree.height() <= 15, "height {}", tree.height());
        assert_eq!(
            overlapping(&mut tree, &cube([9.5, -1.0, -1.0], 3.0)),
            vec![5, 6]
        );
        // Moving the first box far away: it is found there, and no longer where it was.
        tree.update(leaves[0], cube([1e7, 1e7, 0.0], 1.0));
        assert_eq!(
            overlapping(&mut tree, &cube([1e7 - 1.0, 1e7 - 1.0, -1.0], 3.0)),
            vec![0]
        );
        assert!(overlapping(&mut tree, &cube([-0.5, -0.5, -0.5], 1.0)).is_empty());
        for leaf in leaves {
            tree.remove(leaf);
        }
        assert!(tree.check().is_empty());
        // Removed nodes are reused.
        let nodes = tree.nodes.len();
        tree.insert(cube([0.0; 3], 1.0), 7);
        tree.insert(cube([3.0; 3], 1.0), 8);
        assert_eq!(tree.nodes.len(), nodes);
        assert_eq!(tree.check().len(), 2);
    }

    #[derive(Clone, Debug)]
    enum Operation {
        Insert([f64; 3], f64),
        Move(usize, [f64; 3]),
        Remove(usize),
        Query([f64; 3], f64),
    }

    fn point() -> impl Strategy<Value = [f64; 3]> {
        proptest::array::uniform3(-100.0..100.0_f64)
    }

    fn operation() -> impl Strategy<Value = Operation> {
        prop_oneof![
            3 => (point(), 0.0..20.0_f64).prop_map(|(at, side)| Operation::Insert(at, side)),
            2 => (any::<usize>(), point()).prop_map(|(which, at)| Operation::Move(which, at)),
            1 => any::<usize>().prop_map(Operation::Remove),
            2 => (point(), 0.0..80.0_f64).prop_map(|(at, side)| Operation::Query(at, side)),
        ]
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        /// Whatever the inserts, moves and removals, the tree keeps its structure, and a query
        /// finds exactly the items a brute-force scan finds.
        #[test]
        fn queries_match_a_brute_force_scan(
            operations in proptest::collection::vec(operation(), 1..200),
        ) {
            let mut tree = Bvh::new();
            let mut live: BTreeMap<u32, (Leaf, Aabb)> = BTreeMap::new();
            let mut next = 0;
            for operation in operations {
                let pick = |which: usize| live.keys().nth(which % live.len().max(1)).copied();
                match operation {
                    Operation::Insert(at, side) => {
                        let bounds = cube(at, side);
                        live.insert(next, (tree.insert(bounds, next), bounds));
                        next += 1;
                    }
                    Operation::Move(which, at) => {
                        if let Some(item) = pick(which) {
                            let entry = live.get_mut(&item).unwrap();
                            entry.1 = cube(at, 1.0);
                            tree.update(entry.0, entry.1);
                        }
                    }
                    Operation::Remove(which) => {
                        if let Some(item) = pick(which) {
                            tree.remove(live.remove(&item).unwrap().0);
                        }
                    }
                    Operation::Query(at, side) => {
                        let region = cube(at, side);
                        let expected: Vec<u32> = live
                            .iter()
                            .filter(|(_, (_, bounds))| region.overlaps(bounds))
                            .map(|(&item, _)| item)
                            .collect();
                        prop_assert_eq!(overlapping(&mut tree, &region), expected);
                    }
                }
                let mut items = tree.check();
                items.sort_unstable();
                prop_assert_eq!(items, live.keys().copied().collect::<Vec<_>>());
            }
        }
    }
}
