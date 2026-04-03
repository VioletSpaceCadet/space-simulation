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
fn resolve_counter(state: &GameState, counter: &str) -> Option<f64> {
    match counter {
        "asteroids_discovered" => Some(state.asteroids.len() as f64),
        "techs_unlocked" => Some(state.research.unlocked.len() as f64),
        "ships_built" => Some(state.ships.len() as f64),
        "assembler_runs" => {
            // Count total component items across all station inventories
            let count: usize = state
                .stations
                .values()
                .flat_map(|s| s.inventory.iter())
                .filter(|item| matches!(item, crate::InventoryItem::Component { .. }))
                .count();
            Some(count as f64)
        }
        _ => None,
    }
}

/// Check whether a single milestone condition is met.
fn condition_met(cond: &MilestoneCondition, state: &GameState, metrics: &MetricsSnapshot) -> bool {
    match cond {
        MilestoneCondition::MetricAbove { field, threshold } => metrics
            .get_field_f64(field)
            .map_or(false, |val| val >= *threshold),
        MilestoneCondition::CounterAbove { counter, threshold } => {
            resolve_counter(state, counter).map_or(false, |val| val >= *threshold)
        }
        MilestoneCondition::MilestoneCompleted { milestone_id } => {
            state.progression.is_milestone_completed(milestone_id)
        }
    }
}

/// Evaluate all milestones against current state. Returns IDs of newly completed milestones.
///
/// Milestones are evaluated in sorted order (by ID) for determinism.
/// Multiple milestones can complete on the same tick, and chained milestones
/// (condition: `MilestoneCompleted`) can trigger within the same evaluation
/// pass because completions are applied immediately.
pub fn evaluate_milestones(
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<crate::EventEnvelope>,
) -> Vec<String> {
    if content.milestones.is_empty() {
        return Vec::new();
    }

    let metrics = crate::metrics::compute_metrics(state, content);

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
                .all(|c| condition_met(c, state, &metrics));

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
                        state.progression.phase = new_phase;
                    }
                }

                // Record grant (actual balance application is VIO-534)
                if milestone.rewards.grant_amount > 0.0 {
                    state.progression.grant_history.push(GrantRecord {
                        milestone_id: milestone.id.clone(),
                        amount: milestone.rewards.grant_amount,
                        tick: state.meta.tick,
                    });
                    state.balance += milestone.rewards.grant_amount;
                }

                // Advance reputation
                state.progression.reputation += milestone.rewards.reputation;

                // Advance trade tier (only upgrades)
                if let Some(new_tier) = milestone.rewards.unlock_trade_tier {
                    if new_tier > state.progression.trade_tier {
                        state.progression.trade_tier = new_tier;
                    }
                }

                // Emit event
                events.push(crate::emit(
                    &mut state.counters,
                    state.meta.tick,
                    crate::Event::MilestoneCompleted {
                        milestone_id: milestone.id.clone(),
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
}
