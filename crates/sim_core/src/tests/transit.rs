use super::*;

#[test]
#[allow(clippy::too_many_lines)]
fn transit_moves_ship_and_starts_next_task() {
    let mut content = test_content();
    let node_a = NodeId("node_a".to_string());
    let node_b = NodeId("node_b".to_string());
    content.solar_system = SolarSystemDef {
        bodies: vec![],
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
    content.constants.survey_scan_ticks = 1;

    let pos_a = Position {
        parent_body: BodyId("body_a".to_string()),
        radius_au_um: RadiusAuMicro(0),
        angle_mdeg: AngleMilliDeg(0),
    };
    let pos_b = Position {
        parent_body: BodyId("body_b".to_string()),
        radius_au_um: RadiusAuMicro(1_000_000),
        angle_mdeg: AngleMilliDeg(0),
    };
    let body_b = BodyId("body_b".to_string());

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
            position: pos_b.clone(),
            template_id: "tmpl_iron_rich".to_string(),
        }],
        asteroids: std::collections::BTreeMap::new(),
        ships: [(
            ship_id.clone(),
            ShipState {
                id: ship_id.clone(),
                position: pos_a.clone(),
                owner: owner.clone(),
                inventory: vec![],
                cargo_capacity_m3: 20.0,
                task: None,
                speed_ticks_per_au: None,
                modifiers: crate::modifiers::ModifierSet::default(),
                hull_id: HullId("hull_general_purpose".to_string()),
                fitted_modules: vec![],
                propellant_kg: 0.0,
                propellant_capacity_kg: 0.0,
            },
        )]
        .into_iter()
        .collect(),
        stations: [(
            station_id.clone(),
            StationState {
                id: station_id,
                position: pos_a.clone(),
                inventory: vec![],
                cargo_capacity_m3: 10_000.0,
                power_available_per_tick: 100.0,
                modules: vec![],
                modifiers: crate::modifiers::ModifierSet::default(),
                power: PowerState::default(),
                cached_inventory_volume_m3: None,
                module_type_index: crate::ModuleTypeIndex::default(),
                power_budget_cache: crate::PowerBudgetCache::default(),
            },
        )]
        .into_iter()
        .collect(),
        research: ResearchState {
            unlocked: std::collections::HashSet::new(),
            data_pool: AHashMap::default(),
            evidence: AHashMap::default(),
            action_counts: AHashMap::default(),
        },
        balance: 0.0,
        export_revenue_total: 0.0,
        export_count: 0,
        counters: Counters {
            next_event_id: 0,
            next_command_id: 0,
            next_asteroid_id: 0,
            next_lot_id: 0,
            next_module_instance_id: 0,
        },
        modifiers: crate::modifiers::ModifierSet::default(),
        events: crate::sim_events::SimEventState::default(),
        body_cache: AHashMap::default(),
    };

    let mut rng = ChaCha8Rng::seed_from_u64(0);

    let transit_cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: owner,
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::AssignShipTask {
            ship_id: ship_id.clone(),
            task_kind: TaskKind::Transit {
                destination: pos_b.clone(),
                total_ticks: 5,
                then: Box::new(TaskKind::Survey {
                    site: site_id.clone(),
                }),
            },
        },
    };

    tick(&mut state, &[transit_cmd], &content, &mut rng, None);
    assert_eq!(
        state.ships[&ship_id].position, pos_a,
        "ship still at origin during transit"
    );

    for _ in 1..5 {
        tick(&mut state, &[], &content, &mut rng, None);
    }
    assert_eq!(
        state.ships[&ship_id].position, pos_a,
        "ship still in transit"
    );

    let events = tick(&mut state, &[], &content, &mut rng, None);
    assert_eq!(
        state.ships[&ship_id].position, pos_b,
        "ship arrived at destination"
    );
    assert!(
        events.iter().any(
            |e| matches!(&e.event, Event::ShipArrived { ref position, .. } if position.parent_body == body_b)
        ),
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

    let events = tick(&mut state, &[], &content, &mut rng, None);
    assert!(
        events
            .iter()
            .any(|e| matches!(e.event, Event::AsteroidDiscovered { .. })),
        "AsteroidDiscovered after survey completes"
    );

    // Transit should have generated TransitData
    let transit_data = state
        .research
        .data_pool
        .get(&DataKind::TransitData)
        .copied()
        .unwrap_or(0.0);
    assert!(
        transit_data > 0.0,
        "TransitData should accumulate after transit completion, got {transit_data}"
    );
    assert_eq!(
        state
            .research
            .action_counts
            .get("transit")
            .copied()
            .unwrap_or(0),
        1,
        "transit action counter should increment"
    );
}

#[test]
fn transit_generates_transit_data_with_diminishing_returns() {
    let mut content = test_content();
    let node_a = NodeId("node_a".to_string());
    let node_b = NodeId("node_b".to_string());
    content.solar_system = SolarSystemDef {
        bodies: vec![],
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

    let pos_a = Position {
        parent_body: BodyId("body_a".to_string()),
        radius_au_um: RadiusAuMicro(0),
        angle_mdeg: AngleMilliDeg(0),
    };
    let pos_b = Position {
        parent_body: BodyId("body_b".to_string()),
        radius_au_um: RadiusAuMicro(1_000_000),
        angle_mdeg: AngleMilliDeg(0),
    };

    let ship_id = ShipId("ship_0001".to_string());
    let owner = PrincipalId("principal_autopilot".to_string());
    let station_id = StationId("station_test".to_string());

    let mut state = GameState {
        meta: MetaState {
            tick: 0,
            seed: 0,
            schema_version: 1,
            content_version: "test".to_string(),
        },
        scan_sites: vec![],
        asteroids: std::collections::BTreeMap::new(),
        ships: [(
            ship_id.clone(),
            ShipState {
                id: ship_id.clone(),
                position: pos_a.clone(),
                owner: owner.clone(),
                inventory: vec![],
                cargo_capacity_m3: 20.0,
                task: None,
                speed_ticks_per_au: None,
                modifiers: crate::modifiers::ModifierSet::default(),
                hull_id: HullId("hull_general_purpose".to_string()),
                fitted_modules: vec![],
                propellant_kg: 0.0,
                propellant_capacity_kg: 0.0,
            },
        )]
        .into_iter()
        .collect(),
        stations: [(
            station_id.clone(),
            StationState {
                id: station_id,
                position: pos_a.clone(),
                inventory: vec![],
                cargo_capacity_m3: 10_000.0,
                power_available_per_tick: 100.0,
                modules: vec![],
                modifiers: crate::modifiers::ModifierSet::default(),
                power: PowerState::default(),
                cached_inventory_volume_m3: None,
                module_type_index: crate::ModuleTypeIndex::default(),
                power_budget_cache: crate::PowerBudgetCache::default(),
            },
        )]
        .into_iter()
        .collect(),
        research: ResearchState {
            unlocked: std::collections::HashSet::new(),
            data_pool: AHashMap::default(),
            evidence: AHashMap::default(),
            action_counts: AHashMap::default(),
        },
        balance: 0.0,
        export_revenue_total: 0.0,
        export_count: 0,
        counters: Counters {
            next_event_id: 0,
            next_command_id: 0,
            next_asteroid_id: 0,
            next_lot_id: 0,
            next_module_instance_id: 0,
        },
        modifiers: crate::modifiers::ModifierSet::default(),
        events: crate::sim_events::SimEventState::default(),
        body_cache: AHashMap::default(),
    };

    let mut rng = ChaCha8Rng::seed_from_u64(42);

    // First transit: Idle as follow-on
    let transit_cmd = CommandEnvelope {
        id: CommandId(0),
        issued_by: owner.clone(),
        issued_tick: 0,
        execute_at_tick: 0,
        command: Command::AssignShipTask {
            ship_id: ship_id.clone(),
            task_kind: TaskKind::Transit {
                destination: pos_b.clone(),
                total_ticks: 1,
                then: Box::new(TaskKind::Idle),
            },
        },
    };

    tick(&mut state, &[transit_cmd], &content, &mut rng, None);
    let events = tick(&mut state, &[], &content, &mut rng, None);
    assert!(events
        .iter()
        .any(|e| matches!(&e.event, Event::ShipArrived { .. })));

    let first_transit_data = state
        .research
        .data_pool
        .get(&DataKind::TransitData)
        .copied()
        .unwrap_or(0.0);
    assert!(
        first_transit_data > 0.0,
        "first transit should generate data"
    );

    // Second transit back: should generate less data (diminishing returns)
    let transit_back = CommandEnvelope {
        id: CommandId(0),
        issued_by: owner,
        issued_tick: state.meta.tick,
        execute_at_tick: state.meta.tick,
        command: Command::AssignShipTask {
            ship_id: ship_id.clone(),
            task_kind: TaskKind::Transit {
                destination: pos_a,
                total_ticks: 1,
                then: Box::new(TaskKind::Idle),
            },
        },
    };

    tick(&mut state, &[transit_back], &content, &mut rng, None);
    tick(&mut state, &[], &content, &mut rng, None);

    let total_transit_data = state
        .research
        .data_pool
        .get(&DataKind::TransitData)
        .copied()
        .unwrap_or(0.0);
    let second_amount = total_transit_data - first_transit_data;

    assert!(
        second_amount > 0.0,
        "second transit should still generate some data"
    );
    assert!(
        second_amount < first_transit_data,
        "second transit should generate less than first (diminishing returns): \
         first={first_transit_data}, second={second_amount}"
    );
    assert_eq!(
        state.research.action_counts["transit"], 2,
        "transit action counter should be 2 after two transits"
    );
}

#[test]
fn ship_ticks_per_au_uses_per_ship_override() {
    let ship_default = ShipState {
        id: ShipId("ship_default".to_string()),
        position: test_position(),
        owner: PrincipalId("test".to_string()),
        inventory: vec![],
        cargo_capacity_m3: 20.0,
        task: None,
        speed_ticks_per_au: None,
        modifiers: crate::modifiers::ModifierSet::default(),
        hull_id: HullId("hull_general_purpose".to_string()),
        fitted_modules: vec![],
        propellant_kg: 0.0,
        propellant_capacity_kg: 0.0,
    };
    let ship_fast = ShipState {
        id: ShipId("ship_fast".to_string()),
        position: test_position(),
        owner: PrincipalId("test".to_string()),
        inventory: vec![],
        cargo_capacity_m3: 20.0,
        task: None,
        speed_ticks_per_au: Some(1000),
        modifiers: crate::modifiers::ModifierSet::default(),
        hull_id: HullId("hull_general_purpose".to_string()),
        fitted_modules: vec![],
        propellant_kg: 0.0,
        propellant_capacity_kg: 0.0,
    };
    let ship_slow = ShipState {
        id: ShipId("ship_slow".to_string()),
        position: test_position(),
        owner: PrincipalId("test".to_string()),
        inventory: vec![],
        cargo_capacity_m3: 20.0,
        task: None,
        speed_ticks_per_au: Some(5000),
        modifiers: crate::modifiers::ModifierSet::default(),
        hull_id: HullId("hull_general_purpose".to_string()),
        fitted_modules: vec![],
        propellant_kg: 0.0,
        propellant_capacity_kg: 0.0,
    };

    let global = 2133;
    assert_eq!(
        ship_default.ticks_per_au(global),
        2133,
        "None → global default"
    );
    assert_eq!(
        ship_fast.ticks_per_au(global),
        1000,
        "fast ship uses override"
    );
    assert_eq!(
        ship_slow.ticks_per_au(global),
        5000,
        "slow ship uses override"
    );

    // Different speeds produce different travel times for the same distance.
    let a = AbsolutePos {
        x_au_um: 0,
        y_au_um: 0,
    };
    let b = AbsolutePos {
        x_au_um: 2_000_000,
        y_au_um: 0,
    };
    let min_transit = 1;
    let fast_ticks = travel_ticks(a, b, ship_fast.ticks_per_au(global), min_transit);
    let slow_ticks = travel_ticks(a, b, ship_slow.ticks_per_au(global), min_transit);
    assert!(
        fast_ticks < slow_ticks,
        "fast ship should arrive sooner: fast={fast_ticks}, slow={slow_ticks}"
    );
}
