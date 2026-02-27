use super::*;

#[test]
fn test_shortest_hop_count_same_node() {
    let content = test_content();
    let node = NodeId("node_test".to_string());
    assert_eq!(
        shortest_hop_count(&node, &node, &content.solar_system),
        Some(0)
    );
}

#[test]
fn test_shortest_hop_count_adjacent() {
    let solar_system = SolarSystemDef {
        nodes: vec![
            NodeDef {
                id: NodeId("a".to_string()),
                name: "A".to_string(),
                solar_intensity: 1.0,
            },
            NodeDef {
                id: NodeId("b".to_string()),
                name: "B".to_string(),
                solar_intensity: 1.0,
            },
        ],
        edges: vec![(NodeId("a".to_string()), NodeId("b".to_string()))],
    };
    assert_eq!(
        shortest_hop_count(
            &NodeId("a".to_string()),
            &NodeId("b".to_string()),
            &solar_system
        ),
        Some(1)
    );
    assert_eq!(
        shortest_hop_count(
            &NodeId("b".to_string()),
            &NodeId("a".to_string()),
            &solar_system
        ),
        Some(1)
    );
}

#[test]
fn test_shortest_hop_count_two_hops() {
    let solar_system = SolarSystemDef {
        nodes: vec![
            NodeDef {
                id: NodeId("a".to_string()),
                name: "A".to_string(),
                solar_intensity: 1.0,
            },
            NodeDef {
                id: NodeId("b".to_string()),
                name: "B".to_string(),
                solar_intensity: 1.0,
            },
            NodeDef {
                id: NodeId("c".to_string()),
                name: "C".to_string(),
                solar_intensity: 1.0,
            },
        ],
        edges: vec![
            (NodeId("a".to_string()), NodeId("b".to_string())),
            (NodeId("b".to_string()), NodeId("c".to_string())),
        ],
    };
    assert_eq!(
        shortest_hop_count(
            &NodeId("a".to_string()),
            &NodeId("c".to_string()),
            &solar_system
        ),
        Some(2)
    );
}

#[test]
fn test_shortest_hop_count_no_path() {
    let solar_system = SolarSystemDef {
        nodes: vec![
            NodeDef {
                id: NodeId("a".to_string()),
                name: "A".to_string(),
                solar_intensity: 1.0,
            },
            NodeDef {
                id: NodeId("b".to_string()),
                name: "B".to_string(),
                solar_intensity: 1.0,
            },
        ],
        edges: vec![],
    };
    assert_eq!(
        shortest_hop_count(
            &NodeId("a".to_string()),
            &NodeId("b".to_string()),
            &solar_system
        ),
        None
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn transit_moves_ship_and_starts_next_task() {
    let mut content = test_content();
    let node_a = NodeId("node_a".to_string());
    let node_b = NodeId("node_b".to_string());
    content.solar_system = SolarSystemDef {
        nodes: vec![
            NodeDef {
                id: node_a.clone(),
                name: "A".to_string(),
                solar_intensity: 1.0,
            },
            NodeDef {
                id: node_b.clone(),
                name: "B".to_string(),
                solar_intensity: 1.0,
            },
        ],
        edges: vec![(node_a.clone(), node_b.clone())],
    };
    content.constants.travel_ticks_per_hop = 5;
    content.constants.survey_scan_ticks = 1;

    let ship_id = ShipId("ship_0001".to_string());
    let site_id = SiteId("site_0001".to_string());
    let owner = PrincipalId("principal_autopilot".to_string());
    let station_id = StationId("station_test".to_string());

    let mut state = GameState {
        meta: MetaState {
            tick: 0,
            seed: 0,
            schema_version: 1,
            content_version: "test".to_string(),
        },
        scan_sites: vec![ScanSite {
            id: site_id.clone(),
            node: node_b.clone(),
            template_id: "tmpl_iron_rich".to_string(),
        }],
        asteroids: HashMap::new(),
        ships: HashMap::from([(
            ship_id.clone(),
            ShipState {
                id: ship_id.clone(),
                location_node: node_a.clone(),
                owner: owner.clone(),
                inventory: vec![],
                cargo_capacity_m3: 20.0,
                task: None,
            },
        )]),
        stations: HashMap::from([(
            station_id.clone(),
            StationState {
                id: station_id,
                location_node: node_a.clone(),
                inventory: vec![],
                cargo_capacity_m3: 10_000.0,
                power_available_per_tick: 100.0,
                modules: vec![],
                power: PowerState::default(),
                cached_inventory_volume_m3: None,
            },
        )]),
        research: ResearchState {
            unlocked: std::collections::HashSet::new(),
            data_pool: HashMap::new(),
            evidence: HashMap::new(),
            action_counts: HashMap::new(),
        },
        balance: 0.0,
        counters: Counters {
            next_event_id: 0,
            next_command_id: 0,
            next_asteroid_id: 0,
            next_lot_id: 0,
            next_module_instance_id: 0,
        },
    };

    let mut rng = ChaCha8Rng::seed_from_u64(0);

    let transit_cmd = CommandEnvelope {
        id: CommandId("cmd_000000".to_string()),
        issued_by: owner,
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::AssignShipTask {
            ship_id: ship_id.clone(),
            task_kind: TaskKind::Transit {
                destination: node_b.clone(),
                total_ticks: 5,
                then: Box::new(TaskKind::Survey {
                    site: site_id.clone(),
                }),
            },
        },
    };

    tick(
        &mut state,
        &[transit_cmd],
        &content,
        &mut rng,
        EventLevel::Normal,
    );
    assert_eq!(
        state.ships[&ship_id].location_node, node_a,
        "ship still at origin during transit"
    );

    for _ in 1..5 {
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    }
    assert_eq!(
        state.ships[&ship_id].location_node, node_a,
        "ship still in transit"
    );

    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    assert_eq!(
        state.ships[&ship_id].location_node, node_b,
        "ship arrived at destination"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(&e.event, Event::ShipArrived { node, .. } if node == &node_b)),
        "ShipArrived event should be emitted"
    );
    let survey_started = events.iter().any(|e| {
        matches!(&e.event,
            Event::TaskStarted { task_kind, .. } if task_kind == "Survey"
        )
    });
    assert!(
        survey_started,
        "Survey task should start immediately after arrival"
    );

    let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::AsteroidDiscovered { .. })),
        "AsteroidDiscovered after survey completes"
    );
}
