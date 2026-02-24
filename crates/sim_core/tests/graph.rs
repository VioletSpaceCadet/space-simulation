use sim_core::{shortest_hop_count, NodeDef, NodeId, SolarSystemDef};

fn node(id: &str) -> NodeDef {
    NodeDef {
        id: NodeId(id.to_string()),
        name: id.to_string(),
    }
}

fn nid(id: &str) -> NodeId {
    NodeId(id.to_string())
}

#[test]
fn test_same_node_returns_zero() {
    let solar = SolarSystemDef {
        nodes: vec![node("A")],
        edges: vec![],
    };
    assert_eq!(shortest_hop_count(&nid("A"), &nid("A"), &solar), Some(0));
}

#[test]
fn test_direct_neighbors_returns_one() {
    let solar = SolarSystemDef {
        nodes: vec![node("A"), node("B")],
        edges: vec![(nid("A"), nid("B"))],
    };
    assert_eq!(shortest_hop_count(&nid("A"), &nid("B"), &solar), Some(1));
}

#[test]
fn test_two_hops() {
    let solar = SolarSystemDef {
        nodes: vec![node("A"), node("B"), node("C")],
        edges: vec![(nid("A"), nid("B")), (nid("B"), nid("C"))],
    };
    assert_eq!(shortest_hop_count(&nid("A"), &nid("C"), &solar), Some(2));
}

#[test]
fn test_disconnected_returns_none() {
    let solar = SolarSystemDef {
        nodes: vec![node("A"), node("B"), node("C")],
        edges: vec![(nid("A"), nid("B"))],
    };
    assert_eq!(shortest_hop_count(&nid("A"), &nid("C"), &solar), None);
}

#[test]
fn test_single_node_no_edges() {
    let solar = SolarSystemDef {
        nodes: vec![node("A")],
        edges: vec![],
    };
    assert_eq!(shortest_hop_count(&nid("A"), &nid("A"), &solar), Some(0));
}

#[test]
fn test_bidirectional() {
    let solar = SolarSystemDef {
        nodes: vec![node("A"), node("B"), node("C")],
        edges: vec![(nid("A"), nid("B")), (nid("B"), nid("C"))],
    };
    let ab = shortest_hop_count(&nid("A"), &nid("B"), &solar);
    let ba = shortest_hop_count(&nid("B"), &nid("A"), &solar);
    assert_eq!(ab, ba);
    let ac = shortest_hop_count(&nid("A"), &nid("C"), &solar);
    let ca = shortest_hop_count(&nid("C"), &nid("A"), &solar);
    assert_eq!(ac, ca);
}
