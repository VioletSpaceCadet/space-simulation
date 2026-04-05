//! Milestone evaluation engine.
//!
//! Called at tick step 4.5 (after research, before events). Evaluates
//! content-driven milestone conditions against current game state and
//! metrics. Newly completed milestones are recorded in `ProgressionState`.

use crate::{
    GameContent, GameState, GrantRecord, MetricsSnapshot, MilestoneCondition, MilestoneDef,
};

/// Resolve a counter name to a value derived from game state.
///
/// Counter names map to computed properties that aren't in `MetricsSnapshot`
/// but can be derived from `GameState` directly.
fn resolve_counter(state: &GameState, content: &GameContent, counter: &str) -> Option<f64> {
    match counter {
        "asteroids_discovered" => Some(state.asteroids.len() as f64),
        "techs_unlocked" => Some(state.research.unlocked.len() as f64),
        "ships_built" => Some(state.ships.len() as f64),
        "assembler_runs" => {
            // Count total component items across all station + ground facility inventories
            let count: usize = state
                .stations
                .values()
                .flat_map(|s| s.core.inventory.iter())
                .chain(
                    state
                        .ground_facilities
                        .values()
                        .flat_map(|f| f.core.inventory.iter()),
                )
                .filter(|item| matches!(item, crate::InventoryItem::Component { .. }))
                .count();
            Some(count as f64)
        }
        "satellites_deployed" => {
            Some(state.satellites.values().filter(|s| s.enabled).count() as f64)
        }
        "comm_satellites" => Some(
            state
                .satellites
                .values()
                .filter(|s| s.enabled && s.satellite_type == "communication")
                .count() as f64,
        ),
        "survey_satellites" => Some(
            state
                .satellites
                .values()
                .filter(|s| s.enabled && s.satellite_type == "survey")
                .count() as f64,
        ),
        "science_satellites" => Some(
            state
                .satellites
                .values()
                .filter(|s| s.enabled && s.satellite_type == "science_platform")
                .count() as f64,
        ),
        "nav_satellites" => Some(
            state
                .satellites
                .values()
                .filter(|s| s.enabled && s.satellite_type == "navigation")
                .count() as f64,
        ),
        // Ground operations counters
        "total_raw_data" => {
            let total: f32 = state.research.data_pool.values().sum();
            Some(f64::from(total))
        }
        "asteroids_classified" => Some(
            state
                .asteroids
                .values()
                .filter(|a| a.knowledge.composition.is_some())
                .count() as f64,
        ),
        "rockets_in_inventory" => {
            let count: usize = state
                .stations
                .values()
                .flat_map(|s| s.core.inventory.iter())
                .chain(
                    state
                        .ground_facilities
                        .values()
                        .flat_map(|f| f.core.inventory.iter()),
                )
                .filter(|item| {
                    matches!(item, crate::InventoryItem::Component { component_id, .. }
                        if content.rocket_defs.contains_key(component_id.0.as_str()))
                })
                .count();
            Some(count as f64)
        }
        "total_launches" => {
            let count: u64 = state
                .ground_facilities
                .values()
                .flat_map(|f| f.core.modules.iter())
                .filter_map(|m| match &m.kind_state {
                    crate::ModuleKindState::LaunchPad(pad) => Some(pad.launches_count),
                    _ => None,
                })
                .sum();
            Some(count as f64)
        }
        "stations_deployed" => Some(state.counters.stations_deployed as f64),
        "reusable_landings" => Some(0.0), // Placeholder — VIO-560 deferred
        _ => None,
    }
}

/// Check whether a single milestone condition is met.
fn condition_met(
    cond: &MilestoneCondition,
    state: &GameState,
    content: &GameContent,
    metrics: Option<&MetricsSnapshot>,
) -> bool {
    match cond {
        MilestoneCondition::MetricAbove { field, threshold } => metrics
            .and_then(|m| m.get_field_f64(field))
            .is_some_and(|val| val >= *threshold),
        MilestoneCondition::CounterAbove { counter, threshold } => {
            resolve_counter(state, content, counter).is_some_and(|val| val >= *threshold)
        }
        MilestoneCondition::MilestoneCompleted { milestone_id } => {
            state.progression.is_milestone_completed(milestone_id)
        }
    }
}

/// Check whether any uncompleted milestone has a `MetricAbove` condition.
/// If not, we can skip the expensive `compute_metrics` call entirely.
fn needs_metrics(
    milestones: &[MilestoneDef],
    completed: &std::collections::BTreeSet<String>,
) -> bool {
    milestones.iter().any(|m| {
        !completed.contains(&m.id)
            && m.conditions
                .iter()
                .any(|c| matches!(c, MilestoneCondition::MetricAbove { .. }))
    })
}

/// Evaluate all milestones against current state. Returns IDs of newly completed milestones.
///
/// Milestones are evaluated in sorted order (by ID) for determinism.
/// Multiple milestones can complete on the same tick, and chained milestones
/// (condition: `MilestoneCompleted`) can trigger within the same evaluation
/// pass because completions are applied immediately.
///
/// Optimization: `compute_metrics` is only called if at least one uncompleted
/// milestone has a `MetricAbove` condition. Once all metric-based milestones
/// complete, evaluation uses only counters and completion checks (O(1) each).
pub fn evaluate_milestones(
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<crate::EventEnvelope>,
) -> Vec<String> {
    if content.milestones.is_empty()
        || state.progression.completed_milestones.len() >= content.milestones.len()
    {
        return Vec::new();
    }

    // Only compute metrics if at least one uncompleted milestone needs them.
    // This avoids a full station/module/inventory walk on ticks where all
    // remaining milestones use counters or completion checks only.
    let metrics = if needs_metrics(&content.milestones, &state.progression.completed_milestones) {
        Some(crate::metrics::compute_metrics(state, content))
    } else {
        None
    };

    // Sort milestones by ID for deterministic evaluation order
    let mut sorted: Vec<&MilestoneDef> = content.milestones.iter().collect();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));

    let mut newly_completed = Vec::new();

    // Multiple passes to handle chained milestones that depend on milestones
    // completing in the same tick. Cap at len() passes to prevent infinite loops.
    let max_passes = sorted.len();
    for _ in 0..max_passes {
        let mut any_completed_this_pass = false;

        for milestone in &sorted {
            if state.progression.is_milestone_completed(&milestone.id) {
                continue;
            }

            let all_met = milestone
                .conditions
                .iter()
                .all(|c| condition_met(c, state, content, metrics.as_ref()));

            if all_met {
                // Mark completed
                state
                    .progression
                    .completed_milestones
                    .insert(milestone.id.clone());
                newly_completed.push(milestone.id.clone());
                any_completed_this_pass = true;

                // Advance phase if specified
                if let Some(new_phase) = milestone.phase_advance {
                    if new_phase > state.progression.phase {
                        let from = state.progression.phase.to_string();
                        state.progression.phase = new_phase;
                        events.push(crate::emit(
                            &mut state.counters,
                            state.meta.tick,
                            crate::Event::PhaseAdvanced {
                                from_phase: from,
                                to_phase: new_phase.to_string(),
                            },
                        ));
                    }
                }

                // Apply grant
                if milestone.rewards.grant_amount > 0.0 {
                    state.progression.grant_history.push(GrantRecord {
                        milestone_id: milestone.id.clone(),
                        amount: milestone.rewards.grant_amount,
                        tick: state.meta.tick,
                    });
                    state.balance += milestone.rewards.grant_amount;
                    events.push(crate::emit(
                        &mut state.counters,
                        state.meta.tick,
                        crate::Event::GrantAwarded {
                            milestone_id: milestone.id.clone(),
                            amount: milestone.rewards.grant_amount,
                        },
                    ));
                }

                // Advance reputation
                state.progression.reputation += milestone.rewards.reputation;

                // Advance trade tier (only upgrades)
                if let Some(new_tier) = milestone.rewards.unlock_trade_tier {
                    if new_tier > state.progression.trade_tier {
                        state.progression.trade_tier = new_tier;
                    }
                }

                // Record zone and module unlocks
                for zone_id in &milestone.rewards.unlock_zone_ids {
                    state.progression.unlocked_zone_ids.insert(zone_id.clone());
                }
                for module_id in &milestone.rewards.unlock_module_ids {
                    state
                        .progression
                        .unlocked_module_ids
                        .insert(module_id.clone());
                }

                // Emit milestone reached event
                events.push(crate::emit(
                    &mut state.counters,
                    state.meta.tick,
                    crate::Event::MilestoneReached {
                        milestone_id: milestone.id.clone(),
                        milestone_name: milestone.name.clone(),
                    },
                ));
            }
        }

        if !any_completed_this_pass {
            break;
        }
    }

    newly_completed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{base_content, base_state};
    use crate::{GamePhase, MilestoneReward, TradeTier};

    fn test_milestone(id: &str, conditions: Vec<MilestoneCondition>) -> MilestoneDef {
        MilestoneDef {
            id: id.to_string(),
            name: id.to_string(),
            description: String::new(),
            conditions,
            rewards: MilestoneReward {
                grant_amount: 0.0,
                reputation: 0.0,
                unlock_trade_tier: None,
                unlock_zone_ids: vec![],
                unlock_module_ids: vec![],
            },
            phase_advance: None,
        }
    }

    #[test]
    fn milestone_triggers_when_metric_met() {
        let mut content = base_content();
        content.milestones = vec![test_milestone(
            "test_ore",
            vec![MilestoneCondition::MetricAbove {
                field: "total_ore_kg".to_string(),
                threshold: 10.0,
            }],
        )];

        let mut state = base_state(&content);
        // Add ore to meet the condition
        state
            .stations
            .values_mut()
            .next()
            .unwrap()
            .core
            .inventory
            .push(crate::InventoryItem::Ore {
                lot_id: crate::LotId("lot_1".to_string()),
                asteroid_id: crate::AsteroidId("a1".to_string()),
                kg: 50.0,
                composition: std::collections::HashMap::from([
                    ("Fe".to_string(), 0.7),
                    ("Si".to_string(), 0.3),
                ]),
            });

        let mut events = Vec::new();
        let completed = evaluate_milestones(&mut state, &content, &mut events);

        assert_eq!(completed, vec!["test_ore"]);
        assert!(state.progression.is_milestone_completed("test_ore"));
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn completed_milestone_does_not_retrigger() {
        let mut content = base_content();
        content.milestones = vec![test_milestone(
            "test_m",
            vec![MilestoneCondition::MetricAbove {
                field: "tick".to_string(),
                threshold: 0.0,
            }],
        )];

        let mut state = base_state(&content);

        let mut events = Vec::new();
        let first = evaluate_milestones(&mut state, &content, &mut events);
        assert_eq!(first, vec!["test_m"]);

        let second = evaluate_milestones(&mut state, &content, &mut events);
        assert!(second.is_empty());
        // Only 1 event total
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn chained_milestone_triggers_same_tick() {
        let mut content = base_content();
        content.milestones = vec![
            test_milestone(
                "m_first",
                vec![MilestoneCondition::MetricAbove {
                    field: "tick".to_string(),
                    threshold: 0.0,
                }],
            ),
            test_milestone(
                "m_second",
                vec![MilestoneCondition::MilestoneCompleted {
                    milestone_id: "m_first".to_string(),
                }],
            ),
        ];

        let mut state = base_state(&content);
        let mut events = Vec::new();
        let completed = evaluate_milestones(&mut state, &content, &mut events);

        assert_eq!(completed.len(), 2);
        assert!(state.progression.is_milestone_completed("m_first"));
        assert!(state.progression.is_milestone_completed("m_second"));
    }

    #[test]
    fn counter_above_resolves_asteroids() {
        let mut content = base_content();
        content.milestones = vec![test_milestone(
            "test_asteroids",
            vec![MilestoneCondition::CounterAbove {
                counter: "asteroids_discovered".to_string(),
                threshold: 1.0,
            }],
        )];

        let mut state = base_state(&content);
        assert!(state.asteroids.is_empty());

        // No asteroids yet — should not trigger
        let mut events = Vec::new();
        let completed = evaluate_milestones(&mut state, &content, &mut events);
        assert!(completed.is_empty());

        // Add an asteroid
        let asteroid_id = crate::AsteroidId("asteroid_1".to_string());
        state.asteroids.insert(
            asteroid_id.clone(),
            crate::AsteroidState {
                id: asteroid_id,
                position: crate::test_fixtures::test_position(),
                true_composition: std::collections::HashMap::from([("Fe".to_string(), 0.7)]),
                anomaly_tags: vec![],
                mass_kg: 1000.0,
                knowledge: crate::AsteroidKnowledge {
                    tag_beliefs: vec![],
                    composition: None,
                },
            },
        );

        let completed = evaluate_milestones(&mut state, &content, &mut events);
        assert_eq!(completed, vec!["test_asteroids"]);
    }

    #[test]
    fn phase_advances_on_milestone() {
        let mut content = base_content();
        content.milestones = vec![MilestoneDef {
            id: "phase_test".to_string(),
            name: "Phase Test".to_string(),
            description: String::new(),
            conditions: vec![MilestoneCondition::MetricAbove {
                field: "tick".to_string(),
                threshold: 0.0,
            }],
            rewards: MilestoneReward {
                grant_amount: 5_000_000.0,
                reputation: 10.0,
                unlock_trade_tier: Some(TradeTier::BasicImport),
                unlock_zone_ids: vec![],
                unlock_module_ids: vec![],
            },
            phase_advance: Some(GamePhase::Orbital),
        }];

        let mut state = base_state(&content);
        assert_eq!(state.progression.phase, GamePhase::Startup);
        assert_eq!(state.progression.trade_tier, TradeTier::None);

        let mut events = Vec::new();
        evaluate_milestones(&mut state, &content, &mut events);

        assert_eq!(state.progression.phase, GamePhase::Orbital);
        assert_eq!(state.progression.trade_tier, TradeTier::BasicImport);
        assert_eq!(state.balance, 5_000_000.0);
        assert_eq!(state.progression.reputation, 10.0);
        assert_eq!(state.progression.grant_history.len(), 1);

        // Verify all 3 event types emitted: PhaseAdvanced, GrantAwarded, MilestoneReached
        assert!(
            events
                .iter()
                .any(|e| matches!(&e.event, crate::Event::PhaseAdvanced { .. })),
            "should emit PhaseAdvanced"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(&e.event, crate::Event::GrantAwarded { amount, .. } if *amount == 5_000_000.0)),
            "should emit GrantAwarded"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(&e.event, crate::Event::MilestoneReached { milestone_name, .. } if milestone_name == "Phase Test")),
            "should emit MilestoneReached with name"
        );
    }

    #[test]
    fn unmet_conditions_prevent_completion() {
        let mut content = base_content();
        content.milestones = vec![test_milestone(
            "impossible",
            vec![MilestoneCondition::MetricAbove {
                field: "total_ore_kg".to_string(),
                threshold: 999_999.0,
            }],
        )];

        let mut state = base_state(&content);
        let mut events = Vec::new();
        let completed = evaluate_milestones(&mut state, &content, &mut events);
        assert!(completed.is_empty());
    }

    #[test]
    fn empty_milestones_is_noop() {
        let mut content = base_content();
        content.milestones = vec![];

        let mut state = base_state(&content);
        let mut events = Vec::new();
        let completed = evaluate_milestones(&mut state, &content, &mut events);
        assert!(completed.is_empty());
    }

    #[test]
    fn satellite_counters_resolve() {
        let content = base_content();
        let mut state = base_state(&content);

        // No satellites — all counters should be 0.
        assert_eq!(
            resolve_counter(&state, &content, "satellites_deployed"),
            Some(0.0)
        );
        assert_eq!(
            resolve_counter(&state, &content, "comm_satellites"),
            Some(0.0)
        );
        assert_eq!(
            resolve_counter(&state, &content, "survey_satellites"),
            Some(0.0)
        );
        assert_eq!(
            resolve_counter(&state, &content, "science_satellites"),
            Some(0.0)
        );

        // Add satellites.
        state.satellites.insert(
            crate::SatelliteId("sat_1".into()),
            crate::SatelliteState {
                id: crate::SatelliteId("sat_1".into()),
                def_id: "sat_comm_relay".into(),
                name: "Comm 1".into(),
                position: crate::test_fixtures::test_position(),
                deployed_tick: 0,
                wear: 0.0,
                enabled: true,
                satellite_type: "communication".into(),
                payload_config: None,
            },
        );
        state.satellites.insert(
            crate::SatelliteId("sat_2".into()),
            crate::SatelliteState {
                id: crate::SatelliteId("sat_2".into()),
                def_id: "sat_survey".into(),
                name: "Survey 1".into(),
                position: crate::test_fixtures::test_position(),
                deployed_tick: 0,
                wear: 0.0,
                enabled: true,
                satellite_type: "survey".into(),
                payload_config: None,
            },
        );
        state.satellites.insert(
            crate::SatelliteId("sat_3".into()),
            crate::SatelliteState {
                id: crate::SatelliteId("sat_3".into()),
                def_id: "sat_survey".into(),
                name: "Survey 2".into(),
                position: crate::test_fixtures::test_position(),
                deployed_tick: 0,
                wear: 0.0,
                enabled: false, // disabled — should not count
                satellite_type: "survey".into(),
                payload_config: None,
            },
        );

        assert_eq!(
            resolve_counter(&state, &content, "satellites_deployed"),
            Some(2.0)
        );
        assert_eq!(
            resolve_counter(&state, &content, "comm_satellites"),
            Some(1.0)
        );
        assert_eq!(
            resolve_counter(&state, &content, "survey_satellites"),
            Some(1.0)
        );
        assert_eq!(
            resolve_counter(&state, &content, "science_satellites"),
            Some(0.0)
        );
    }

    #[test]
    fn first_satellite_deployed_milestone_triggers() {
        let mut content = base_content();
        content.milestones = vec![MilestoneDef {
            id: "first_satellite_deployed".into(),
            name: "First Satellite".into(),
            description: "Deploy a satellite".into(),
            conditions: vec![MilestoneCondition::CounterAbove {
                counter: "satellites_deployed".into(),
                threshold: 1.0,
            }],
            rewards: MilestoneReward {
                grant_amount: 15_000_000.0,
                reputation: 0.0,
                unlock_trade_tier: None,
                unlock_zone_ids: vec![],
                unlock_module_ids: vec![],
            },
            phase_advance: None,
        }];

        let mut state = base_state(&content);
        let initial_balance = state.balance;

        // No satellites — milestone should not trigger.
        let mut events = Vec::new();
        let completed = evaluate_milestones(&mut state, &content, &mut events);
        assert!(completed.is_empty());

        // Add a satellite.
        state.satellites.insert(
            crate::SatelliteId("sat_1".into()),
            crate::SatelliteState {
                id: crate::SatelliteId("sat_1".into()),
                def_id: "sat_comm_relay".into(),
                name: "Comm 1".into(),
                position: crate::test_fixtures::test_position(),
                deployed_tick: 0,
                wear: 0.0,
                enabled: true,
                satellite_type: "communication".into(),
                payload_config: None,
            },
        );

        let mut events = Vec::new();
        let completed = evaluate_milestones(&mut state, &content, &mut events);
        assert_eq!(completed, vec!["first_satellite_deployed"]);
        assert!(
            (state.balance - initial_balance - 15_000_000.0).abs() < f64::EPSILON,
            "grant should be applied"
        );
    }

    #[test]
    fn ground_operations_counters() {
        let content = base_content();
        let mut state = base_state(&content);

        // total_raw_data — initially 0
        assert_eq!(
            resolve_counter(&state, &content, "total_raw_data"),
            Some(0.0)
        );

        // Add some raw data
        state
            .research
            .data_pool
            .insert(crate::DataKind::new("OpticalData"), 5.0);
        assert_eq!(
            resolve_counter(&state, &content, "total_raw_data"),
            Some(5.0)
        );

        // asteroids_classified — initially 0
        assert_eq!(
            resolve_counter(&state, &content, "asteroids_classified"),
            Some(0.0)
        );

        // Add an asteroid without classification
        let asteroid_id = crate::AsteroidId("a1".into());
        state.asteroids.insert(
            asteroid_id.clone(),
            crate::AsteroidState {
                id: asteroid_id,
                position: crate::test_fixtures::test_position(),
                true_composition: std::collections::HashMap::from([("Fe".into(), 0.7)]),
                anomaly_tags: vec![],
                mass_kg: 1000.0,
                knowledge: crate::AsteroidKnowledge {
                    tag_beliefs: vec![],
                    composition: None,
                },
            },
        );
        assert_eq!(
            resolve_counter(&state, &content, "asteroids_classified"),
            Some(0.0),
            "unclassified asteroid should not count"
        );

        // Classify the asteroid
        state
            .asteroids
            .get_mut(&crate::AsteroidId("a1".into()))
            .unwrap()
            .knowledge
            .composition = Some(std::collections::HashMap::from([("Fe".into(), 0.7)]));
        assert_eq!(
            resolve_counter(&state, &content, "asteroids_classified"),
            Some(1.0)
        );

        // total_launches — initially 0
        assert_eq!(
            resolve_counter(&state, &content, "total_launches"),
            Some(0.0)
        );

        // stations_deployed — from counters, initially 0
        assert_eq!(
            resolve_counter(&state, &content, "stations_deployed"),
            Some(0.0)
        );

        // reusable_landings — placeholder, always 0
        assert_eq!(
            resolve_counter(&state, &content, "reusable_landings"),
            Some(0.0)
        );
    }

    #[test]
    fn rockets_in_inventory_counter() {
        let mut content = base_content();
        // Add a test rocket def
        content.rocket_defs.insert(
            "rocket_test".into(),
            crate::RocketDef {
                id: "rocket_test".into(),
                name: "Test Rocket".into(),
                payload_capacity_kg: 100.0,
                base_launch_cost: 1_000_000.0,
                fuel_kg: 500.0,
                transit_minutes: 60,
                required_tech: None,
            },
        );

        let mut state = base_state(&content);

        // No rockets in inventory initially
        assert_eq!(
            resolve_counter(&state, &content, "rockets_in_inventory"),
            Some(0.0)
        );

        // Add a non-rocket component — should not count
        state
            .stations
            .values_mut()
            .next()
            .unwrap()
            .core
            .inventory
            .push(crate::InventoryItem::Component {
                component_id: crate::ComponentId("nozzle".into()),
                count: 1,
                quality: 1.0,
            });
        assert_eq!(
            resolve_counter(&state, &content, "rockets_in_inventory"),
            Some(0.0),
            "non-rocket component should not count"
        );

        // Add a rocket component matching a rocket_def key
        state
            .stations
            .values_mut()
            .next()
            .unwrap()
            .core
            .inventory
            .push(crate::InventoryItem::Component {
                component_id: crate::ComponentId("rocket_test".into()),
                count: 1,
                quality: 1.0,
            });
        assert_eq!(
            resolve_counter(&state, &content, "rockets_in_inventory"),
            Some(1.0),
            "rocket component matching rocket_def key should count"
        );
    }

    #[test]
    fn first_observation_milestone_triggers() {
        let mut content = base_content();
        content.milestones = vec![test_milestone(
            "first_observation",
            vec![MilestoneCondition::CounterAbove {
                counter: "total_raw_data".into(),
                threshold: 1.0,
            }],
        )];

        let mut state = base_state(&content);

        // No data — should not trigger
        let mut events = Vec::new();
        let completed = evaluate_milestones(&mut state, &content, &mut events);
        assert!(completed.is_empty());

        // Add raw data
        state
            .research
            .data_pool
            .insert(crate::DataKind::new("OpticalData"), 3.0);
        let completed = evaluate_milestones(&mut state, &content, &mut events);
        assert_eq!(completed, vec!["first_observation"]);
    }

    #[test]
    fn first_launch_milestone_triggers() {
        let mut content = base_content();
        content.milestones = vec![MilestoneDef {
            id: "first_launch".into(),
            name: "First Launch".into(),
            description: String::new(),
            conditions: vec![MilestoneCondition::CounterAbove {
                counter: "total_launches".into(),
                threshold: 1.0,
            }],
            rewards: MilestoneReward {
                grant_amount: 25_000_000.0,
                reputation: 30.0,
                unlock_trade_tier: None,
                unlock_zone_ids: vec![],
                unlock_module_ids: vec![],
            },
            phase_advance: Some(GamePhase::Orbital),
        }];

        let mut state = base_state(&content);
        assert_eq!(state.progression.phase, GamePhase::Startup);

        // No launches — should not trigger
        let mut events = Vec::new();
        let completed = evaluate_milestones(&mut state, &content, &mut events);
        assert!(completed.is_empty());

        // Add a ground facility with a launched pad
        let facility_id = crate::GroundFacilityId("gf1".into());
        let mut facility = crate::GroundFacilityState {
            id: facility_id.clone(),
            name: "Test Facility".into(),
            position: crate::test_fixtures::test_position(),
            core: crate::FacilityCore::default(),
            launch_transits: vec![],
        };
        facility.core.modules.push(crate::ModuleState {
            id: crate::ModuleInstanceId("1".into()),
            def_id: "module_launch_pad_small".into(),
            enabled: true,
            wear: crate::WearState::default(),
            kind_state: crate::ModuleKindState::LaunchPad(crate::LaunchPadState {
                available: true,
                recovery_ticks_remaining: 0,
                launches_count: 1,
            }),
            thermal: None,
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
            slot_index: None,
        });
        state.ground_facilities.insert(facility_id, facility);

        let completed = evaluate_milestones(&mut state, &content, &mut events);
        assert_eq!(completed, vec!["first_launch"]);
        assert_eq!(state.progression.phase, GamePhase::Orbital);
    }

    #[test]
    fn assembler_runs_includes_ground_facilities() {
        let content = base_content();
        let mut state = base_state(&content);

        // Start with just station components
        let station_count = resolve_counter(&state, &content, "assembler_runs").unwrap_or(0.0);

        // Add a component to a ground facility
        let facility_id = crate::GroundFacilityId("gf1".into());
        let mut facility = crate::GroundFacilityState {
            id: facility_id.clone(),
            name: "Test Facility".into(),
            position: crate::test_fixtures::test_position(),
            core: crate::FacilityCore::default(),
            launch_transits: vec![],
        };
        facility
            .core
            .inventory
            .push(crate::InventoryItem::Component {
                component_id: crate::ComponentId("nozzle".into()),
                count: 1,
                quality: 1.0,
            });
        state.ground_facilities.insert(facility_id, facility);

        let new_count = resolve_counter(&state, &content, "assembler_runs").unwrap();
        assert_eq!(
            new_count,
            station_count + 1.0,
            "ground facility components should count in assembler_runs"
        );
    }

    /// Pins the engine-level interval gating behavior:
    /// `engine::tick` must only call `evaluate_milestones` every N ticks
    /// where N == `content.scoring.computation_interval_ticks`.
    ///
    /// Without this test, the gate could be silently removed or changed
    /// (e.g. to a different constant) and no other test would catch it —
    /// the unit tests in this module call `evaluate_milestones` directly.
    #[test]
    fn engine_tick_gates_milestone_evaluation_to_scoring_interval() {
        use rand::SeedableRng;

        let mut content = base_content();
        // Set a small interval so we don't have to tick hundreds of times.
        content.scoring.computation_interval_ticks = 5;
        // A milestone that's already satisfied at tick 0 (tick >= 0).
        // If evaluated, it fires on the very first call.
        content.milestones = vec![test_milestone(
            "ticks_any",
            vec![MilestoneCondition::MetricAbove {
                field: "tick".to_string(),
                threshold: 0.0,
            }],
        )];

        let mut state = base_state(&content);
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0);

        // Tick 0: gate fires (0 % 5 == 0) — milestone completes immediately.
        let _ = crate::tick(&mut state, &[], &content, &mut rng, None);
        assert!(
            state.progression.is_milestone_completed("ticks_any"),
            "milestone should fire on tick=0 when gate aligns (0 % 5 == 0)"
        );

        // Reset and test the non-aligned case: start state at tick=1 so the
        // gate is skipped for 4 consecutive ticks before it fires at tick=5.
        let mut state = base_state(&content);
        state.meta.tick = 1;
        // Ticks 1..5 skip evaluation (1,2,3,4 all fail `% 5 == 0`).
        for _ in 0..4 {
            let _ = crate::tick(&mut state, &[], &content, &mut rng, None);
            assert!(
                !state.progression.is_milestone_completed("ticks_any"),
                "milestone should NOT fire between gate intervals (tick={})",
                state.meta.tick
            );
        }
        // Next tick moves tick from 4→5, gate fires at tick=5.
        let _ = crate::tick(&mut state, &[], &content, &mut rng, None);
        assert!(
            state.progression.is_milestone_completed("ticks_any"),
            "milestone should fire when gate re-aligns at tick={}",
            state.meta.tick
        );
    }
}
