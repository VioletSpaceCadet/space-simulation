//! `sim_core` — deterministic simulation tick.
//!
//! No IO, no network. All randomness via the passed-in Rng.

mod engine;
mod graph;
mod research;
mod tasks;
mod types;

pub use engine::tick;
pub use graph::shortest_hop_count;
pub use tasks::{cargo_volume_used, mine_duration};
pub use types::*;

pub(crate) fn emit(counters: &mut Counters, tick: u64, event: Event) -> EventEnvelope {
    let id = EventId(format!("evt_{:06}", counters.next_event_id));
    counters.next_event_id += 1;
    EventEnvelope { id, tick, event }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use std::collections::{HashMap, HashSet};

    // --- Test helpers -------------------------------------------------------

    fn test_content() -> GameContent {
        GameContent {
            content_version: "test".to_string(),
            techs: vec![TechDef {
                id: TechId("tech_deep_scan_v1".to_string()),
                name: "Deep Scan v1".to_string(),
                prereqs: vec![],
                accepted_data: vec![DataKind::ScanData],
                difficulty: 10.0,
                effects: vec![
                    TechEffect::EnableDeepScan,
                    // sigma=0: mapped composition matches true composition exactly
                    TechEffect::DeepScanCompositionNoise { sigma: 0.0 },
                ],
            }],
            solar_system: SolarSystemDef {
                nodes: vec![NodeDef {
                    id: NodeId("node_test".to_string()),
                    name: "Test Node".to_string(),
                }],
                edges: vec![],
            },
            asteroid_templates: vec![AsteroidTemplateDef {
                id: "tmpl_iron_rich".to_string(),
                anomaly_tags: vec![AnomalyTag::IronRich],
                composition_ranges: HashMap::from([
                    // Fixed ranges so true_composition is deterministic.
                    ("Fe".to_string(), (0.7, 0.7)),
                    ("Si".to_string(), (0.3, 0.3)),
                ]),
            }],
            elements: vec![
                ElementDef {
                    id: "Fe".to_string(),
                    density_kg_per_m3: 7874.0,
                    display_name: "Iron".to_string(),
                },
                ElementDef {
                    id: "Si".to_string(),
                    density_kg_per_m3: 2329.0,
                    display_name: "Silicon".to_string(),
                },
            ],
            constants: Constants {
                survey_scan_ticks: 1,
                deep_scan_ticks: 1,
                travel_ticks_per_hop: 1,
                survey_scan_data_amount: 5.0,
                survey_scan_data_quality: 1.0,
                deep_scan_data_amount: 15.0,
                deep_scan_data_quality: 1.2,
                // Always detect tags so tests are predictable.
                survey_tag_detection_probability: 1.0,
                asteroid_count_per_template: 1,
                asteroid_mass_min_kg: 500.0, // fixed range so tests are deterministic
                asteroid_mass_max_kg: 500.0,
                ship_cargo_capacity_m3: 20.0,
                station_cargo_capacity_m3: 10_000.0,
                station_compute_units_total: 10,
                station_power_per_compute_unit_per_tick: 1.0,
                station_efficiency: 1.0,
                station_power_available_per_tick: 100.0,
                mining_rate_kg_per_tick: 50.0,
                deposit_ticks: 1,   // fast for tests
            },
        }
    }

    fn test_state(content: &GameContent) -> GameState {
        let node_id = NodeId("node_test".to_string());
        let ship_id = ShipId("ship_0001".to_string());
        let station_id = StationId("station_earth_orbit".to_string());
        let owner = PrincipalId("principal_autopilot".to_string());

        GameState {
            meta: MetaState {
                tick: 0,
                seed: 42,
                schema_version: 1,
                content_version: content.content_version.clone(),
            },
            scan_sites: vec![ScanSite {
                id: SiteId("site_0001".to_string()),
                node: node_id.clone(),
                template_id: "tmpl_iron_rich".to_string(),
            }],
            asteroids: HashMap::new(),
            ships: HashMap::from([(
                ship_id.clone(),
                ShipState {
                    id: ship_id,
                    location_node: node_id.clone(),
                    owner,
                    cargo: HashMap::new(),
                    cargo_capacity_m3: 20.0,
                    task: None,
                },
            )]),
            stations: HashMap::from([(
                station_id.clone(),
                StationState {
                    id: station_id,
                    location_node: node_id,
                    cargo: HashMap::new(),
                    cargo_capacity_m3: 10_000.0,
                    power_available_per_tick: 100.0,
                    facilities: FacilitiesState {
                        compute_units_total: 10,
                        power_per_compute_unit_per_tick: 1.0,
                        efficiency: 1.0,
                    },
                },
            )]),
            research: ResearchState {
                unlocked: HashSet::new(),
                data_pool: HashMap::new(),
                evidence: HashMap::new(),
            },
            counters: Counters {
                next_event_id: 0,
                next_command_id: 0,
                next_asteroid_id: 0,
            },
        }
    }

    fn make_rng() -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(42)
    }

    fn survey_command(state: &GameState) -> CommandEnvelope {
        let ship_id = ShipId("ship_0001".to_string());
        let owner = state.ships[&ship_id].owner.clone();
        CommandEnvelope {
            id: CommandId("cmd_000001".to_string()),
            issued_by: owner,
            issued_tick: state.meta.tick,
            execute_at_tick: state.meta.tick,
            command: Command::AssignShipTask {
                ship_id,
                task_kind: TaskKind::Survey {
                    site: SiteId("site_0001".to_string()),
                },
            },
        }
    }

    // --- Command application ------------------------------------------------

    #[test]
    fn test_assign_survey_sets_task() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

        let ship = &state.ships[&ShipId("ship_0001".to_string())];
        assert!(
            matches!(&ship.task, Some(task) if matches!(task.kind, TaskKind::Survey { .. })),
            "ship should have a Survey task after command"
        );
    }

    #[test]
    fn test_assign_command_emits_task_started() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let cmd = survey_command(&state);
        let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

        assert!(
            events
                .iter()
                .any(|e| matches!(e.event, Event::TaskStarted { .. })),
            "TaskStarted event should be emitted"
        );
    }

    #[test]
    fn test_wrong_owner_command_is_dropped() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let ship_id = ShipId("ship_0001".to_string());
        let bad_command = CommandEnvelope {
            id: CommandId("cmd_000001".to_string()),
            issued_by: PrincipalId("principal_intruder".to_string()),
            issued_tick: 0,
            execute_at_tick: 0,
            command: Command::AssignShipTask {
                ship_id: ship_id.clone(),
                task_kind: TaskKind::Survey {
                    site: SiteId("site_0001".to_string()),
                },
            },
        };

        tick(
            &mut state,
            &[bad_command],
            &content,
            &mut rng,
            EventLevel::Normal,
        );

        let ship = &state.ships[&ship_id];
        assert!(
            ship.task.is_none(),
            "command from wrong owner should be silently dropped"
        );
    }

    #[test]
    fn test_future_command_not_applied_early() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let ship_id = ShipId("ship_0001".to_string());
        let future_command = CommandEnvelope {
            id: CommandId("cmd_000001".to_string()),
            issued_by: state.ships[&ship_id].owner.clone(),
            issued_tick: 0,
            execute_at_tick: 5, // scheduled for tick 5, not now
            command: Command::AssignShipTask {
                ship_id: ship_id.clone(),
                task_kind: TaskKind::Survey {
                    site: SiteId("site_0001".to_string()),
                },
            },
        };

        tick(
            &mut state,
            &[future_command],
            &content,
            &mut rng,
            EventLevel::Normal,
        );

        let ship = &state.ships[&ship_id];
        assert!(
            ship.task.is_none(),
            "command scheduled for a future tick should not apply yet"
        );
    }

    // --- Survey scan --------------------------------------------------------

    #[test]
    fn test_survey_creates_asteroid_on_completion() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        // Tick 0: assign task (eta_tick = 0 + 1 = 1).
        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        assert!(
            state.asteroids.is_empty(),
            "asteroid should not exist before task completes"
        );

        // Tick 1: task resolves.
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
        assert_eq!(
            state.asteroids.len(),
            1,
            "asteroid should be created on survey completion"
        );
        assert!(state.scan_sites.is_empty(), "scan site should be consumed");
    }

    #[test]
    fn test_survey_emits_expected_events() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        let completion_events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let event_kinds: Vec<&str> = completion_events
            .iter()
            .map(|e| match &e.event {
                Event::AsteroidDiscovered { .. } => "AsteroidDiscovered",
                Event::ScanResult { .. } => "ScanResult",
                Event::DataGenerated { .. } => "DataGenerated",
                Event::TaskCompleted { .. } => "TaskCompleted",
                _ => "other",
            })
            .collect();

        assert!(event_kinds.contains(&"AsteroidDiscovered"));
        assert!(event_kinds.contains(&"ScanResult"));
        assert!(event_kinds.contains(&"DataGenerated"));
        assert!(event_kinds.contains(&"TaskCompleted"));
    }

    #[test]
    fn test_survey_detects_tags_with_prob_one() {
        // test_content sets detection probability to 1.0, so all tags must be detected.
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let tags = events
            .iter()
            .find_map(|e| match &e.event {
                Event::ScanResult { tags, .. } => Some(tags.clone()),
                _ => None,
            })
            .expect("ScanResult should be emitted");

        assert!(
            tags.iter().any(|(tag, _)| *tag == AnomalyTag::IronRich),
            "IronRich tag should be detected when probability is 1.0"
        );
    }

    #[test]
    fn test_survey_accumulates_scan_data() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let scan_data = state
            .research
            .data_pool
            .get(&DataKind::ScanData)
            .copied()
            .unwrap_or(0.0);
        assert!(
            scan_data > 0.0,
            "ScanData should accumulate in the data pool after a survey"
        );
    }

    #[test]
    fn test_ship_returns_to_idle_after_survey() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let ship = &state.ships[&ShipId("ship_0001".to_string())];
        assert!(
            matches!(&ship.task, Some(task) if matches!(task.kind, TaskKind::Idle)),
            "ship should return to Idle after survey completes"
        );
    }

    // --- Deep scan ----------------------------------------------------------

    #[test]
    fn test_deep_scan_blocked_without_tech() {
        // Use very high difficulty so research cannot unlock the tech in the setup ticks.
        let mut content = test_content();
        content.techs[0].difficulty = 1_000_000.0;
        let mut state = test_state(&content);
        let mut rng = make_rng();

        // Create an asteroid first.
        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let asteroid_id = state.asteroids.keys().next().unwrap().clone();
        let ship_id = ShipId("ship_0001".to_string());
        let owner = state.ships[&ship_id].owner.clone();

        let deep_cmd = CommandEnvelope {
            id: CommandId("cmd_000002".to_string()),
            issued_by: owner,
            issued_tick: state.meta.tick,
            execute_at_tick: state.meta.tick,
            command: Command::AssignShipTask {
                ship_id: ship_id.clone(),
                task_kind: TaskKind::DeepScan {
                    asteroid: asteroid_id,
                },
            },
        };

        tick(
            &mut state,
            &[deep_cmd],
            &content,
            &mut rng,
            EventLevel::Normal,
        );

        let ship = &state.ships[&ship_id];
        assert!(
            !matches!(&ship.task, Some(task) if matches!(task.kind, TaskKind::DeepScan { .. })),
            "DeepScan command should be dropped when tech is not unlocked"
        );
    }

    #[test]
    fn test_deep_scan_maps_composition() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        // Unlock the tech directly.
        state
            .research
            .unlocked
            .insert(TechId("tech_deep_scan_v1".to_string()));

        // Survey to create an asteroid.
        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let asteroid_id = state.asteroids.keys().next().unwrap().clone();
        assert!(
            state.asteroids[&asteroid_id]
                .knowledge
                .composition
                .is_none(),
            "composition should be unknown before deep scan"
        );

        let ship_id = ShipId("ship_0001".to_string());
        let owner = state.ships[&ship_id].owner.clone();
        let deep_cmd = CommandEnvelope {
            id: CommandId("cmd_000002".to_string()),
            issued_by: owner,
            issued_tick: state.meta.tick,
            execute_at_tick: state.meta.tick,
            command: Command::AssignShipTask {
                ship_id,
                task_kind: TaskKind::DeepScan {
                    asteroid: asteroid_id.clone(),
                },
            },
        };

        tick(
            &mut state,
            &[deep_cmd],
            &content,
            &mut rng,
            EventLevel::Normal,
        );
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let composition = state.asteroids[&asteroid_id].knowledge.composition.as_ref();
        assert!(
            composition.is_some(),
            "composition should be mapped after deep scan"
        );
    }

    #[test]
    fn test_deep_scan_composition_matches_truth_with_zero_sigma() {
        // test_content sets sigma=0.0, so mapped should exactly equal true.
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        state
            .research
            .unlocked
            .insert(TechId("tech_deep_scan_v1".to_string()));

        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let asteroid_id = state.asteroids.keys().next().unwrap().clone();
        let ship_id = ShipId("ship_0001".to_string());
        let owner = state.ships[&ship_id].owner.clone();
        let deep_cmd = CommandEnvelope {
            id: CommandId("cmd_000002".to_string()),
            issued_by: owner,
            issued_tick: state.meta.tick,
            execute_at_tick: state.meta.tick,
            command: Command::AssignShipTask {
                ship_id,
                task_kind: TaskKind::DeepScan {
                    asteroid: asteroid_id.clone(),
                },
            },
        };
        tick(
            &mut state,
            &[deep_cmd],
            &content,
            &mut rng,
            EventLevel::Normal,
        );
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let asteroid = &state.asteroids[&asteroid_id];
        let mapped = asteroid.knowledge.composition.as_ref().unwrap();
        for (element, &true_val) in &asteroid.true_composition {
            let mapped_val = mapped.get(element).copied().unwrap_or(0.0);
            assert!(
                (mapped_val - true_val).abs() < 1e-5,
                "mapped {element} ({mapped_val}) should equal true value ({true_val}) with sigma=0"
            );
        }
    }

    // --- Research -----------------------------------------------------------

    #[test]
    fn test_research_evidence_accumulates_each_tick() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(999); // seed unlikely to unlock tech immediately

        let tech_id = TechId("tech_deep_scan_v1".to_string());

        // Run enough ticks that evidence grows, but use a known-safe seed.
        // We just want to observe accumulation, not guarantee no unlock.
        // Instead, set difficulty very high so unlock is practically impossible.
        let mut high_difficulty_content = content.clone();
        high_difficulty_content.techs[0].difficulty = 1_000_000.0;

        tick(
            &mut state,
            &[],
            &high_difficulty_content,
            &mut rng,
            EventLevel::Normal,
        );
        let evidence_t1 = *state.research.evidence.get(&tech_id).unwrap_or(&0.0);

        tick(
            &mut state,
            &[],
            &high_difficulty_content,
            &mut rng,
            EventLevel::Normal,
        );
        let evidence_t2 = *state.research.evidence.get(&tech_id).unwrap_or(&0.0);

        assert!(
            evidence_t1 > 0.0,
            "evidence should be positive after first tick"
        );
        assert!(
            evidence_t2 > evidence_t1,
            "evidence should increase each tick"
        );
    }

    #[test]
    fn test_research_emits_power_consumed() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        assert!(
            events
                .iter()
                .any(|e| matches!(e.event, Event::PowerConsumed { .. })),
            "PowerConsumed should be emitted each tick that research runs"
        );
    }

    #[test]
    fn test_research_power_amount_correct() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let power = events
            .iter()
            .find_map(|e| match &e.event {
                Event::PowerConsumed { amount, .. } => Some(*amount),
                _ => None,
            })
            .expect("PowerConsumed event should be present");

        // compute_units_total=10, power_per_unit=1.0 → 10.0
        assert!(
            (power - 10.0).abs() < 1e-5,
            "power consumed should equal compute_units_total * power_per_unit"
        );
    }

    #[test]
    fn test_research_prereq_blocks_evidence() {
        let mut content = test_content();
        content.techs[0].prereqs = vec![TechId("tech_not_yet_unlocked".to_string())];

        let mut state = test_state(&content);
        let mut rng = make_rng();

        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let tech_id = TechId("tech_deep_scan_v1".to_string());
        let evidence = state
            .research
            .evidence
            .get(&tech_id)
            .copied()
            .unwrap_or(0.0);
        assert_eq!(
            evidence, 0.0,
            "evidence should not accumulate when prerequisites are unmet"
        );
    }

    #[test]
    fn test_research_no_power_consumed_when_no_eligible_techs() {
        let content = test_content();
        let mut state = test_state(&content);
        // Pre-unlock the only tech so nothing is eligible.
        state
            .research
            .unlocked
            .insert(TechId("tech_deep_scan_v1".to_string()));
        let mut rng = make_rng();

        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        assert!(
            !events
                .iter()
                .any(|e| matches!(e.event, Event::PowerConsumed { .. })),
            "no PowerConsumed when all techs are already unlocked"
        );
    }

    #[test]
    fn test_tech_unlocks_eventually() {
        let content = test_content(); // difficulty=10, compute=10
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let tech_id = TechId("tech_deep_scan_v1".to_string());
        let mut unlocked_at = None;

        for tick_num in 0..500 {
            tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
            if state.research.unlocked.contains(&tech_id) {
                unlocked_at = Some(tick_num);
                break;
            }
        }

        assert!(unlocked_at.is_some(), "tech should unlock within 500 ticks");
    }

    #[test]
    fn test_tech_unlock_tick_is_deterministic() {
        let content = test_content();

        let unlock_tick = |seed: u64| -> Option<u64> {
            let mut state = test_state(&content);
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let tech_id = TechId("tech_deep_scan_v1".to_string());
            for _ in 0..500 {
                tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
                if state.research.unlocked.contains(&tech_id) {
                    return Some(state.meta.tick);
                }
            }
            None
        };

        assert_eq!(
            unlock_tick(42),
            unlock_tick(42),
            "same seed must produce the same unlock tick"
        );
    }

    #[test]
    fn test_debug_level_emits_research_roll_events() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Debug);

        assert!(
            events
                .iter()
                .any(|e| matches!(e.event, Event::ResearchRoll { .. })),
            "ResearchRoll events should be emitted at EventLevel::Debug"
        );
    }

    #[test]
    fn test_normal_level_suppresses_research_roll_events() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        assert!(
            !events
                .iter()
                .any(|e| matches!(e.event, Event::ResearchRoll { .. })),
            "ResearchRoll events should not be emitted at EventLevel::Normal"
        );
    }

    // --- Element definitions ------------------------------------------------

    #[test]
    fn test_content_has_element_densities() {
        let content = test_content();
        let fe = content
            .elements
            .iter()
            .find(|e| e.id == "Fe")
            .expect("Fe element must be defined in content");
        assert!(
            (fe.density_kg_per_m3 - 7874.0).abs() < 1.0,
            "Fe density should be ~7874 kg/m³"
        );
    }

    // --- Asteroid mass ------------------------------------------------------

    #[test]
    fn test_asteroid_has_mass_after_survey() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let asteroid = state.asteroids.values().next().unwrap();
        assert!(
            asteroid.mass_kg > 0.0,
            "asteroid must have positive mass after survey"
        );
    }

    #[test]
    fn test_asteroid_mass_within_range() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let asteroid = state.asteroids.values().next().unwrap();
        // test_content sets min=max=500.0 for determinism
        assert!(
            (asteroid.mass_kg - 500.0).abs() < 1e-3,
            "mass should be 500.0 in test content (fixed range)"
        );
    }

    // --- Determinism --------------------------------------------------------

    #[test]
    fn test_identical_seeds_produce_identical_event_logs() {
        let content = test_content();

        let run = |seed: u64| -> Vec<(String, u64)> {
            let mut state = test_state(&content);
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let mut log = Vec::new();

            let cmd = survey_command(&state);
            for i in 0..20u64 {
                let commands = if i == 0 {
                    std::slice::from_ref(&cmd)
                } else {
                    &[]
                };
                let events = tick(&mut state, commands, &content, &mut rng, EventLevel::Debug);
                for event in events {
                    log.push((event.id.0.clone(), event.tick));
                }
            }
            log
        };

        assert_eq!(
            run(42),
            run(42),
            "identical seeds must produce identical event logs"
        );
    }

    #[test]
    fn test_different_seeds_produce_different_results() {
        let content = test_content();

        let unlock_tick = |seed: u64| -> Option<u64> {
            let mut state = test_state(&content);
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let tech_id = TechId("tech_deep_scan_v1".to_string());
            for _ in 0..500 {
                tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
                if state.research.unlocked.contains(&tech_id) {
                    return Some(state.meta.tick);
                }
            }
            None
        };

        // Seeds 42 and 1234 should unlock at different ticks (very likely with this model).
        // If they happen to collide the test is a false failure — acceptable in practice.
        let tick_42 = unlock_tick(42);
        let tick_1234 = unlock_tick(1234);
        assert_ne!(
            tick_42, tick_1234,
            "different seeds should generally produce different results"
        );
    }

    // --- Mine ---------------------------------------------------------------

    // Helper: build a state with an already-surveyed asteroid (mass 500, 70% Fe / 30% Si).
    fn state_with_asteroid(content: &GameContent) -> (GameState, AsteroidId) {
        let mut state = test_state(content);
        let mut rng = make_rng();
        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], content, &mut rng, EventLevel::Normal);
        tick(&mut state, &[], content, &mut rng, EventLevel::Normal);
        let asteroid_id = state.asteroids.keys().next().unwrap().clone();
        (state, asteroid_id)
    }

    fn mine_command(
        state: &GameState,
        asteroid_id: &AsteroidId,
        _content: &GameContent,
    ) -> CommandEnvelope {
        let ship_id = ShipId("ship_0001".to_string());
        let ship = &state.ships[&ship_id];
        // Use a simple fixed duration for tests
        let duration_ticks = 10;
        CommandEnvelope {
            id: CommandId("cmd_mine_001".to_string()),
            issued_by: ship.owner.clone(),
            issued_tick: state.meta.tick,
            execute_at_tick: state.meta.tick,
            command: Command::AssignShipTask {
                ship_id,
                task_kind: TaskKind::Mine {
                    asteroid: asteroid_id.clone(),
                    duration_ticks,
                },
            },
        }
    }

    #[test]
    fn test_mine_emits_ore_mined_event() {
        let content = test_content();
        let (mut state, asteroid_id) = state_with_asteroid(&content);
        let mut rng = make_rng();

        let cmd = mine_command(&state, &asteroid_id, &content);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        // Fast forward to completion (duration_ticks=10, so task eta is 10 ticks from start)
        let completion_tick = state.meta.tick + 9;
        while state.meta.tick < completion_tick {
            tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
        }
        // Tick once more to hit the eta_tick and resolve the task
        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        assert!(
            events
                .iter()
                .any(|e| matches!(e.event, Event::OreMined { .. })),
            "OreMined event should be emitted when mining completes"
        );
    }

    #[test]
    fn test_mine_adds_ore_to_ship_cargo() {
        let content = test_content();
        let (mut state, asteroid_id) = state_with_asteroid(&content);
        let mut rng = make_rng();

        let ship_id = ShipId("ship_0001".to_string());
        assert!(state.ships[&ship_id].cargo.is_empty());

        let cmd = mine_command(&state, &asteroid_id, &content);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        let completion_tick = state.meta.tick + 10;
        while state.meta.tick <= completion_tick {
            tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
        }

        let cargo = &state.ships[&ship_id].cargo;
        assert!(
            !cargo.is_empty(),
            "ship cargo should not be empty after mining"
        );
        assert!(
            cargo.values().any(|&kg| kg > 0.0),
            "extracted mass must be positive"
        );
    }

    #[test]
    fn test_mine_reduces_asteroid_mass() {
        let content = test_content();
        let (mut state, asteroid_id) = state_with_asteroid(&content);
        let mut rng = make_rng();

        let original_mass = state.asteroids[&asteroid_id].mass_kg;
        let cmd = mine_command(&state, &asteroid_id, &content);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        let completion_tick = state.meta.tick + 10;
        while state.meta.tick <= completion_tick {
            tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
        }

        let remaining = state
            .asteroids
            .get(&asteroid_id)
            .map(|a| a.mass_kg)
            .unwrap_or(0.0);
        assert!(
            remaining < original_mass,
            "asteroid mass must decrease after mining"
        );
    }

    #[test]
    fn test_mine_removes_depleted_asteroid() {
        let mut content = test_content();
        content.constants.mining_rate_kg_per_tick = 1_000_000.0; // deplete in 1 tick
        let (mut state, asteroid_id) = state_with_asteroid(&content);
        let mut rng = make_rng();

        let cmd = mine_command(&state, &asteroid_id, &content);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        // Run to completion
        for _ in 0..11 {
            tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
        }

        assert!(
            !state.asteroids.contains_key(&asteroid_id),
            "fully mined asteroid should be removed from state"
        );
    }

    // --- Deposit ------------------------------------------------------------

    fn deposit_command(state: &GameState) -> CommandEnvelope {
        let ship_id = ShipId("ship_0001".to_string());
        let station_id = StationId("station_earth_orbit".to_string());
        let ship = &state.ships[&ship_id];
        CommandEnvelope {
            id: CommandId("cmd_deposit_001".to_string()),
            issued_by: ship.owner.clone(),
            issued_tick: state.meta.tick,
            execute_at_tick: state.meta.tick,
            command: Command::AssignShipTask {
                ship_id,
                task_kind: TaskKind::Deposit { station: station_id },
            },
        }
    }

    #[test]
    fn test_deposit_moves_cargo_to_station() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let ship_id = ShipId("ship_0001".to_string());
        state.ships.get_mut(&ship_id).unwrap().cargo.insert("Fe".to_string(), 100.0);

        let cmd = deposit_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let station_id = StationId("station_earth_orbit".to_string());
        let station_fe = state.stations[&station_id].cargo.get("Fe").copied().unwrap_or(0.0);
        assert!((station_fe - 100.0).abs() < 1e-3, "Fe should transfer to station");
    }

    #[test]
    fn test_deposit_clears_ship_cargo() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let ship_id = ShipId("ship_0001".to_string());
        state.ships.get_mut(&ship_id).unwrap().cargo.insert("Fe".to_string(), 100.0);

        let cmd = deposit_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let cargo = &state.ships[&ship_id].cargo;
        let total: f32 = cargo.values().sum();
        assert!(total < 1e-3, "ship cargo should be empty after deposit");
    }

    #[test]
    fn test_deposit_emits_ore_deposited_event() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let ship_id = ShipId("ship_0001".to_string());
        state.ships.get_mut(&ship_id).unwrap().cargo.insert("Fe".to_string(), 50.0);

        let cmd = deposit_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        assert!(
            events.iter().any(|e| matches!(e.event, Event::OreDeposited { .. })),
            "OreDeposited event should be emitted"
        );
    }

    // --- Cargo holds --------------------------------------------------------

    #[test]
    fn test_ship_starts_with_empty_cargo() {
        let content = test_content();
        let state = test_state(&content);
        let ship = state.ships.values().next().unwrap();
        assert!(ship.cargo.is_empty(), "ship cargo should be empty at start");
        assert!(
            (ship.cargo_capacity_m3 - 20.0).abs() < 1e-5,
            "ship capacity should be 20 m³"
        );
    }

    #[test]
    fn test_station_starts_with_empty_cargo() {
        let content = test_content();
        let state = test_state(&content);
        let station = state.stations.values().next().unwrap();
        assert!(
            station.cargo.is_empty(),
            "station cargo should be empty at start"
        );
        assert!(
            (station.cargo_capacity_m3 - 10_000.0).abs() < 1e-5,
            "station capacity should be 10,000 m³"
        );
    }

    // --- Transit ------------------------------------------------------------

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
                },
                NodeDef {
                    id: NodeId("b".to_string()),
                    name: "B".to_string(),
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
        // Undirected: reverse also works.
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
                },
                NodeDef {
                    id: NodeId("b".to_string()),
                    name: "B".to_string(),
                },
                NodeDef {
                    id: NodeId("c".to_string()),
                    name: "C".to_string(),
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
                },
                NodeDef {
                    id: NodeId("b".to_string()),
                    name: "B".to_string(),
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
    fn transit_moves_ship_and_starts_next_task() {
        // Two-node solar system; ship starts at node_a, site is at node_b.
        let mut content = test_content();
        let node_a = NodeId("node_a".to_string());
        let node_b = NodeId("node_b".to_string());
        content.solar_system = SolarSystemDef {
            nodes: vec![
                NodeDef {
                    id: node_a.clone(),
                    name: "A".to_string(),
                },
                NodeDef {
                    id: node_b.clone(),
                    name: "B".to_string(),
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
                    cargo: HashMap::new(),
                    cargo_capacity_m3: 20.0,
                    task: None,
                },
            )]),
            stations: HashMap::from([(
                station_id.clone(),
                StationState {
                    id: station_id,
                    location_node: node_a.clone(),
                    cargo: HashMap::new(),
                    cargo_capacity_m3: 10_000.0,
                    power_available_per_tick: 100.0,
                    facilities: FacilitiesState {
                        compute_units_total: 10,
                        power_per_compute_unit_per_tick: 1.0,
                        efficiency: 1.0,
                    },
                },
            )]),
            research: ResearchState {
                unlocked: std::collections::HashSet::new(),
                data_pool: HashMap::new(),
                evidence: HashMap::new(),
            },
            counters: Counters {
                next_event_id: 0,
                next_command_id: 0,
                next_asteroid_id: 0,
            },
        };

        let mut rng = ChaCha8Rng::seed_from_u64(0);

        // Assign a Transit task: 5 ticks to node_b, then Survey.
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

        // Tick 0: assign transit.
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

        // Ticks 1–4: transit in progress, ship still at node_a.
        for _ in 1..5 {
            tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
        }
        assert_eq!(
            state.ships[&ship_id].location_node, node_a,
            "ship still in transit"
        );

        // Tick 5: transit resolves → ship moves to node_b, survey starts.
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

        // Tick 6: survey resolves → asteroid discovered.
        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
        assert!(
            events
                .iter()
                .any(|e| matches!(e.event, Event::AsteroidDiscovered { .. })),
            "AsteroidDiscovered after survey completes"
        );
    }
}
