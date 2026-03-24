use crate::{Event, EventLevel, GameContent, GameState, TechId};
use rand::Rng;

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

/// Geometric mean of a slice of f32 values.
pub(crate) fn geometric_mean(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let product: f32 = values.iter().product();
    product.powf(1.0 / values.len() as f32)
}

pub(crate) fn advance_research(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    event_level: EventLevel,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    // Only roll every N ticks (skip tick 0)
    if current_tick == 0
        || !current_tick.is_multiple_of(content.constants.research_roll_interval_ticks)
    {
        return;
    }

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

        // Compute domain sufficiency
        let sufficiency = if tech_def.domain_requirements.is_empty() {
            1.0
        } else {
            let ratios: Vec<f32> = tech_def
                .domain_requirements
                .iter()
                .map(|(domain, required)| {
                    let accumulated =
                        progress.map_or(0.0, |p| p.points.get(domain).copied().unwrap_or(0.0));
                    (accumulated / required).min(1.0)
                })
                .collect();
            geometric_mean(&ratios)
        };

        let total_points: f32 = progress.map_or(0.0, |p| p.points.values().sum());

        let effective = sufficiency * total_points;
        let probability = if tech_def.difficulty > 0.0 {
            1.0 - (-effective / tech_def.difficulty).exp()
        } else if effective > 0.0 {
            1.0
        } else {
            0.0
        };
        let rolled: f32 = rng.gen();

        if event_level == EventLevel::Debug {
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                Event::ResearchRoll {
                    tech_id: tech_id.clone(),
                    evidence: effective,
                    p: probability,
                    rolled,
                },
            ));
        }

        if rolled < probability {
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
                Event::TechUnlocked { tech_id },
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
            crate::DataKind::SurveyData,
            "survey",
            &constants,
        );
        assert!(amount > 0.0);
        assert!(
            *research
                .data_pool
                .get(&crate::DataKind::SurveyData)
                .unwrap()
                > 0.0
        );
        assert_eq!(research.action_counts["survey"], 1);

        let amount2 = generate_data(
            &mut research,
            crate::DataKind::SurveyData,
            "survey",
            &constants,
        );
        assert!(amount2 < amount);
        assert_eq!(research.action_counts["survey"], 2);
    }
}

#[cfg(test)]
mod research_roll_tests {
    use super::*;
    use crate::test_fixtures::base_content;
    use crate::AHashMap;
    use crate::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use std::collections::HashMap;

    fn research_state_at_tick(tick: u64) -> GameState {
        let content = base_content();
        let mut state = crate::test_fixtures::base_state(&content);
        state.meta.tick = tick;
        state
    }

    #[test]
    fn research_roll_skips_when_not_interval_tick() {
        let content = base_content();
        let mut state = research_state_at_tick(1);
        // Add some evidence to make unlock possible
        state.research.evidence.insert(
            TechId("tech_deep_scan_v1".to_string()),
            DomainProgress {
                points: HashMap::from([(ResearchDomain::Survey, 10000.0)]),
            },
        );

        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let mut events = Vec::new();
        advance_research(
            &mut state,
            &content,
            &mut rng,
            EventLevel::Normal,
            &mut events,
        );

        // No unlock events at tick=1 (not interval)
        let unlocks: Vec<_> = events
            .iter()
            .filter(|e| matches!(&e.event, Event::TechUnlocked { .. }))
            .collect();
        assert!(unlocks.is_empty(), "should not roll at non-interval tick");
    }

    #[test]
    fn research_roll_runs_at_interval() {
        let content = base_content();
        let mut state = research_state_at_tick(60);
        // Easy tech (difficulty=10) with lots of points — very high probability
        state.research.evidence.insert(
            TechId("tech_deep_scan_v1".to_string()),
            DomainProgress {
                points: HashMap::from([(ResearchDomain::Survey, 10000.0)]),
            },
        );

        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let mut events = Vec::new();
        advance_research(
            &mut state,
            &content,
            &mut rng,
            EventLevel::Normal,
            &mut events,
        );

        // With 10000 points and difficulty=10, p ≈ 1.0, should unlock
        assert!(
            state
                .research
                .unlocked
                .contains(&TechId("tech_deep_scan_v1".to_string())),
            "tech should be unlocked"
        );
        let unlock_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(&e.event, Event::TechUnlocked { .. }))
            .collect();
        assert_eq!(unlock_events.len(), 1);
    }

    #[test]
    fn zero_domain_progress_means_zero_probability() {
        let mut content = base_content();
        // Set domain requirements so sufficiency matters
        content.techs[0].domain_requirements = HashMap::from([(ResearchDomain::Survey, 100.0)]);

        let mut state = research_state_at_tick(60);
        // No evidence at all — zero progress

        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let mut events = Vec::new();
        advance_research(
            &mut state,
            &content,
            &mut rng,
            EventLevel::Normal,
            &mut events,
        );

        assert!(
            !state
                .research
                .unlocked
                .contains(&TechId("tech_deep_scan_v1".to_string())),
            "tech should NOT be unlocked with zero progress"
        );
    }

    #[test]
    fn domain_sufficiency_geometric_mean() {
        // Simple case: all equal
        let result = geometric_mean(&[0.5, 0.5, 0.5]);
        assert!((result - 0.5).abs() < 1e-5);

        // Mixed case: geometric mean of [1.0, 0.25] = sqrt(0.25) = 0.5
        let result = geometric_mean(&[1.0, 0.25]);
        assert!((result - 0.5).abs() < 1e-5);

        // Empty returns 0
        let result = geometric_mean(&[]);
        assert!((result - 0.0).abs() < 1e-5);

        // Single value
        let result = geometric_mean(&[0.8]);
        assert!((result - 0.8).abs() < 1e-5);
    }

    #[test]
    fn research_roll_with_no_domain_requirements() {
        let mut content = base_content();
        // Ensure empty domain_requirements (base_content already has this)
        content.techs[0].domain_requirements = HashMap::new();
        // Very low difficulty so any points unlock
        content.techs[0].difficulty = 0.001;

        let mut state = research_state_at_tick(60);
        // Add some evidence points (sufficiency=1.0 when no domain_requirements)
        state.research.evidence.insert(
            TechId("tech_deep_scan_v1".to_string()),
            DomainProgress {
                points: HashMap::from([(ResearchDomain::Survey, 100.0)]),
            },
        );

        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let mut events = Vec::new();
        advance_research(
            &mut state,
            &content,
            &mut rng,
            EventLevel::Normal,
            &mut events,
        );

        assert!(
            state
                .research
                .unlocked
                .contains(&TechId("tech_deep_scan_v1".to_string())),
            "tech should unlock with no domain requirements and sufficient points"
        );
    }

    #[test]
    fn tech_unlock_applies_stat_modifiers_to_global_set() {
        let mut content = base_content();
        // Set up a tech with a StatModifier effect, easy to unlock.
        content.techs = vec![TechDef {
            id: TechId("tech_test_modifier".to_string()),
            name: "Test Modifier".to_string(),
            prereqs: vec![],
            domain_requirements: HashMap::new(),
            accepted_data: vec![crate::DataKind::SurveyData],
            difficulty: 0.001,
            effects: vec![TechEffect::StatModifier {
                stat: crate::modifiers::StatId::ProcessingYield,
                op: crate::modifiers::ModifierOp::PctAdditive,
                value: 0.25,
            }],
        }];

        let mut state = research_state_at_tick(60);
        state.research.evidence.insert(
            TechId("tech_test_modifier".to_string()),
            DomainProgress {
                points: HashMap::from([(ResearchDomain::Survey, 100.0)]),
            },
        );

        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let mut events = Vec::new();
        advance_research(
            &mut state,
            &content,
            &mut rng,
            EventLevel::Normal,
            &mut events,
        );

        // Tech should have unlocked.
        assert!(
            state
                .research
                .unlocked
                .contains(&TechId("tech_test_modifier".to_string())),
            "test tech should unlock"
        );
        // Global modifiers should contain the stat modifier.
        assert_eq!(state.modifiers.len(), 1);
        let result = state
            .modifiers
            .resolve(crate::modifiers::StatId::ProcessingYield, 100.0);
        // 100 × (1 + 0.25) = 125
        assert!(
            (result - 125.0).abs() < 1e-10,
            "modifier should increase yield by 25%: got {result}"
        );
    }

    #[test]
    fn stat_modifiers_survive_serde_roundtrip() {
        let mut state = research_state_at_tick(0);
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
}
