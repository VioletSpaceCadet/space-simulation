use std::collections::{HashSet, VecDeque};
use crate::{NodeId, SolarSystemDef};

/// Returns the number of hops on the shortest undirected path between two nodes,
/// or `None` if no path exists. Returns `Some(0)` when `from == to`.
pub fn shortest_hop_count(from: &NodeId, to: &NodeId, solar_system: &SolarSystemDef) -> Option<u64> {
    if from == to {
        return Some(0);
    }
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back((from.clone(), 0u64));
    visited.insert(from.clone());
    while let Some((node, dist)) = queue.pop_front() {
        for (a, b) in &solar_system.edges {
            let neighbor = if a == &node {
                Some(b)
            } else if b == &node {
                Some(a)
            } else {
                None
            };
            if let Some(neighbor) = neighbor {
                if neighbor == to {
                    return Some(dist + 1);
                }
                if visited.insert(neighbor.clone()) {
                    queue.push_back((neighbor.clone(), dist + 1));
                }
            }
        }
    }
    None
}
