use crate::{GameContent, GameState, TechId};

/// Diminishing-returns yield: `floor + (peak - floor) * decay_rate^count`
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn data_yield(count: u64, peak: f32, floor: f32, decay_rate: f32) -> f32 {
    floor + (peak - floor) * decay_rate.powi(count as i32)
}

/// Generate raw data with diminishing returns, updating pool and action counter.
pub(crate) fn generate_data(
    research: &mut crate::ResearchState,
    kind: crate::DataKind,
    action_key: &str,
    constants: &crate::Constants,
) -> f32 {
    let count = research.action_counts.get(action_key).copied().unwrap_or(0);
    let amount = data_yield(
        count,
        constants.data_generation_peak,
        constants.data_generation_floor,
        constants.data_generation_decay_rate,
    );
    *research.data_pool.entry(kind).or_insert(0.0) += amount;
    *research
        .action_counts
        .entry(action_key.to_string())
        .or_insert(0) += 1;
    amount
}

/// Check if all domain requirements are met for a tech.
fn requirements_met(tech_def: &crate::TechDef, progress: Option<&crate::DomainProgress>) -> bool {
    tech_def
        .domain_requirements
        .iter()
        .all(|(domain, required)| {
            let accumulated =
                progress.map_or(0.0, |p| p.points.get(domain).copied().unwrap_or(0.0));
            accumulated >= *required
        })
}

pub(crate) fn advance_research(
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    // Collect eligible techs: prereqs met, not yet unlocked. Sort for determinism.
    let mut eligible: Vec<TechId> = content
        .techs
        .iter()
        .filter(|tech| {
            !state.research.unlocked.contains(&tech.id)
                && tech
                    .prereqs
                    .iter()
                    .all(|prereq| state.research.unlocked.contains(prereq))
        })
        .map(|tech| tech.id.clone())
        .collect();
    eligible.sort_by(|a, b| a.0.cmp(&b.0));

    for tech_id in eligible {
        let Some(tech_def) = content.techs.iter().find(|t| t.id == tech_id) else {
            continue;
        };
        let progress = state.research.evidence.get(&tech_id);

        if requirements_met(tech_def, progress) {
            state.research.unlocked.insert(tech_id.clone());
            // Apply stat modifier effects to global modifiers.
            for effect in &tech_def.effects {
                if let crate::TechEffect::StatModifier { stat, op, value } = effect {
                    state.modifiers.add(crate::modifiers::Modifier {
                        stat: *stat,
                        op: *op,
                        value: *value,
                        source: crate::modifiers::ModifierSource::Tech(tech_id.0.clone()),
                        condition: None,
                    });
                }
            }
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                crate::Event::TechUnlocked { tech_id },
            ));
        }
    }
}

#[cfg(test)]
mod data_generation_tests {
    use super::*;
    use crate::test_fixtures::base_content;
    use crate::AHashMap;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn diminishing_returns_first_action_yields_peak() {
        let amount = data_yield(0, 100.0, 5.0, 0.7);
        assert!((amount - 100.0).abs() < 1e-3);
    }

    #[test]
    fn diminishing_returns_decays_over_actions() {
        let first = data_yield(0, 100.0, 5.0, 0.7);
        let second = data_yield(1, 100.0, 5.0, 0.7);
        let tenth = data_yield(9, 100.0, 5.0, 0.7);
        assert!(second < first);
        assert!(tenth < second);
        assert!(tenth >= 5.0);
    }

    #[test]
    fn diminishing_returns_converges_to_floor() {
        let amount = data_yield(100, 100.0, 5.0, 0.7);
        assert!((amount - 5.0).abs() < 0.1);
    }

    #[test]
    fn generate_data_adds_to_pool_and_increments_counter() {
        let mut research = crate::ResearchState {
            unlocked: HashSet::new(),
            data_pool: AHashMap::default(),
            evidence: AHashMap::default(),
            action_counts: AHashMap::default(),
        };
        let constants = base_content().constants;

        let amount = generate_data(
            &mut research,
            crate::DataKind::new(crate::DataKind::SURVEY),
            "survey",
            &constants,
        );
        assert!(amount > 0.0);
        assert!(
            *research
                .data_pool
                .get(&crate::DataKind::new(crate::DataKind::SURVEY))
                .unwrap()
                > 0.0
        );
        assert_eq!(research.action_counts["survey"], 1);

        let amount2 = generate_data(
            &mut research,
            crate::DataKind::new(crate::DataKind::SURVEY),
            "survey",
            &constants,
        );
        assert!(amount2 < amount);
        assert_eq!(research.action_counts["survey"], 2);
    }
}

#[cfg(test)]
mod research_threshold_tests {
    use super::*;
    use crate::test_fixtures::base_content;
    use crate::*;
    use std::collections::HashMap;

    fn test_state_at_tick(tick: u64) -> GameState {
        let content = base_content();
        let mut state = crate::test_fixtures::base_state(&content);
        state.meta.tick = tick;
        state
    }

    #[test]
    fn unlocks_when_all_domain_requirements_met() {
        let mut content = base_content();
        content.techs[0].domain_requirements = HashMap::from([(ResearchDomain::Survey, 100.0)]);

        let mut state = test_state_at_tick(1);
        state.research.evidence.insert(
            TechId("tech_deep_scan_v1".to_string()),
            DomainProgress {
                points: HashMap::from([(ResearchDomain::Survey, 100.0)]),
            },
        );

        let mut events = Vec::new();
        advance_research(&mut state, &content, &mut events);

        assert!(
            state
                .research
                .unlocked
                .contains(&TechId("tech_deep_scan_v1".to_string())),
            "tech should unlock when domain requirements are exactly met"
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(&e.event, Event::TechUnlocked { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn does_not_unlock_when_requirements_not_met() {
        let mut content = base_content();
        content.techs[0].domain_requirements = HashMap::from([(ResearchDomain::Survey, 100.0)]);

        let mut state = test_state_at_tick(1);
        // Only 99 points — not enough
        state.research.evidence.insert(
            TechId("tech_deep_scan_v1".to_string()),
            DomainProgress {
                points: HashMap::from([(ResearchDomain::Survey, 99.0)]),
            },
        );

        let mut events = Vec::new();
        advance_research(&mut state, &content, &mut events);

        assert!(
            !state
                .research
                .unlocked
                .contains(&TechId("tech_deep_scan_v1".to_string())),
            "tech should NOT unlock when requirements are not met"
        );
    }

    #[test]
    fn zero_progress_means_no_unlock() {
        let mut content = base_content();
        content.techs[0].domain_requirements = HashMap::from([(ResearchDomain::Survey, 100.0)]);

        let mut state = test_state_at_tick(1);
        // No evidence at all

        let mut events = Vec::new();
        advance_research(&mut state, &content, &mut events);

        assert!(
            !state
                .research
                .unlocked
                .contains(&TechId("tech_deep_scan_v1".to_string())),
            "tech should NOT unlock with zero progress"
        );
    }

    #[test]
    fn multi_domain_requires_all_met() {
        let mut content = base_content();
        content.techs[0].domain_requirements = HashMap::from([
            (ResearchDomain::Survey, 50.0),
            (ResearchDomain::Materials, 50.0),
        ]);

        let mut state = test_state_at_tick(1);
        // Only one domain met
        state.research.evidence.insert(
            TechId("tech_deep_scan_v1".to_string()),
            DomainProgress {
                points: HashMap::from([
                    (ResearchDomain::Survey, 100.0),
                    (ResearchDomain::Materials, 10.0),
                ]),
            },
        );

        let mut events = Vec::new();
        advance_research(&mut state, &content, &mut events);

        assert!(
            !state
                .research
                .unlocked
                .contains(&TechId("tech_deep_scan_v1".to_string())),
            "tech should NOT unlock when only some domains are met"
        );
    }

    #[test]
    fn no_domain_requirements_unlocks_immediately() {
        let mut content = base_content();
        content.techs[0].domain_requirements = HashMap::new();

        let mut state = test_state_at_tick(1);

        let mut events = Vec::new();
        advance_research(&mut state, &content, &mut events);

        assert!(
            state
                .research
                .unlocked
                .contains(&TechId("tech_deep_scan_v1".to_string())),
            "tech with no domain requirements should unlock immediately"
        );
    }

    #[test]
    fn tech_unlock_applies_stat_modifiers_to_global_set() {
        let mut content = base_content();
        content.techs = vec![TechDef {
            id: TechId("tech_test_modifier".to_string()),
            name: "Test Modifier".to_string(),
            prereqs: vec![],
            domain_requirements: HashMap::new(),
            accepted_data: vec![crate::DataKind::new(crate::DataKind::SURVEY)],
            effects: vec![TechEffect::StatModifier {
                stat: crate::modifiers::StatId::ProcessingYield,
                op: crate::modifiers::ModifierOp::PctAdditive,
                value: 0.25,
            }],
        }];

        let mut state = test_state_at_tick(1);

        let mut events = Vec::new();
        advance_research(&mut state, &content, &mut events);

        assert!(
            state
                .research
                .unlocked
                .contains(&TechId("tech_test_modifier".to_string())),
            "test tech should unlock"
        );
        assert_eq!(state.modifiers.len(), 1);
        let result = state
            .modifiers
            .resolve(crate::modifiers::StatId::ProcessingYield, 100.0);
        assert!(
            (result - 125.0).abs() < 1e-10,
            "modifier should increase yield by 25%: got {result}"
        );
    }

    #[test]
    fn stat_modifiers_survive_serde_roundtrip() {
        let mut state = test_state_at_tick(0);
        state.modifiers.add(crate::modifiers::Modifier {
            stat: crate::modifiers::StatId::ShipSpeed,
            op: crate::modifiers::ModifierOp::PctAdditive,
            value: 0.10,
            source: crate::modifiers::ModifierSource::Tech("tech_efficient_transit".into()),
            condition: None,
        });

        let json = serde_json::to_string(&state).expect("serialize");
        let deserialized: GameState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.modifiers.len(), 1);
        let result = deserialized
            .modifiers
            .resolve(crate::modifiers::StatId::ShipSpeed, 100.0);
        assert!((result - 110.0).abs() < 1e-10);
    }

    #[test]
    fn prereq_chain_respected() {
        let mut content = base_content();
        content.techs = vec![
            TechDef {
                id: TechId("tech_a".to_string()),
                name: "Tech A".to_string(),
                prereqs: vec![],
                domain_requirements: HashMap::new(),
                accepted_data: vec![],
                effects: vec![],
            },
            TechDef {
                id: TechId("tech_b".to_string()),
                name: "Tech B".to_string(),
                prereqs: vec![TechId("tech_a".to_string())],
                domain_requirements: HashMap::new(),
                accepted_data: vec![],
                effects: vec![],
            },
        ];

        let mut state = test_state_at_tick(1);

        // First call: tech_a unlocks (no prereqs, no requirements)
        let mut events = Vec::new();
        advance_research(&mut state, &content, &mut events);

        assert!(state
            .research
            .unlocked
            .contains(&TechId("tech_a".to_string())));
        // tech_b was not eligible on this pass because tech_a wasn't unlocked at filter time
        assert!(!state
            .research
            .unlocked
            .contains(&TechId("tech_b".to_string())));

        // Second call: tech_b now eligible
        let mut events = Vec::new();
        advance_research(&mut state, &content, &mut events);

        assert!(state
            .research
            .unlocked
            .contains(&TechId("tech_b".to_string())));
    }
}
