use crate::{
    composition::{blend_slag_composition, merge_material_lot, weighted_composition},
    thermal, Event, EventEnvelope, GameContent, GameState, InputAmount, InventoryItem,
    ModuleBehaviorDef, ModuleKindState, OutputSpec, QualityFormula, StationId, YieldFormula,
};
use std::collections::HashMap;

use super::{estimate_output_volume_m3, matches_input_filter, MIN_MEANINGFUL_KG};

pub(super) fn tick_station_modules(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let module_count = state
        .stations
        .get(station_id)
        .map_or(0, |s| s.modules.len());

    for module_idx in 0..module_count {
        let Some(ctx) = super::extract_context(state, station_id, module_idx, content) else {
            continue;
        };

        let ModuleBehaviorDef::Processor(_) = &ctx.def.behavior else {
            continue;
        };

        if !super::should_run(state, &ctx) {
            continue;
        }

        let outcome = execute(&ctx, state, content, events);
        super::apply_run_result(state, &ctx, outcome, events);
    }
}

fn execute(
    ctx: &super::ModuleTickContext,
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) -> super::RunOutcome {
    // Check ore threshold
    let threshold_kg = {
        let Some(station) = state.stations.get(&ctx.station_id) else {
            return super::RunOutcome::Skipped { reset_timer: false };
        };
        match &station.modules[ctx.module_idx].kind_state {
            ModuleKindState::Processor(ps) => ps.threshold_kg,
            _ => return super::RunOutcome::Skipped { reset_timer: false },
        }
    };

    let total_ore_kg: f32 = state.stations.get(&ctx.station_id).map_or(0.0, |s| {
        s.inventory
            .iter()
            .filter_map(|i| {
                if let InventoryItem::Ore { kg, .. } = i {
                    Some(*kg)
                } else {
                    None
                }
            })
            .sum()
    });

    if total_ore_kg < threshold_kg {
        return super::RunOutcome::Skipped { reset_timer: false };
    }

    // Capacity pre-check
    let ModuleBehaviorDef::Processor(processor_def) = &ctx.def.behavior else {
        return super::RunOutcome::Skipped { reset_timer: false };
    };
    let Some(recipe) = processor_def.recipes.first() else {
        return super::RunOutcome::Skipped { reset_timer: false };
    };

    // Thermal gating: check if recipe requires a minimum temperature
    let (thermal_eff, thermal_qual) = if let Some(ref thermal_req) = recipe.thermal_req {
        let temp_mk = state
            .stations
            .get(&ctx.station_id)
            .and_then(|s| s.modules[ctx.module_idx].thermal.as_ref())
            .map_or(0, |t| t.temp_mk);

        if temp_mk < thermal_req.min_temp_mk {
            return super::RunOutcome::Stalled(super::StallReason::TooCold {
                current_temp_mk: temp_mk,
                required_temp_mk: thermal_req.min_temp_mk,
            });
        }

        (
            thermal::thermal_efficiency(temp_mk, thermal_req),
            thermal::thermal_quality_factor(temp_mk, thermal_req),
        )
    } else {
        (1.0, 1.0)
    };

    let rate_kg = match recipe.inputs.first().map(|i| &i.amount) {
        Some(InputAmount::Kg(kg)) => *kg,
        _ => return super::RunOutcome::Skipped { reset_timer: false },
    };

    let input_filter = recipe.inputs.first().map(|i| &i.filter).cloned();

    // Warm the volume cache
    {
        let Some(station_mut) = state.stations.get_mut(&ctx.station_id) else {
            return super::RunOutcome::Skipped { reset_timer: false };
        };
        let _ = station_mut.used_volume_m3(content);
    }

    let Some(station) = state.stations.get(&ctx.station_id) else {
        return super::RunOutcome::Skipped { reset_timer: false };
    };
    let (peeked_kg, lots) = peek_ore_fifo_with_lots(&station.inventory, rate_kg, |item| {
        matches_input_filter(item, input_filter.as_ref())
    });

    if peeked_kg < MIN_MEANINGFUL_KG {
        return super::RunOutcome::Skipped { reset_timer: false };
    }

    let lot_refs: Vec<(&HashMap<String, f32>, f32)> =
        lots.iter().map(|(comp, kg)| (comp, *kg)).collect();
    let avg_composition = weighted_composition(&lot_refs);
    let output_volume =
        estimate_output_volume_m3(recipe, &avg_composition, peeked_kg, content) * thermal_eff;

    let current_used = station
        .cached_inventory_volume_m3
        .unwrap_or_else(|| crate::inventory_volume_m3(&station.inventory, content));
    let capacity = station.cargo_capacity_m3;
    let shortfall = (current_used + output_volume) - capacity;

    if shortfall > 0.0 {
        return super::RunOutcome::Stalled(super::StallReason::VolumeCap {
            shortfall_m3: shortfall,
        });
    }

    // Process the ore
    resolve_processor_run(ctx, state, content, events, thermal_eff, thermal_qual);

    super::RunOutcome::Completed
}

#[allow(clippy::too_many_lines)]
fn resolve_processor_run(
    ctx: &super::ModuleTickContext,
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
    thermal_efficiency: f32,
    thermal_quality: f32,
) {
    let current_tick = state.meta.tick;

    let ModuleBehaviorDef::Processor(processor_def) = &ctx.def.behavior else {
        return;
    };
    let Some(recipe) = processor_def.recipes.first() else {
        return;
    };

    let rate_kg = match recipe.inputs.first().map(|i| &i.amount) {
        Some(InputAmount::Kg(kg)) => *kg,
        _ => return,
    };

    let input_filter = recipe.inputs.first().map(|i| &i.filter).cloned();

    // FIFO-consume ore and compute weighted average composition.
    let (consumed_kg, lots) = {
        let Some(station) = state.stations.get_mut(&ctx.station_id) else {
            return;
        };
        consume_ore_fifo_with_lots(&mut station.inventory, rate_kg, |item| {
            matches_input_filter(item, input_filter.as_ref())
        })
    };

    if consumed_kg < MIN_MEANINGFUL_KG {
        return;
    }

    let lot_refs: Vec<(&HashMap<String, f32>, f32)> =
        lots.iter().map(|(comp, kg)| (comp, *kg)).collect();
    let avg_composition = weighted_composition(&lot_refs);

    let extracted_element: Option<String> = recipe.outputs.iter().find_map(|o| {
        if let OutputSpec::Material { element, .. } = o {
            Some(element.clone())
        } else {
            None
        }
    });

    // Use pre-computed wear efficiency from context
    let efficiency = ctx.efficiency;

    let mut material_kg = 0.0_f32;
    let mut material_quality = 0.0_f32;
    let mut slag_kg = 0.0_f32;

    for output in &recipe.outputs {
        match output {
            OutputSpec::Material {
                element,
                yield_formula,
                quality_formula,
            } => {
                let yield_frac = match yield_formula {
                    YieldFormula::ElementFraction { element: el } => {
                        avg_composition.get(el).copied().unwrap_or(0.0)
                    }
                    YieldFormula::FixedFraction(f) => *f,
                };
                material_kg = consumed_kg * yield_frac * efficiency * thermal_efficiency;
                let base_quality = match quality_formula {
                    QualityFormula::ElementFractionTimesMultiplier {
                        element: el,
                        multiplier,
                    } => (avg_composition.get(el).copied().unwrap_or(0.0) * multiplier)
                        .clamp(0.0, 1.0),
                    QualityFormula::Fixed(q) => *q,
                };
                material_quality = base_quality * thermal_quality;
                if material_kg > MIN_MEANINGFUL_KG {
                    if let Some(station) = state.stations.get_mut(&ctx.station_id) {
                        merge_material_lot(
                            &mut station.inventory,
                            element.clone(),
                            material_kg,
                            material_quality,
                        );
                    }
                }
            }
            OutputSpec::Slag { yield_formula } => {
                let yield_frac = match yield_formula {
                    YieldFormula::FixedFraction(f) => *f,
                    YieldFormula::ElementFraction { element } => {
                        avg_composition.get(element).copied().unwrap_or(0.0)
                    }
                };
                slag_kg = (consumed_kg - material_kg) * yield_frac;

                // Slag composition: non-extracted elements, re-normalized.
                let slag_composition =
                    slag_composition_from_avg(&avg_composition, extracted_element.as_deref());

                if slag_kg > MIN_MEANINGFUL_KG {
                    if let Some(station) = state.stations.get_mut(&ctx.station_id) {
                        let existing = station
                            .inventory
                            .iter_mut()
                            .find(|i| matches!(i, InventoryItem::Slag { .. }));
                        if let Some(InventoryItem::Slag {
                            kg: existing_kg,
                            composition: existing_comp,
                        }) = existing
                        {
                            let blended = blend_slag_composition(
                                existing_comp,
                                *existing_kg,
                                &slag_composition,
                                slag_kg,
                            );
                            *existing_kg += slag_kg;
                            *existing_comp = blended;
                        } else {
                            station.inventory.push(InventoryItem::Slag {
                                kg: slag_kg,
                                composition: slag_composition,
                            });
                        }
                    }
                }
            }
            OutputSpec::Component { .. } | OutputSpec::Ship { .. } => {} // not yet implemented
        }
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::RefineryRan {
            station_id: ctx.station_id.clone(),
            module_id: ctx.module_id.clone(),
            ore_consumed_kg: consumed_kg,
            material_produced_kg: material_kg,
            material_quality,
            slag_produced_kg: slag_kg,
            material_element: extracted_element.unwrap_or_default(),
        },
    ));

    // Apply recipe heat generation to module thermal state
    if let Some(ref thermal_req) = recipe.thermal_req {
        if thermal_req.heat_per_run_j != 0 {
            if let Some(station) = state.stations.get_mut(&ctx.station_id) {
                let module = &mut station.modules[ctx.module_idx];
                if let Some(thermal_def) = content
                    .module_defs
                    .get(&module.def_id)
                    .and_then(|d| d.thermal.as_ref())
                {
                    let delta_mk = thermal::heat_to_temp_delta_mk(
                        thermal_req.heat_per_run_j,
                        thermal_def.heat_capacity_j_per_k,
                    );
                    if let Some(ref mut thermal_state) = module.thermal {
                        if delta_mk >= 0 {
                            #[allow(clippy::cast_sign_loss)]
                            {
                                thermal_state.temp_mk =
                                    thermal_state.temp_mk.saturating_add(delta_mk as u32);
                            }
                        } else {
                            thermal_state.temp_mk = thermal_state
                                .temp_mk
                                .saturating_sub(delta_mk.unsigned_abs());
                        }
                    }
                }
            }
        }
    }
}

/// FIFO-consume up to `rate_kg` from matching Ore items.
/// Returns `(consumed_kg, Vec<(composition, kg_taken)>)` for weighted averaging.
#[allow(clippy::type_complexity)]
fn consume_ore_fifo_with_lots(
    inventory: &mut Vec<InventoryItem>,
    rate_kg: f32,
    filter: impl Fn(&InventoryItem) -> bool,
) -> (f32, Vec<(HashMap<String, f32>, f32)>) {
    let mut remaining = rate_kg;
    let mut consumed_kg = 0.0_f32;
    let mut lots: Vec<(HashMap<String, f32>, f32)> = Vec::new();
    let mut new_inventory: Vec<InventoryItem> = Vec::new();

    for item in inventory.drain(..) {
        if remaining > 0.0 && matches!(item, InventoryItem::Ore { .. }) && filter(&item) {
            let InventoryItem::Ore {
                lot_id,
                asteroid_id,
                kg,
                composition,
            } = item
            else {
                unreachable!()
            };
            let take = kg.min(remaining);
            remaining -= take;
            consumed_kg += take;
            lots.push((composition.clone(), take));
            let leftover = kg - take;
            if leftover > MIN_MEANINGFUL_KG {
                new_inventory.push(InventoryItem::Ore {
                    lot_id,
                    asteroid_id,
                    kg: leftover,
                    composition,
                });
            }
        } else {
            new_inventory.push(item);
        }
    }
    *inventory = new_inventory;
    (consumed_kg, lots)
}

/// Peek at what would be consumed by FIFO without mutating inventory.
/// Returns `(consumed_kg, Vec<(composition, kg_taken)>)`.
#[allow(clippy::type_complexity)]
fn peek_ore_fifo_with_lots(
    inventory: &[InventoryItem],
    rate_kg: f32,
    filter: impl Fn(&InventoryItem) -> bool,
) -> (f32, Vec<(HashMap<String, f32>, f32)>) {
    let mut remaining = rate_kg;
    let mut consumed_kg = 0.0_f32;
    let mut lots: Vec<(HashMap<String, f32>, f32)> = Vec::new();

    for item in inventory {
        if remaining <= 0.0 {
            break;
        }
        if matches!(item, InventoryItem::Ore { .. }) && filter(item) {
            let InventoryItem::Ore {
                kg, composition, ..
            } = item
            else {
                unreachable!()
            };
            let take = kg.min(remaining);
            remaining -= take;
            consumed_kg += take;
            lots.push((composition.clone(), take));
        }
    }
    (consumed_kg, lots)
}

/// Build slag composition from average composition, excluding the extracted element.
fn slag_composition_from_avg(
    avg: &HashMap<String, f32>,
    extracted: Option<&str>,
) -> HashMap<String, f32> {
    let non_extracted_total: f32 = avg
        .iter()
        .filter(|(k, _)| Some(k.as_str()) != extracted)
        .map(|(_, v)| v)
        .sum();

    if non_extracted_total < 1e-6 {
        return HashMap::new();
    }

    avg.iter()
        .filter(|(k, _)| Some(k.as_str()) != extracted)
        .map(|(k, v)| (k.clone(), v / non_extracted_total))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AsteroidId, InventoryItem, LotId};

    #[test]
    fn peek_ore_fifo_does_not_mutate() {
        let inventory = vec![
            InventoryItem::Ore {
                lot_id: LotId("lot_0001".to_string()),
                asteroid_id: AsteroidId("ast_0001".to_string()),
                kg: 600.0,
                composition: HashMap::from([("Fe".to_string(), 0.7), ("Si".to_string(), 0.3)]),
            },
            InventoryItem::Material {
                element: "Fe".to_string(),
                kg: 100.0,
                quality: 0.8,
                thermal: None,
            },
        ];

        let filter = |item: &InventoryItem| matches!(item, InventoryItem::Ore { .. });
        let (consumed_kg, lots) = peek_ore_fifo_with_lots(&inventory, 500.0, filter);

        assert!((consumed_kg - 500.0).abs() < 1e-3);
        assert_eq!(lots.len(), 1);
        assert_eq!(inventory.len(), 2); // unchanged
        if let InventoryItem::Ore { kg, .. } = &inventory[0] {
            assert!((kg - 600.0).abs() < 1e-3, "original should be unchanged");
        }
    }

    #[test]
    fn peek_ore_fifo_consumes_multiple_lots() {
        let inventory = vec![
            InventoryItem::Ore {
                lot_id: LotId("lot_0001".to_string()),
                asteroid_id: AsteroidId("ast_0001".to_string()),
                kg: 200.0,
                composition: HashMap::from([("Fe".to_string(), 0.8)]),
            },
            InventoryItem::Ore {
                lot_id: LotId("lot_0002".to_string()),
                asteroid_id: AsteroidId("ast_0002".to_string()),
                kg: 400.0,
                composition: HashMap::from([("Fe".to_string(), 0.6)]),
            },
        ];

        let filter = |item: &InventoryItem| matches!(item, InventoryItem::Ore { .. });
        let (consumed_kg, lots) = peek_ore_fifo_with_lots(&inventory, 500.0, filter);

        assert!((consumed_kg - 500.0).abs() < 1e-3);
        assert_eq!(lots.len(), 2);
        assert!((lots[0].1 - 200.0).abs() < 1e-3); // first lot fully consumed
        assert!((lots[1].1 - 300.0).abs() < 1e-3); // second lot partially consumed
    }

    // ── Thermal recipe gating integration tests ──────────────────────

    use crate::{
        Counters, InputFilter, MetaState, ModuleDef, ModuleInstanceId, ModuleState, NodeId,
        PowerState, ProcessorDef, ProcessorState, RecipeInput, RecipeThermalReq, StationState,
        ThermalDef, ThermalState, WearState,
    };
    use std::collections::HashSet;

    /// Build content with a processor that has a thermal recipe.
    fn thermal_processor_content() -> GameContent {
        let mut content = crate::test_fixtures::base_content();
        content.module_defs.insert(
            "module_smelter".to_string(),
            ModuleDef {
                id: "module_smelter".to_string(),
                name: "Smelter".to_string(),
                mass_kg: 5000.0,
                volume_m3: 10.0,
                power_consumption_per_run: 10.0,
                wear_per_run: 0.01,
                behavior: ModuleBehaviorDef::Processor(ProcessorDef {
                    processing_interval_minutes: 1,
                    processing_interval_ticks: 1,
                    recipes: vec![crate::RecipeDef {
                        id: "smelt_fe".to_string(),
                        inputs: vec![RecipeInput {
                            filter: InputFilter::ItemKind(crate::ItemKind::Ore),
                            amount: InputAmount::Kg(100.0),
                        }],
                        outputs: vec![OutputSpec::Material {
                            element: "Fe".to_string(),
                            yield_formula: YieldFormula::ElementFraction {
                                element: "Fe".to_string(),
                            },
                            quality_formula: QualityFormula::Fixed(0.9),
                        }],
                        efficiency: 1.0,
                        thermal_req: Some(RecipeThermalReq {
                            min_temp_mk: 1_000_000,    // 1000K
                            optimal_min_mk: 1_500_000, // 1500K
                            optimal_max_mk: 2_000_000, // 2000K
                            max_temp_mk: 2_500_000,    // 2500K
                            heat_per_run_j: 50_000,
                        }),
                    }],
                }),
                thermal: Some(ThermalDef {
                    heat_capacity_j_per_k: 500.0,
                    passive_cooling_coefficient: 0.05,
                    max_temp_mk: 3_000_000,
                    operating_min_mk: None,
                    operating_max_mk: None,
                    thermal_group: Some("smelting".to_string()),
                }),
            },
        );
        content
    }

    /// Build state with a thermal processor module at the given temperature.
    fn thermal_processor_state(content: &GameContent, temp_mk: u32) -> GameState {
        let station_id = StationId("station_test".to_string());
        GameState {
            meta: MetaState {
                tick: 10,
                seed: 42,
                schema_version: 1,
                content_version: content.content_version.clone(),
            },
            scan_sites: vec![],
            asteroids: HashMap::new(),
            ships: HashMap::new(),
            stations: HashMap::from([(
                station_id.clone(),
                StationState {
                    id: station_id,
                    location_node: NodeId("node_test".to_string()),
                    inventory: vec![InventoryItem::Ore {
                        lot_id: LotId("lot_0001".to_string()),
                        asteroid_id: AsteroidId("ast_0001".to_string()),
                        kg: 500.0,
                        composition: HashMap::from([
                            ("Fe".to_string(), 0.7),
                            ("Si".to_string(), 0.3),
                        ]),
                    }],
                    cargo_capacity_m3: 10_000.0,
                    power_available_per_tick: 100.0,
                    modules: vec![ModuleState {
                        id: ModuleInstanceId("smelter_0001".to_string()),
                        def_id: "module_smelter".to_string(),
                        enabled: true,
                        kind_state: ModuleKindState::Processor(ProcessorState {
                            threshold_kg: 0.0,
                            ticks_since_last_run: 0,
                            stalled: false,
                        }),
                        wear: WearState::default(),
                        power_stalled: false,
                        thermal: Some(ThermalState {
                            temp_mk,
                            thermal_group: Some("smelting".to_string()),
                        }),
                    }],
                    power: PowerState::default(),
                    cached_inventory_volume_m3: None,
                },
            )]),
            research: crate::ResearchState {
                unlocked: HashSet::new(),
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
        }
    }

    #[test]
    fn cold_processor_stalls_with_too_cold() {
        let content = thermal_processor_content();
        let mut state = thermal_processor_state(&content, 293_000); // 293K, way below 1000K min
        let station_id = StationId("station_test".to_string());
        let mut events = Vec::new();

        tick_station_modules(&mut state, &station_id, &content, &mut events);

        // Should have emitted ProcessorTooCold
        assert!(
            events
                .iter()
                .any(|e| matches!(&e.event, Event::ProcessorTooCold { .. })),
            "expected ProcessorTooCold event"
        );
        // No RefineryRan event
        assert!(
            !events
                .iter()
                .any(|e| matches!(&e.event, Event::RefineryRan { .. })),
            "should not have run refinery when too cold"
        );
        // Ore should be unchanged
        let station = state.stations.get(&station_id).unwrap();
        let ore_kg: f32 = station
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Ore { kg, .. } => Some(*kg),
                _ => None,
            })
            .sum();
        assert!((ore_kg - 500.0).abs() < 1e-3, "ore should not be consumed");
    }

    #[test]
    fn hot_processor_runs_at_full_efficiency() {
        let content = thermal_processor_content();
        // 1800K — in optimal range (1500K–2000K)
        let mut state = thermal_processor_state(&content, 1_800_000);
        let station_id = StationId("station_test".to_string());
        let mut events = Vec::new();

        tick_station_modules(&mut state, &station_id, &content, &mut events);

        // Should have run
        let refinery_event = events.iter().find_map(|e| {
            if let Event::RefineryRan {
                material_produced_kg,
                material_quality,
                ..
            } = &e.event
            {
                Some((*material_produced_kg, *material_quality))
            } else {
                None
            }
        });
        assert!(refinery_event.is_some(), "expected RefineryRan event");
        let (material_kg, quality) = refinery_event.unwrap();

        // 100 kg ore * 0.7 Fe fraction * 1.0 wear eff * 1.0 thermal eff = 70 kg
        assert!(
            (material_kg - 70.0).abs() < 1.0,
            "expected ~70 kg material, got {material_kg}"
        );
        // Quality = Fixed(0.9) * 1.0 thermal quality = 0.9
        assert!(
            (quality - 0.9).abs() < 0.01,
            "expected ~0.9 quality, got {quality}"
        );
    }

    #[test]
    fn processor_at_min_temp_has_reduced_efficiency() {
        let content = thermal_processor_content();
        // Exactly at min_temp_mk (1000K) → 80% efficiency
        let mut state = thermal_processor_state(&content, 1_000_000);
        let station_id = StationId("station_test".to_string());
        let mut events = Vec::new();

        tick_station_modules(&mut state, &station_id, &content, &mut events);

        let material_kg = events.iter().find_map(|e| {
            if let Event::RefineryRan {
                material_produced_kg,
                ..
            } = &e.event
            {
                Some(*material_produced_kg)
            } else {
                None
            }
        });
        assert!(material_kg.is_some(), "expected RefineryRan event");
        // 100 kg * 0.7 * 1.0 wear * 0.8 thermal eff = 56 kg
        let kg = material_kg.unwrap();
        assert!(
            (kg - 56.0).abs() < 1.0,
            "expected ~56 kg at 80% efficiency, got {kg}"
        );
    }

    #[test]
    fn processor_above_max_has_degraded_quality() {
        let content = thermal_processor_content();
        // 3000K — above max_temp (2500K) → quality = 0.3
        let mut state = thermal_processor_state(&content, 3_000_000);
        let station_id = StationId("station_test".to_string());
        let mut events = Vec::new();

        tick_station_modules(&mut state, &station_id, &content, &mut events);

        let quality = events.iter().find_map(|e| {
            if let Event::RefineryRan {
                material_quality, ..
            } = &e.event
            {
                Some(*material_quality)
            } else {
                None
            }
        });
        assert!(quality.is_some(), "expected RefineryRan event");
        // Fixed(0.9) * 0.3 thermal quality = 0.27
        let q = quality.unwrap();
        assert!(
            (q - 0.27).abs() < 0.05,
            "expected ~0.27 quality above max, got {q}"
        );
    }

    #[test]
    fn heat_generation_increases_module_temp() {
        let content = thermal_processor_content();
        let mut state = thermal_processor_state(&content, 1_800_000);
        let station_id = StationId("station_test".to_string());
        let mut events = Vec::new();

        tick_station_modules(&mut state, &station_id, &content, &mut events);

        // Verify temp increased (heat_per_run_j = 50_000, capacity = 500 J/K → +100K = +100_000 mK)
        let temp_after = state.stations.get(&station_id).unwrap().modules[0]
            .thermal
            .as_ref()
            .unwrap()
            .temp_mk;
        assert!(
            temp_after > 1_800_000,
            "temperature should increase after exothermic run, got {temp_after}"
        );
        // 50_000 J / 500 J/K = 100 K = 100_000 mK
        assert!(
            (temp_after - 1_900_000) < 1_000,
            "expected ~1_900_000 mK, got {temp_after}"
        );
    }

    #[test]
    fn recipe_without_thermal_req_runs_normally() {
        let mut content = thermal_processor_content();
        // Remove thermal_req from the recipe
        if let ModuleBehaviorDef::Processor(ref mut p) = content
            .module_defs
            .get_mut("module_smelter")
            .unwrap()
            .behavior
        {
            p.recipes[0].thermal_req = None;
        }
        // Module at room temp — should still run fine
        let mut state = thermal_processor_state(&content, 293_000);
        let station_id = StationId("station_test".to_string());
        let mut events = Vec::new();

        tick_station_modules(&mut state, &station_id, &content, &mut events);

        assert!(
            events
                .iter()
                .any(|e| matches!(&e.event, Event::RefineryRan { .. })),
            "should run without thermal_req regardless of temperature"
        );
    }

    #[test]
    fn thermal_recipe_without_thermal_state_stalls() {
        // Module has a thermal recipe but no ThermalState → temp defaults to 0 → TooCold
        let content = thermal_processor_content();
        let mut state = thermal_processor_state(&content, 1_800_000);
        let station_id = StationId("station_test".to_string());

        // Remove ThermalState from the module
        state.stations.get_mut(&station_id).unwrap().modules[0].thermal = None;

        let mut events = Vec::new();
        tick_station_modules(&mut state, &station_id, &content, &mut events);

        assert!(
            events
                .iter()
                .any(|e| matches!(&e.event, Event::ProcessorTooCold { .. })),
            "missing ThermalState with thermal recipe should stall as TooCold"
        );
    }
}
