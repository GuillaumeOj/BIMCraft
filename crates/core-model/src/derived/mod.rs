//! Lazy, incremental recomputation of derived data through a dependency graph (CORE-08).
//!
//! Geometry, views and analytical models are computed from the model and from each other. A
//! [`DependencyGraph`] records which node is computed from which, caches every computed value,
//! and recomputes only what a change makes stale:
//!
//! - **incremental**: [`DependencyGraph::invalidate`] marks the changed nodes and everything that
//!   depends on them, transitively, as stale; every other cached value is kept;
//! - **lazy**: nothing is computed until [`DependencyGraph::get`] asks for a node, and then only
//!   the stale nodes it depends on are recomputed, each once, dependencies first.
//!
//! Keys are chosen by the caller, for example `(EntityId, attribute)` for model inputs and
//! `(EntityId, representation)` for geometry. A key that only appears as a dependency is an input:
//! it is computed like any other node, with no dependency values.
//!
//! ```
//! use core_model::derived::DependencyGraph;
//!
//! let mut width = 3;
//! let mut graph = DependencyGraph::new();
//! graph.set_dependencies("area", ["width", "depth"]).unwrap();
//! let compute = |width: i32| {
//!     move |key: &&str, deps: &[(&&str, &i32)]| match *key {
//!         "width" => width,
//!         "depth" => 2,
//!         _ => deps.iter().map(|(_, value)| **value).product(),
//!     }
//! };
//! assert_eq!(graph.get(&"area", compute(width)), Ok(&6));
//!
//! width = 4;
//! graph.invalidate(["width"]);
//! assert_eq!(graph.get(&"area", compute(width)), Ok(&8));
//! ```
//!
//! A value is recomputed whenever a dependency was invalidated, even if the dependency's new value
//! equals the old one; stopping early in that case is left for when profiling asks for it.

mod error;

use std::collections::{BTreeMap, BTreeSet};

pub use error::DerivedError;

/// Derived values and the dependencies between them, with lazy, incremental recomputation.
///
/// The graph never contains a cycle: [`set_dependencies`](Self::set_dependencies) rejects any
/// declaration that would close one. Every traversal is iterative, so deep graphs cannot overflow
/// the stack.
#[derive(Debug)]
pub struct DependencyGraph<K, V> {
    nodes: BTreeMap<K, Node<K, V>>,
}

/// One node: its edges in both directions and its cached value.
#[derive(Debug)]
struct Node<K, V> {
    /// Nodes this one is computed from.
    deps: BTreeSet<K>,
    /// Nodes computed from this one.
    dependents: BTreeSet<K>,
    /// The cached value; `None` while stale.
    value: Option<V>,
}

impl<K, V> Default for Node<K, V> {
    fn default() -> Self {
        Self {
            deps: BTreeSet::new(),
            dependents: BTreeSet::new(),
            value: None,
        }
    }
}

impl<K: Ord + Clone, V> Default for DependencyGraph<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord + Clone, V> DependencyGraph<K, V> {
    /// An empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
        }
    }

    /// Declares that `node` is computed from `deps`, replacing its previous dependencies.
    ///
    /// Unknown keys are added as nodes. `node` and everything that depends on it become stale.
    ///
    /// # Errors
    ///
    /// [`DerivedError::Cycle`] if `node` would depend on itself, directly or transitively. The
    /// graph is then left unchanged.
    pub fn set_dependencies(
        &mut self,
        node: K,
        deps: impl IntoIterator<Item = K>,
    ) -> Result<(), DerivedError<K>> {
        let deps: BTreeSet<K> = deps.into_iter().collect();
        if let Some(path) = self.cycle_through(&node, &deps) {
            return Err(DerivedError::Cycle { path });
        }
        let entry = self.nodes.entry(node.clone()).or_default();
        let old_deps = std::mem::replace(&mut entry.deps, deps.clone());
        for dep in &old_deps {
            if let Some(dep_node) = self.nodes.get_mut(dep) {
                dep_node.dependents.remove(&node);
            }
        }
        for dep in deps {
            self.nodes
                .entry(dep)
                .or_default()
                .dependents
                .insert(node.clone());
        }
        self.mark_stale(node);
        Ok(())
    }

    /// Removes `node`, for example when its object is deleted, and returns the other nodes it made
    /// stale.
    ///
    /// Its dependents become stale and lose it as a dependency; redeclare them if they still need
    /// one. Removing an unknown key does nothing.
    pub fn remove(&mut self, node: &K) -> BTreeSet<K> {
        let mut stale = self.invalidate([node.clone()]);
        stale.remove(node);
        if let Some(removed) = self.nodes.remove(node) {
            for dep in &removed.deps {
                if let Some(dep_node) = self.nodes.get_mut(dep) {
                    dep_node.dependents.remove(node);
                }
            }
            for dependent in &removed.dependents {
                if let Some(dependent_node) = self.nodes.get_mut(dependent) {
                    dependent_node.deps.remove(node);
                }
            }
        }
        stale
    }

    /// Marks the `changed` nodes and everything that depends on them as stale, and returns the
    /// nodes that were up to date before. Computes nothing; unknown keys are ignored.
    pub fn invalidate(&mut self, changed: impl IntoIterator<Item = K>) -> BTreeSet<K> {
        let mut stale = BTreeSet::new();
        for key in changed {
            stale.append(&mut self.mark_stale(key));
        }
        stale
    }

    /// Whether `node` is known and holds an up-to-date value.
    #[must_use]
    pub fn is_clean(&self, node: &K) -> bool {
        self.nodes.get(node).is_some_and(|n| n.value.is_some())
    }

    /// The nodes `node` is computed from, in key order; empty for an input or an unknown key.
    pub fn dependencies<'a>(&'a self, node: &K) -> impl Iterator<Item = &'a K> + use<'a, K, V> {
        self.nodes.get(node).into_iter().flat_map(|n| n.deps.iter())
    }

    /// The value of `node`, recomputing first whatever is stale among it and its dependencies.
    ///
    /// `compute(key, deps)` is called once per stale node, dependencies first, with the values of
    /// the node's dependencies in key order. Up-to-date nodes are not recomputed.
    ///
    /// # Errors
    ///
    /// [`DerivedError::UnknownNode`] if `node` was never declared.
    pub fn get(
        &mut self,
        node: &K,
        mut compute: impl FnMut(&K, &[(&K, &V)]) -> V,
    ) -> Result<&V, DerivedError<K>> {
        if !self.nodes.contains_key(node) {
            return Err(DerivedError::UnknownNode);
        }
        // Post-order walk over stale nodes: a node is computed when popped the second time, after
        // its dependencies. The graph is acyclic, so this terminates.
        let mut stack = vec![(node.clone(), false)];
        while let Some((key, deps_ready)) = stack.pop() {
            let Some(current) = self.nodes.get(&key).filter(|n| n.value.is_none()) else {
                continue;
            };
            if deps_ready {
                let value = {
                    let deps: Vec<(&K, &V)> = current
                        .deps
                        .iter()
                        .filter_map(|dep| Some((dep, self.nodes.get(dep)?.value.as_ref()?)))
                        .collect();
                    compute(&key, &deps)
                };
                if let Some(current) = self.nodes.get_mut(&key) {
                    current.value = Some(value);
                }
            } else {
                let stale_deps: Vec<K> = current
                    .deps
                    .iter()
                    .filter(|dep| !self.is_clean(dep))
                    .cloned()
                    .collect();
                stack.push((key, true));
                stack.extend(stale_deps.into_iter().map(|dep| (dep, false)));
            }
        }
        self.nodes
            .get(node)
            .and_then(|n| n.value.as_ref())
            .ok_or(DerivedError::UnknownNode)
    }

    /// Marks `start` and its transitive dependents stale; returns those that were up to date.
    ///
    /// A stale node's dependents are always stale already, so the walk stops at stale nodes.
    fn mark_stale(&mut self, start: K) -> BTreeSet<K> {
        let mut stale = BTreeSet::new();
        let mut pending = vec![start];
        while let Some(key) = pending.pop() {
            let Some(current) = self.nodes.get_mut(&key) else {
                continue;
            };
            if current.value.take().is_some() {
                pending.extend(current.dependents.iter().cloned());
                stale.insert(key);
            }
        }
        stale
    }

    /// The cycle that making `node` depend on `deps` would close, if any: `node`, one of `deps`,
    /// …, `node`, each key depending on the next.
    fn cycle_through(&self, node: &K, deps: &BTreeSet<K>) -> Option<Vec<K>> {
        if deps.contains(node) {
            return Some(vec![node.clone(), node.clone()]);
        }
        // Only a node with dependents can be reached by following dependencies.
        if self.nodes.get(node).is_none_or(|n| n.dependents.is_empty()) {
            return None;
        }
        // Breadth-first search from `deps` along dependencies, recording who reached whom.
        let mut reached_from: BTreeMap<K, Option<K>> =
            deps.iter().map(|dep| (dep.clone(), None)).collect();
        let mut queue: std::collections::VecDeque<K> = deps.iter().cloned().collect();
        while let Some(key) = queue.pop_front() {
            for next in self.dependencies(&key) {
                if next == node {
                    return Some(Self::cycle_path(node, key, &reached_from));
                }
                if !reached_from.contains_key(next) {
                    reached_from.insert(next.clone(), Some(key.clone()));
                    queue.push_back(next.clone());
                }
            }
        }
        None
    }

    /// Rebuilds `node → … → last → node` from the search's back links.
    fn cycle_path(node: &K, last: K, reached_from: &BTreeMap<K, Option<K>>) -> Vec<K> {
        let mut path = vec![node.clone()];
        let mut current = Some(last);
        while let Some(key) = current {
            current = reached_from.get(&key).cloned().flatten();
            path.push(key);
        }
        path[1..].reverse();
        path.push(node.clone());
        path
    }
}

#[cfg(test)]
mod tests;
