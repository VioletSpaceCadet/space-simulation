use crate::{
    Event, EventEnvelope, GameContent, GameState, ModuleBehaviorDef, ModuleKindState, StationId,
};
use std::collections::HashMap;

pub(super) fn tick_lab_modules(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    super::ensure_station_index(state, station_id, content);
    let indices: Vec<usize> = state
        .stations
        .get(station_id)
        .map(|s| s.module_type_index.labs.clone())
        .unwrap_or_default();

    for module_idx in indices {
        let Some(ctx) = super::extract_context(state, station_id, module_idx, content) else {
            continue;
        };

        let lab_def = if let ModuleBehaviorDef::Lab(ld) = &ctx.def.behavior {
            ld.clone()
        } else {
            continue;
        };

        if !super::should_run(state, &ctx) {
            continue;
        }

        let outcome = execute(&ctx, &lab_def, state, content, events);
        super::apply_run_result(state, &ctx, outcome, content, events);
    }
}

fn execute(
    ctx: &super::ModuleTickContext,
    lab_def: &crate::LabDef,
    state: &mut GameState,
    _content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) -> super::RunOutcome {
    let current_tick = state.meta.tick;

    // Check assigned_tech
    let assigned_tech = {
        let Some(station) = state.stations.get(&ctx.station_id) else {
            return super::RunOutcome::Skipped { reset_timer: true };
        };
        if let ModuleKindState::Lab(ls) = &station.modules[ctx.module_idx].kind_state {
            ls.assigned_tech.clone()
        } else {
            return super::RunOutcome::Skipped { reset_timer: true };
        }
    };

    let Some(tech_id) = assigned_tech else {
        return super::RunOutcome::Skipped { reset_timer: true };
    };

    // Skip if tech already unlocked
    if state.research.unlocked.contains(&tech_id) {
        return super::RunOutcome::Skipped { reset_timer: true };
    }

    // Sum available data
    let available_data: f32 = lab_def
        .accepted_data
        .iter()
        .map(|kind| state.research.data_pool.get(kind).copied().unwrap_or(0.0))
        .sum();

    if available_data <= 0.0 {
        return super::RunOutcome::Stalled(super::StallReason::DataStarved);
    }

    // Consume data proportionally
    let to_consume = available_data.min(lab_def.data_consumption_per_run);
    let ratio = to_consume / lab_def.data_consumption_per_run;

    let mut consumed_total = 0.0_f32;
    for kind in &lab_def.accepted_data {
        let pool_amount = state.research.data_pool.get(kind).copied().unwrap_or(0.0);
        let fraction = pool_amount / available_data;
        let take = to_consume * fraction;
        if let Some(pool_val) = state.research.data_pool.get_mut(kind) {
            let actual_take = take.min(*pool_val);
            *pool_val -= actual_take;
            consumed_total += actual_take;
        }
    }

    // Route wear through modifier system for research output.
    let mut lab_mods = crate::modifiers::ModifierSet::new();
    lab_mods.add(crate::modifiers::Modifier::pct_mult(
        crate::modifiers::StatId::ResearchSpeed,
        f64::from(ctx.efficiency),
        crate::modifiers::ModifierSource::Wear,
    ));
    let points = lab_mods.resolve_with_f32(
        crate::modifiers::StatId::ResearchSpeed,
        lab_def.research_points_per_run * ratio,
        &state.modifiers,
    );

    // Add points to evidence
    let progress = state
        .research
        .evidence
        .entry(tech_id.clone())
        .or_insert_with(|| crate::DomainProgress {
            points: HashMap::new(),
        });
    *progress.points.entry(lab_def.domain.clone()).or_insert(0.0) += points;

    // Emit LabRan event
    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::LabRan {
            station_id: ctx.station_id.clone(),
            module_id: ctx.module_id.clone(),
            tech_id,
            data_consumed: consumed_total,
            points_produced: points,
            domain: lab_def.domain.clone(),
        },
    ));

    super::RunOutcome::Completed
}

#[cfg(test)]
mod tests {
    use crate::test_fixtures::ModuleDefBuilder;
    use crate::AHashMap;
    use crate::*;
    use std::collections::{HashMap, HashSet};

    fn lab_content() -> GameContent {
        let mut content = crate::test_fixtures::base_content();
        content.module_defs.insert(
            "module_exploration_lab".to_string(),
            ModuleDefBuilder::new("module_exploration_lab")
                .name("Exploration Lab")
                .mass(3500.0)
                .volume(7.0)
                .power(10.0)
                .wear(0.005)
                .behavior(ModuleBehaviorDef::Lab(LabDef {
                    domain: ResearchDomain::Survey,
                    data_consumption_per_run: 8.0,
                    research_points_per_run: 4.0,
                    accepted_data: vec![DataKind::SurveyData],
                    research_interval_minutes: 1,
                    research_interval_ticks: 1,
                }))
                .build(),
        );
        content
    }

    fn lab_state(content: &GameContent) -> GameState {
        let station_id = StationId("station_test".to_string());
        let mut state = GameState {
            meta: MetaState {
                tick: 0,
                seed: 42,
                schema_version: 1,
                content_version: content.content_version.clone(),
            },
            scan_sites: vec![],
            asteroids: std::collections::BTreeMap::new(),
            ships: std::collections::BTreeMap::new(),
            stations: [(
                station_id.clone(),
                StationState {
                    id: station_id,
                    position: crate::test_fixtures::test_position(),
                    inventory: vec![],
                    cargo_capacity_m3: 10_000.0,
                    power_available_per_tick: 100.0,
                    modules: vec![ModuleState {
                        id: ModuleInstanceId("lab_inst_0001".to_string()),
                        def_id: "module_exploration_lab".to_string(),
                        enabled: true,
                        kind_state: ModuleKindState::Lab(LabState {
                            ticks_since_last_run: 0,
                            assigned_tech: Some(TechId("tech_deep_scan_v1".to_string())),
                            starved: false,
                        }),
                        wear: WearState::default(),
                        power_stalled: false,
                        module_priority: 0,
                        assigned_crew: Default::default(),
                        crew_satisfied: true,
                        thermal: None,
                    }],
                    modifiers: crate::modifiers::ModifierSet::default(),
                    crew: Default::default(),
                    leaders: Vec::new(),
                    thermal_links: Vec::new(),
                    power: PowerState::default(),
                    cached_inventory_volume_m3: None,
                    module_type_index: crate::ModuleTypeIndex::default(),
                    power_budget_cache: crate::PowerBudgetCache::default(),
                },
            )]
            .into_iter()
            .collect(),
            research: ResearchState {
                unlocked: HashSet::new(),
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
            propellant_consumed_total: 0.0,
            body_cache: AHashMap::default(),
        };
        crate::test_fixtures::rebuild_indices(&mut state, content);
        state
    }

    #[test]
    fn lab_consumes_data_and_produces_points() {
        let content = lab_content();
        let mut state = lab_state(&content);
        state.research.data_pool.insert(DataKind::SurveyData, 100.0);

        let mut events = Vec::new();
        let station_id = StationId("station_test".to_string());
        super::tick_lab_modules(&mut state, &station_id, &content, &mut events);

        // Should have consumed 8.0 data
        let remaining = state.research.data_pool[&DataKind::SurveyData];
        assert!(
            (remaining - 92.0).abs() < 1e-3,
            "expected 92.0 remaining, got {remaining}"
        );

        // Should have produced 4.0 points in Exploration domain
        let tech_id = TechId("tech_deep_scan_v1".to_string());
        let progress = state.research.evidence.get(&tech_id).unwrap();
        let points = progress.points[&ResearchDomain::Survey];
        assert!(
            (points - 4.0).abs() < 1e-3,
            "expected 4.0 points, got {points}"
        );

        // Should have LabRan event
        let lab_ran = events
            .iter()
            .any(|e| matches!(&e.event, Event::LabRan { .. }));
        assert!(lab_ran, "expected LabRan event");
    }

    #[test]
    fn lab_starves_when_no_data() {
        let content = lab_content();
        let mut state = lab_state(&content);
        // data_pool is empty

        let mut events = Vec::new();
        let station_id = StationId("station_test".to_string());
        super::tick_lab_modules(&mut state, &station_id, &content, &mut events);

        // Should be starved
        let station = state.stations.get(&station_id).unwrap();
        if let ModuleKindState::Lab(ls) = &station.modules[0].kind_state {
            assert!(ls.starved, "expected starved=true");
        } else {
            panic!("expected Lab module");
        }

        // Should have LabStarved event
        let starved_event = events
            .iter()
            .any(|e| matches!(&e.event, Event::LabStarved { .. }));
        assert!(starved_event, "expected LabStarved event");

        // Should NOT have LabRan event
        let lab_ran = events
            .iter()
            .any(|e| matches!(&e.event, Event::LabRan { .. }));
        assert!(!lab_ran, "should not have LabRan when starved");
    }

    #[test]
    fn lab_partial_data_produces_proportional_points() {
        let content = lab_content();
        let mut state = lab_state(&content);
        // Lab wants 8.0 but only 4.0 available — half rate
        state.research.data_pool.insert(DataKind::SurveyData, 4.0);

        let mut events = Vec::new();
        let station_id = StationId("station_test".to_string());
        super::tick_lab_modules(&mut state, &station_id, &content, &mut events);

        // Should have consumed all 4.0
        let remaining = state.research.data_pool[&DataKind::SurveyData];
        assert!(
            remaining.abs() < 1e-3,
            "expected ~0.0 remaining, got {remaining}"
        );

        // Should have produced 2.0 points (4.0 * 0.5 ratio)
        let tech_id = TechId("tech_deep_scan_v1".to_string());
        let progress = state.research.evidence.get(&tech_id).unwrap();
        let points = progress.points[&ResearchDomain::Survey];
        assert!(
            (points - 2.0).abs() < 1e-3,
            "expected 2.0 points, got {points}"
        );
    }

    #[test]
    fn lab_skips_unlocked_tech() {
        let content = lab_content();
        let mut state = lab_state(&content);
        state.research.data_pool.insert(DataKind::SurveyData, 100.0);
        state
            .research
            .unlocked
            .insert(TechId("tech_deep_scan_v1".to_string()));

        let mut events = Vec::new();
        let station_id = StationId("station_test".to_string());
        super::tick_lab_modules(&mut state, &station_id, &content, &mut events);

        // Data should be unchanged
        let remaining = state.research.data_pool[&DataKind::SurveyData];
        assert!((remaining - 100.0).abs() < 1e-3, "data should be unchanged");

        // No LabRan events
        let lab_ran = events
            .iter()
            .any(|e| matches!(&e.event, Event::LabRan { .. }));
        assert!(!lab_ran, "should not run lab for unlocked tech");
    }

    #[test]
    fn lab_skips_when_no_tech_assigned() {
        let content = lab_content();
        let mut state = lab_state(&content);
        state.research.data_pool.insert(DataKind::SurveyData, 100.0);

        // Clear assigned tech
        let station_id = StationId("station_test".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        if let ModuleKindState::Lab(ls) = &mut station.modules[0].kind_state {
            ls.assigned_tech = None;
        }

        let mut events = Vec::new();
        super::tick_lab_modules(&mut state, &station_id, &content, &mut events);

        // Data should be unchanged
        let remaining = state.research.data_pool[&DataKind::SurveyData];
        assert!((remaining - 100.0).abs() < 1e-3, "data should be unchanged");

        // No LabRan events
        let lab_ran = events
            .iter()
            .any(|e| matches!(&e.event, Event::LabRan { .. }));
        assert!(!lab_ran, "should not run lab without assigned tech");
    }
}
