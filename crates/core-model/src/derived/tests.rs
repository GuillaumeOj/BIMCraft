use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;

use super::{DependencyGraph, DerivedError};

/// Computes every node as its input (0 by default) plus the sum of its dependencies, recording
/// each call so tests can count recomputations.
struct Model {
    inputs: BTreeMap<u32, i64>,
    computed: Vec<u32>,
}

impl Model {
    fn new() -> Self {
        Self {
            inputs: BTreeMap::new(),
            computed: Vec::new(),
        }
    }

    fn get(&mut self, graph: &mut DependencyGraph<u32, i64>, node: u32) -> i64 {
        let Self { inputs, computed } = self;
        *graph
            .get(&node, |key, deps| {
                computed.push(*key);
                inputs.get(key).copied().unwrap_or(0) + deps.iter().map(|(_, v)| **v).sum::<i64>()
            })
            .unwrap()
    }

    fn take_computed(&mut self) -> Vec<u32> {
        let mut computed = std::mem::take(&mut self.computed);
        computed.sort_unstable();
        computed
    }
}

/// 3 depends on 1 and 2, which both depend on 0; 5 depends on 4.
fn diamond() -> DependencyGraph<u32, i64> {
    let mut graph = DependencyGraph::new();
    graph.set_dependencies(1, [0]).unwrap();
    graph.set_dependencies(2, [0]).unwrap();
    graph.set_dependencies(3, [1, 2]).unwrap();
    graph.set_dependencies(5, [4]).unwrap();
    graph
}

fn set(keys: impl IntoIterator<Item = u32>) -> BTreeSet<u32> {
    keys.into_iter().collect()
}

#[test]
fn computes_each_node_once_then_serves_the_cache() {
    let mut graph = diamond();
    let mut model = Model::new();
    model.inputs.insert(0, 10);
    assert_eq!(model.get(&mut graph, 3), 20);
    assert_eq!(model.take_computed(), [0, 1, 2, 3]);
    assert_eq!(model.get(&mut graph, 3), 20);
    assert_eq!(model.get(&mut graph, 1), 10);
    assert!(model.take_computed().is_empty());
}

#[test]
fn computes_dependencies_before_dependents() {
    let mut graph = diamond();
    let mut model = Model::new();
    model.get(&mut graph, 3);
    let order = &model.computed;
    let position = |key: u32| order.iter().position(|k| *k == key).unwrap();
    assert!(position(0) < position(1) && position(0) < position(2));
    assert!(position(1) < position(3) && position(2) < position(3));
}

#[test]
fn invalidating_an_input_recomputes_only_its_dependents() {
    let mut graph = diamond();
    let mut model = Model::new();
    model.get(&mut graph, 3);
    model.get(&mut graph, 5);
    model.take_computed();

    model.inputs.insert(0, 7);
    assert_eq!(graph.invalidate([0]), set([0, 1, 2, 3]));
    assert!(
        model.take_computed().is_empty(),
        "invalidation alone computes nothing"
    );
    assert!(graph.is_clean(&5) && graph.is_clean(&4));
    assert_eq!(model.get(&mut graph, 3), 14);
    assert_eq!(model.take_computed(), [0, 1, 2, 3]);
    assert_eq!(model.get(&mut graph, 5), 0);
    assert!(model.take_computed().is_empty());
}

#[test]
fn invalidating_a_middle_node_leaves_its_siblings_alone() {
    let mut graph = diamond();
    let mut model = Model::new();
    model.get(&mut graph, 3);
    model.take_computed();

    model.inputs.insert(1, 5);
    assert_eq!(graph.invalidate([1]), set([1, 3]));
    assert_eq!(model.get(&mut graph, 3), 5);
    assert_eq!(model.take_computed(), [1, 3]);
}

#[test]
fn recomputes_only_what_the_requested_node_needs() {
    let mut graph = diamond();
    let mut model = Model::new();
    model.get(&mut graph, 3);
    graph.invalidate([0]);
    model.take_computed();
    model.get(&mut graph, 1);
    assert_eq!(model.take_computed(), [0, 1]);
    assert!(!graph.is_clean(&2) && !graph.is_clean(&3));
}

#[test]
fn invalidation_stops_at_stale_nodes_and_ignores_unknown_keys() {
    let mut graph = diamond();
    let mut model = Model::new();
    model.get(&mut graph, 3);
    assert_eq!(graph.invalidate([1]), set([1, 3]));
    assert_eq!(graph.invalidate([1, 3, 99]), set([]));
    assert_eq!(graph.invalidate([0]), set([0, 2]));
}

#[test]
fn redeclaring_the_same_dependencies_keeps_the_cache() {
    let mut graph = diamond();
    let mut model = Model::new();
    model.get(&mut graph, 3);
    model.take_computed();
    assert_eq!(graph.set_dependencies(3, [2, 1, 2]), Ok(()));
    assert!(graph.is_clean(&3));
    model.get(&mut graph, 3);
    assert!(model.take_computed().is_empty());
}

#[test]
fn redeclaring_dependencies_replaces_the_old_edges() {
    let mut graph = diamond();
    let mut model = Model::new();
    model.inputs.extend([(0, 1), (4, 100)]);
    model.get(&mut graph, 3);
    model.take_computed();

    assert_eq!(graph.set_dependencies(1, [4]), Ok(()));
    assert_eq!(graph.dependencies(&1).copied().collect::<Vec<_>>(), [4]);
    assert_eq!(model.get(&mut graph, 3), 101);
    assert_eq!(model.take_computed(), [1, 3, 4]);

    // 1 no longer depends on 0, so changing 0 reaches 3 through 2 only.
    assert_eq!(graph.invalidate([0]), set([0, 2, 3]));
}

#[test]
fn removing_a_used_node_keeps_it_as_an_input_of_its_dependents() {
    let mut graph = diamond();
    let mut model = Model::new();
    model.inputs.extend([(0, 1), (1, 40)]);
    model.get(&mut graph, 3);
    model.take_computed();

    assert_eq!(graph.remove(&1), set([3]));
    assert_eq!(graph.dependencies(&3).copied().collect::<Vec<_>>(), [1, 2]);
    assert_eq!(graph.dependencies(&1).count(), 0);
    assert_eq!(
        graph.invalidate([0]),
        set([0, 2]),
        "1 no longer depends on 0"
    );
    assert_eq!(model.get(&mut graph, 3), 41);
    assert_eq!(model.take_computed(), [0, 1, 2, 3]);

    graph.set_dependencies(3, [2]).unwrap();
    assert_eq!(graph.get(&1, |_, _| 0), Err(DerivedError::UnknownNode));
    assert!(graph.remove(&99).is_empty());
}

#[test]
fn removing_an_unused_node_prunes_its_undeclared_inputs() {
    let mut graph = diamond();
    assert!(graph.remove(&5).is_empty());
    assert_eq!(graph.get(&5, |_, _| 0), Err(DerivedError::UnknownNode));
    assert_eq!(graph.get(&4, |_, _| 0), Err(DerivedError::UnknownNode));
    assert!(graph.remove(&3).is_empty());
    assert_eq!(graph.get(&1, |_, _| 7), Ok(&7), "declared nodes stay");
}

#[test]
fn redeclaring_prunes_inputs_nothing_uses_any_more() {
    let mut graph: DependencyGraph<u32, i64> = DependencyGraph::new();
    graph.set_dependencies(10, [1, 2, 3]).unwrap();
    graph.set_dependencies(2, []).unwrap();
    graph.set_dependencies(3, []).unwrap();
    graph.set_dependencies(3, []).unwrap();
    graph.set_dependencies(10, [4]).unwrap();
    assert_eq!(graph.get(&1, |_, _| 0), Err(DerivedError::UnknownNode));
    assert_eq!(
        graph.get(&2, |_, _| 2),
        Ok(&2),
        "declared with set_dependencies"
    );
    assert_eq!(
        graph.get(&3, |_, _| 3),
        Ok(&3),
        "declared while already present"
    );
}

#[test]
fn redeclaring_keeps_the_cached_value_of_a_kept_input() {
    let mut graph = diamond();
    let mut model = Model::new();
    model.get(&mut graph, 1);
    model.take_computed();
    graph.set_dependencies(1, [0, 4]).unwrap();
    assert!(graph.is_clean(&0));
    model.get(&mut graph, 1);
    assert_eq!(model.take_computed(), [1, 4]);
}

#[test]
fn rejects_unknown_nodes() {
    let mut graph: DependencyGraph<u32, i64> = DependencyGraph::default();
    assert_eq!(graph.get(&0, |_, _| 0), Err(DerivedError::UnknownNode));
    assert!(!graph.is_clean(&0));
    assert_eq!(graph.dependencies(&0).count(), 0);
}

#[test]
fn rejects_a_self_dependency() {
    let mut graph: DependencyGraph<u32, i64> = DependencyGraph::new();
    assert_eq!(
        graph.set_dependencies(0, [1, 0]),
        Err(DerivedError::Cycle { path: vec![0, 0] })
    );
    assert_eq!(graph.get(&0, |_, _| 0), Err(DerivedError::UnknownNode));
}

#[test]
fn rejects_a_two_node_cycle() {
    let mut graph = diamond();
    assert_eq!(
        graph.set_dependencies(0, [1]),
        Err(DerivedError::Cycle {
            path: vec![0, 1, 0]
        })
    );
}

#[test]
fn reports_the_whole_cycle_and_leaves_the_graph_unchanged() {
    let mut graph: DependencyGraph<u32, i64> = DependencyGraph::new();
    for node in 1..6 {
        graph.set_dependencies(node, [node - 1, 100]).unwrap();
    }
    let mut model = Model::new();
    model.get(&mut graph, 5);
    model.take_computed();

    assert_eq!(
        graph.set_dependencies(0, [7, 5]),
        Err(DerivedError::Cycle {
            path: vec![0, 5, 4, 3, 2, 1, 0]
        })
    );
    assert_eq!(graph.dependencies(&0).count(), 0);
    assert!(graph.is_clean(&5));
    assert!(!graph.is_clean(&7), "the rejected call adds no node");
    assert_eq!(model.get(&mut graph, 5), 0);
    assert!(model.take_computed().is_empty());
}

#[test]
fn cycle_message_gives_the_length_only() {
    let error = DerivedError::Cycle {
        path: vec!["wall", "slab", "wall"],
    };
    assert_eq!(
        error.to_string(),
        "the dependencies form a cycle of length 2"
    );
    let unknown: DerivedError<u32> = DerivedError::UnknownNode;
    assert_eq!(
        unknown.to_string(),
        "the node is not in the dependency graph"
    );
}

#[test]
fn handles_deep_chains_without_recursion() {
    const DEPTH: u32 = 100_000;
    let mut graph: DependencyGraph<u32, i64> = DependencyGraph::new();
    for node in 1..=DEPTH {
        graph.set_dependencies(node, [node - 1]).unwrap();
    }
    let mut model = Model::new();
    model.inputs.insert(0, 1);
    assert_eq!(model.get(&mut graph, DEPTH), 1);
    assert_eq!(
        graph.invalidate([0]).len(),
        usize::try_from(DEPTH).unwrap() + 1
    );
    assert!(
        matches!(graph.set_dependencies(0, [DEPTH]), Err(DerivedError::Cycle { path }) if path.len() == 100_002)
    );
}

/// A random acyclic graph: node `i` depends on some nodes below `i`.
fn dag() -> impl Strategy<Value = Vec<Vec<u32>>> {
    (1..24_u32).prop_flat_map(|size| {
        (0..size)
            .map(|node| proptest::collection::vec(0..node.max(1), 0..4))
            .collect::<Vec<_>>()
            .prop_map(|deps| {
                deps.into_iter()
                    .enumerate()
                    .map(|(node, deps)| deps.into_iter().filter(|d| (*d as usize) < node).collect())
                    .collect()
            })
    })
}

fn build(deps: &[Vec<u32>]) -> DependencyGraph<u32, i64> {
    let mut graph = DependencyGraph::new();
    for (node, deps) in (0..).zip(deps) {
        graph.set_dependencies(node, deps.iter().copied()).unwrap();
    }
    graph
}

/// Each node's value computed from scratch, nodes in index order.
fn from_scratch(deps: &[Vec<u32>], inputs: &BTreeMap<u32, i64>) -> Vec<i64> {
    let mut values: Vec<i64> = Vec::new();
    for (node, deps) in (0..).zip(deps) {
        let unique: BTreeSet<u32> = deps.iter().copied().collect();
        let sum: i64 = unique.iter().map(|d| values[*d as usize]).sum();
        values.push(inputs.get(&node).copied().unwrap_or(0) + sum);
    }
    values
}

/// Whether following `dependencies` from any node never comes back to it (Kahn's algorithm).
fn is_acyclic(graph: &DependencyGraph<u32, i64>, nodes: u32) -> bool {
    let mut remaining: BTreeMap<u32, usize> = (0..nodes)
        .map(|n| (n, graph.dependencies(&n).count()))
        .collect();
    let mut ready: Vec<u32> = remaining
        .iter()
        .filter(|(_, c)| **c == 0)
        .map(|(n, _)| *n)
        .collect();
    let mut done = 0;
    while let Some(node) = ready.pop() {
        done += 1;
        for other in 0..nodes {
            if graph.dependencies(&other).any(|d| *d == node) {
                let count = remaining.get_mut(&other).unwrap();
                *count -= 1;
                if *count == 0 {
                    ready.push(other);
                }
            }
        }
    }
    done == nodes
}

proptest! {
    #![proptest_config(crate::proptest_config())]

    #[test]
    fn incremental_values_match_a_full_recomputation(
        deps in dag(),
        changes in proptest::collection::vec((0..24_u32, -50..50_i64), 0..6),
    ) {
        let size = u32::try_from(deps.len()).unwrap();
        let mut graph = build(&deps);
        let mut model = Model::new();
        for node in 0..size {
            model.get(&mut graph, node);
        }
        prop_assert_eq!(model.take_computed(), (0..size).collect::<Vec<_>>());

        let mut stale = BTreeSet::new();
        for (node, value) in changes.into_iter().filter(|(node, _)| *node < size) {
            model.inputs.insert(node, value);
            stale.append(&mut graph.invalidate([node]));
        }
        let expected = from_scratch(&deps, &model.inputs);
        for node in 0..size {
            prop_assert_eq!(model.get(&mut graph, node), expected[node as usize]);
        }
        prop_assert_eq!(model.take_computed(), stale.into_iter().collect::<Vec<_>>());
    }

    #[test]
    fn accepted_dependencies_never_form_a_cycle(
        edges in proptest::collection::vec((0..12_u32, proptest::collection::vec(0..12_u32, 0..3)), 0..30),
    ) {
        let mut graph: DependencyGraph<u32, i64> = DependencyGraph::new();
        for (node, deps) in edges {
            let before: Vec<u32> = graph.dependencies(&node).copied().collect();
            match graph.set_dependencies(node, deps.iter().copied()) {
                Ok(()) => {}
                Err(DerivedError::Cycle { path }) => {
                    prop_assert_eq!(path.first(), Some(&node));
                    prop_assert_eq!(path.last(), Some(&node));
                    prop_assert!(deps.contains(&path[1]));
                    for pair in path[1..].windows(2) {
                        prop_assert!(graph.dependencies(&pair[0]).any(|d| *d == pair[1]));
                    }
                    prop_assert_eq!(graph.dependencies(&node).copied().collect::<Vec<_>>(), before);
                }
                Err(DerivedError::UnknownNode) => prop_assert!(false, "unexpected error"),
            }
            prop_assert!(is_acyclic(&graph, 12));
        }
    }
}
