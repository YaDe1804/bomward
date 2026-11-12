// SPDX-License-Identifier: AGPL-3.0-only

//! Why an added version is in the new document.
//!
//! The reverse map and the breadth-first search are each one pass over the
//! new graph. The search keeps a visited set, so a cycle ends.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::model::{DependencyRole, Document, GraphNode};

use super::{BoundedNodes, INTRODUCED_BY_LIMIT, PATH_FROM_ROOT_LIMIT, RootSnapshot};

pub(super) struct Provenance {
    dependents: BTreeMap<GraphNode, BTreeSet<GraphNode>>,
    parent: BTreeMap<GraphNode, GraphNode>,
    root: Option<GraphNode>,
}

impl Provenance {
    pub(super) fn from_document(document: &Document, root: Option<&RootSnapshot>) -> Self {
        let mut dependents: BTreeMap<GraphNode, BTreeSet<GraphNode>> = BTreeMap::new();
        for (from, targets) in &document.dependencies {
            for target in targets {
                dependents
                    .entry(target.clone())
                    .or_default()
                    .insert(from.clone());
            }
        }
        let root_node = root.map(|snapshot| GraphNode {
            key: snapshot.key.clone(),
            version: snapshot.version.clone(),
        });
        let parent = root_node
            .as_ref()
            .map_or_default(|node| shortest_parents(node, &document.dependencies));
        Self {
            dependents,
            parent,
            root: root_node,
        }
    }

    pub(super) fn introduced_by(&self, node: &GraphNode) -> BoundedNodes {
        let nodes = self
            .dependents
            .get(node)
            .map_or_default(|sources| sources.iter().cloned().collect::<Vec<_>>());
        bound(nodes, INTRODUCED_BY_LIMIT)
    }

    pub(super) fn path_from_root(&self, node: &GraphNode) -> Option<BoundedNodes> {
        let root = self.root.as_ref()?;
        if node == root || !self.parent.contains_key(node) {
            return None;
        }
        let mut reversed = Vec::new();
        let mut cursor = node.clone();
        reversed.push(cursor.clone());
        while &cursor != root {
            let previous = self.parent.get(&cursor)?.clone();
            reversed.push(previous.clone());
            cursor = previous;
            if reversed.len() > self.parent.len().saturating_add(1) {
                return None;
            }
        }
        reversed.reverse();
        Some(bound(reversed, PATH_FROM_ROOT_LIMIT))
    }
}

fn shortest_parents(
    root: &GraphNode,
    dependencies: &BTreeMap<GraphNode, BTreeSet<GraphNode>>,
) -> BTreeMap<GraphNode, GraphNode> {
    let mut parent = BTreeMap::new();
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();
    visited.insert(root.clone());
    queue.push_back(root.clone());
    while let Some(node) = queue.pop_front() {
        let Some(children) = dependencies.get(&node) else {
            continue;
        };
        for child in children {
            if visited.insert(child.clone()) {
                parent.insert(child.clone(), node.clone());
                queue.push_back(child.clone());
            }
        }
    }
    parent
}

pub(super) fn find_root(document: &Document) -> Option<RootSnapshot> {
    for (key, component) in &document.components {
        for (version, stored) in &component.versions {
            if stored.role == DependencyRole::Root {
                return Some(RootSnapshot {
                    key: key.clone(),
                    version: version.clone(),
                    role: stored.role,
                    attributes: stored.attributes.clone(),
                });
            }
        }
    }
    None
}

fn bound(nodes: Vec<GraphNode>, limit: usize) -> BoundedNodes {
    let total = nodes.len();
    let mut nodes = nodes;
    nodes.truncate(limit);
    BoundedNodes { nodes, total }
}
