use crate::{
    composition::{blend_slag_composition, merge_material_lot, weighted_composition},
    thermal, Event, EventEnvelope, GameContent, GameState, InputAmount, InventoryItem,
    ModuleBehaviorDef, ModuleKindState, OutputSpec, QualityFormula, StationId, YieldFormula,
};
use std::collections::HashMap;

use super::{estimate_output_volume_m3, matches_input_filter};

pub(super) fn tick_station_modules(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
    scratch: &mut Vec<usize>,
) {
    super::ensure_station_index(state, station_id, content);
    // Use pre-computed processor indices, then sort by priority
    scratch.clear();
    if let Some(station) = state.stations.get(station_id) {
        scratch.extend_from_slice(&station.module_type_index.processors);
        scratch.sort_by(|&a, &b| {
            let ma = &station.modules[a];
            let mb = &station.modules[b];
            mb.manufacturing_priority
                .cmp(&ma.manufacturing_priority)
                .then_with(|| ma.id.0.cmp(&mb.id.0))
        });
    }

    for &module_idx in scratch.iter() {
        let Some(ctx) = super::extract_context(state, station_id, module_idx, content) else {
            continue;
        };

        if !super::should_run(state, &ctx) {
            continue;
        }

        let outcome = execute(&ctx, state, content, events);
        super::apply_run_result(state, &ctx, outcome, content, events);
    }
}

fn execute(
    ctx: &super::ModuleTickContext,
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) -> super::RunOutcome {
    // Capacity pre-check
    let ModuleBehaviorDef::Processor(processor_def) = &ctx.def.behavior else {
        return super::RunOutcome::Skipped { reset_timer: false };
    };

    // Read processor state for threshold and selected recipe
    let threshold_kg = {
        let Some(station) = state.stations.get(&ctx.station_id) else {
            return super::RunOutcome::Skipped { reset_timer: false };
        };
        match &station.modules[ctx.module_idx].kind_state {
            ModuleKindState::Processor(ps) => ps.threshold_kg,
            _ => return super::RunOutcome::Skipped { reset_timer: false },
        }
    };

    let recipe = resolve_recipe(state, ctx, processor_def, content, events);
    let Some(recipe) = recipe else {
        return super::RunOutcome::Skipped { reset_timer: false };
    };

    // Tech gate: if recipe requires a tech that isn't unlocked, skip
    if let Some(outcome) = check_tech_gate(state, ctx, processor_def, recipe, events) {
        return outcome;
    }

    let input_filter_for_threshold = recipe.inputs.first().map(|i| &i.filter);
    let total_input_kg: f32 = state.stations.get(&ctx.station_id).map_or(0.0, |s| {
        s.inventory
            .iter()
            .filter(|item| matches_input_filter(item, input_filter_for_threshold))
            .map(InventoryItem::mass_kg)
            .sum()
    });

    if total_input_kg < threshold_kg {
        return super::RunOutcome::Skipped { reset_timer: false };
    }

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

    if peeked_kg < content.constants.min_meaningful_kg {
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

    // Process the ore — pass the already-resolved recipe ID to avoid double resolution
    let recipe_id = recipe.id.clone();
    resolve_processor_run(
        ctx,
        state,
        content,
        events,
        thermal_eff,
        thermal_qual,
        &recipe_id,
    );

    super::RunOutcome::Completed
}

/// Shared context for processor output emission helpers.
struct ProcessorRunCtx<'a> {
    station_id: &'a StationId,
    avg_composition: &'a HashMap<String, f32>,
    consumed_kg: f32,
    proc_mods: &'a crate::modifiers::ModifierSet,
    min_meaningful_kg: f32,
}

fn build_processor_modifiers(
    wear_efficiency: f32,
    thermal_efficiency: f32,
    thermal_quality: f32,
) -> crate::modifiers::ModifierSet {
    let mut mods = crate::modifiers::ModifierSet::new();
    mods.add(crate::modifiers::Modifier::pct_mult(
        crate::modifiers::StatId::ProcessingYield,
        f64::from(wear_efficiency),
        crate::modifiers::ModifierSource::Wear,
    ));
    mods.add(crate::modifiers::Modifier::pct_mult(
        crate::modifiers::StatId::ProcessingYield,
        f64::from(thermal_efficiency),
        crate::modifiers::ModifierSource::Thermal,
    ));
    mods.add(crate::modifiers::Modifier::pct_mult(
        crate::modifiers::StatId::ProcessingQuality,
        f64::from(thermal_quality),
        crate::modifiers::ModifierSource::Thermal,
    ));
    mods
}

fn resolve_processor_run(
    ctx: &super::ModuleTickContext,
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
    thermal_efficiency: f32,
    thermal_quality: f32,
    recipe_id: &crate::RecipeId,
) {
    let current_tick = state.meta.tick;
    let Some(recipe) = content.recipes.get(recipe_id) else {
        return;
    };
    let rate_kg = match recipe.inputs.first().map(|i| &i.amount) {
        Some(InputAmount::Kg(kg)) => *kg,
        _ => return,
    };
    let input_filter = recipe.inputs.first().map(|i| &i.filter).cloned();
    let min_kg = content.constants.min_meaningful_kg;

    let (consumed_kg, lots) = {
        let Some(station) = state.stations.get_mut(&ctx.station_id) else {
            return;
        };
        consume_ore_fifo_with_lots(&mut station.inventory, rate_kg, min_kg, |item| {
            matches_input_filter(item, input_filter.as_ref())
        })
    };
    if consumed_kg < min_kg {
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

    let proc_mods = build_processor_modifiers(ctx.efficiency, thermal_efficiency, thermal_quality);
    let run_ctx = ProcessorRunCtx {
        station_id: &ctx.station_id,
        avg_composition: &avg_composition,
        consumed_kg,
        proc_mods: &proc_mods,
        min_meaningful_kg: min_kg,
    };

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
                (material_kg, material_quality) =
                    emit_material_output(state, &run_ctx, element, yield_formula, quality_formula);
            }
            OutputSpec::Slag { yield_formula } => {
                slag_kg = emit_slag_output(
                    state,
                    &run_ctx,
                    yield_formula,
                    material_kg,
                    extracted_element.as_deref(),
                );
            }
            OutputSpec::Component {
                component_id,
                quality_formula,
            } => {
                emit_component_output(state, &run_ctx, component_id, quality_formula);
            }
            OutputSpec::Ship { .. } => {}
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
    apply_recipe_heat(state, ctx, content, recipe);
}

/// Compute material output from a processor run and merge into station inventory.
/// Returns `(material_kg, material_quality)`.
fn emit_material_output(
    state: &mut GameState,
    run: &ProcessorRunCtx,
    element: &str,
    yield_formula: &YieldFormula,
    quality_formula: &QualityFormula,
) -> (f32, f32) {
    let yield_frac = match yield_formula {
        YieldFormula::ElementFraction { element: el } => {
            run.avg_composition.get(el).copied().unwrap_or(0.0)
        }
        YieldFormula::FixedFraction(f) => *f,
    };
    let material_kg = run.proc_mods.resolve_with_f32(
        crate::modifiers::StatId::ProcessingYield,
        run.consumed_kg * yield_frac,
        &state.modifiers,
    );
    let base_quality = match quality_formula {
        QualityFormula::ElementFractionTimesMultiplier {
            element: el,
            multiplier,
        } => (run.avg_composition.get(el).copied().unwrap_or(0.0) * multiplier).clamp(0.0, 1.0),
        QualityFormula::Fixed(q) => *q,
    };
    let material_quality = run.proc_mods.resolve_with_f32(
        crate::modifiers::StatId::ProcessingQuality,
        base_quality,
        &state.modifiers,
    );
    if material_kg > run.min_meaningful_kg {
        if let Some(station) = state.stations.get_mut(run.station_id) {
            merge_material_lot(
                &mut station.inventory,
                element.to_string(),
                material_kg,
                material_quality,
                None,
            );
        }
    }
    (material_kg, material_quality)
}

/// Compute slag output from a processor run and merge into station inventory.
/// Returns the slag mass produced.
fn emit_slag_output(
    state: &mut GameState,
    run: &ProcessorRunCtx,
    yield_formula: &YieldFormula,
    material_kg: f32,
    extracted_element: Option<&str>,
) -> f32 {
    let yield_frac = match yield_formula {
        YieldFormula::FixedFraction(f) => *f,
        YieldFormula::ElementFraction { element } => {
            run.avg_composition.get(element).copied().unwrap_or(0.0)
        }
    };
    let slag_kg = (run.consumed_kg - material_kg) * yield_frac;
    let slag_composition = slag_composition_from_avg(run.avg_composition, extracted_element);

    if slag_kg > run.min_meaningful_kg {
        if let Some(station) = state.stations.get_mut(run.station_id) {
            let existing = station.inventory.iter_mut().find(|i| i.is_slag());
            if let Some(InventoryItem::Slag {
                kg: existing_kg,
                composition: existing_comp,
            }) = existing
            {
                let blended =
                    blend_slag_composition(existing_comp, *existing_kg, &slag_composition, slag_kg);
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
    slag_kg
}

/// Produce a component from a processor run and add to station inventory.
fn emit_component_output(
    state: &mut GameState,
    run: &ProcessorRunCtx,
    component_id: &crate::ComponentId,
    quality_formula: &QualityFormula,
) {
    let base_quality = match quality_formula {
        QualityFormula::Fixed(q) => *q,
        QualityFormula::ElementFractionTimesMultiplier {
            element,
            multiplier,
        } => (run
            .avg_composition
            .get(element.as_str())
            .copied()
            .unwrap_or(0.0)
            * multiplier)
            .clamp(0.0, 1.0),
    };
    let quality = run.proc_mods.resolve_with_f32(
        crate::modifiers::StatId::ProcessingQuality,
        base_quality,
        &state.modifiers,
    );
    let produced_count = 1u32;
    if let Some(station) = state.stations.get_mut(run.station_id) {
        let existing = station.inventory.iter_mut().find(|i| {
            matches!(i, InventoryItem::Component { component_id: cid, quality: q, .. }
                if cid.0 == component_id.0 && (*q - quality).abs() < 1e-3)
        });
        if let Some(InventoryItem::Component { count, .. }) = existing {
            *count += produced_count;
        } else {
            station.inventory.push(InventoryItem::Component {
                component_id: component_id.clone(),
                count: produced_count,
                quality,
            });
        }
    }
}

/// Apply recipe heat generation to the module's thermal state after a successful run.
fn apply_recipe_heat(
    state: &mut GameState,
    ctx: &super::ModuleTickContext,
    content: &GameContent,
    recipe: &crate::RecipeDef,
) {
    let Some(ref thermal_req) = recipe.thermal_req else {
        return;
    };
    if thermal_req.heat_per_run_j == 0 {
        return;
    }
    let Some(station) = state.stations.get_mut(&ctx.station_id) else {
        return;
    };
    let module = &mut station.modules[ctx.module_idx];
    let Some(thermal_def) = content
        .module_defs
        .get(&module.def_id)
        .and_then(|d| d.thermal.as_ref())
    else {
        return;
    };
    let delta_mk = thermal::heat_to_temp_delta_mk(
        thermal_req.heat_per_run_j,
        thermal_def.heat_capacity_j_per_k,
    );
    if let Some(ref mut thermal_state) = module.thermal {
        if delta_mk >= 0 {
            #[allow(clippy::cast_sign_loss)] // .max(0) guarantees non-negative
            let delta = delta_mk.max(0) as u32;
            thermal_state.temp_mk = thermal_state.temp_mk.saturating_add(delta);
        } else {
            thermal_state.temp_mk = thermal_state
                .temp_mk
                .saturating_sub(delta_mk.unsigned_abs());
        }
    }
}

/// Check if the recipe requires a tech that isn't unlocked.
/// Returns `Some(RunOutcome)` if the tech gate blocks execution, `None` if OK to proceed.
fn check_tech_gate(
    state: &mut GameState,
    ctx: &super::ModuleTickContext,
    processor_def: &crate::ProcessorDef,
    recipe: &crate::RecipeDef,
    events: &mut Vec<EventEnvelope>,
) -> Option<super::RunOutcome> {
    let required_tech = recipe.required_tech.as_ref()?;
    if state.research.unlocked.iter().any(|t| t == required_tech) {
        return None; // Tech unlocked — no gate
    }

    // Emit ModuleAwaitingTech once per interval
    let first_trigger = state
        .stations
        .get(&ctx.station_id)
        .and_then(|s| match &s.modules[ctx.module_idx].kind_state {
            ModuleKindState::Processor(ps) => {
                Some(ps.ticks_since_last_run == processor_def.processing_interval_ticks)
            }
            _ => None,
        })
        .unwrap_or(false);
    if first_trigger {
        let current_tick = state.meta.tick;
        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            Event::ModuleAwaitingTech {
                station_id: ctx.station_id.clone(),
                module_id: ctx.module_id.clone(),
                tech_id: required_tech.clone(),
            },
        ));
    }
    Some(super::RunOutcome::Skipped { reset_timer: false })
}

/// Resolve the active recipe for a processor module.
/// Uses `selected_recipe` from state if set, otherwise falls back to the first recipe in the
/// module's recipe list. If the selected recipe is not in the processor's recipe list, resets
/// to the first recipe and emits a `RecipeSelectionReset` event. Returns `None` if no valid
/// recipe can be resolved.
fn resolve_recipe<'a>(
    state: &mut GameState,
    ctx: &super::ModuleTickContext,
    processor_def: &crate::ProcessorDef,
    content: &'a GameContent,
    events: &mut Vec<EventEnvelope>,
) -> Option<&'a crate::RecipeDef> {
    let selected = state.stations.get(&ctx.station_id).and_then(|s| {
        match &s.modules[ctx.module_idx].kind_state {
            crate::ModuleKindState::Processor(ps) => ps.selected_recipe.clone(),
            _ => None,
        }
    });

    if let Some(ref sel_id) = selected {
        if !processor_def.recipes.contains(sel_id) {
            // Invalid selection — fall back to first recipe and emit reset event
            let new_recipe = processor_def.recipes.first().cloned();
            if let Some(station) = state.stations.get_mut(&ctx.station_id) {
                if let ModuleKindState::Processor(ps) =
                    &mut station.modules[ctx.module_idx].kind_state
                {
                    ps.selected_recipe.clone_from(&new_recipe);
                }
            }
            let current_tick = state.meta.tick;
            if let Some(ref new_id) = new_recipe {
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    Event::RecipeSelectionReset {
                        station_id: ctx.station_id.clone(),
                        module_id: ctx.module_id.clone(),
                        old_recipe: sel_id.clone(),
                        new_recipe: new_id.clone(),
                    },
                ));
            }
            return new_recipe.as_ref().and_then(|id| content.recipes.get(id));
        }
    }

    let recipe_id = selected.as_ref().or_else(|| processor_def.recipes.first());
    recipe_id.and_then(|id| content.recipes.get(id))
}

/// FIFO-consume up to `rate_kg` from matching inventory items (Ore or Material).
/// Returns `(consumed_kg, Vec<(composition, kg_taken)>)` for weighted averaging.
#[allow(clippy::type_complexity)]
fn consume_ore_fifo_with_lots(
    inventory: &mut Vec<InventoryItem>,
    rate_kg: f32,
    min_meaningful_kg: f32,
    filter: impl Fn(&InventoryItem) -> bool,
) -> (f32, Vec<(HashMap<String, f32>, f32)>) {
    let mut remaining = rate_kg;
    let mut consumed_kg = 0.0_f32;
    let mut lots: Vec<(HashMap<String, f32>, f32)> = Vec::new();
    let mut new_inventory: Vec<InventoryItem> = Vec::new();

    for item in inventory.drain(..) {
        if remaining <= 0.0 || !filter(&item) {
            new_inventory.push(item);
            continue;
        }
        match item {
            InventoryItem::Ore {
                lot_id,
                asteroid_id,
                kg,
                composition,
            } => {
                let take = kg.min(remaining);
                remaining -= take;
                consumed_kg += take;
                lots.push((composition.clone(), take));
                let leftover = kg - take;
                if leftover > min_meaningful_kg {
                    new_inventory.push(InventoryItem::Ore {
                        lot_id,
                        asteroid_id,
                        kg: leftover,
                        composition,
                    });
                }
            }
            InventoryItem::Material {
                element,
                kg,
                quality,
                thermal,
            } => {
                let take = kg.min(remaining);
                remaining -= take;
                consumed_kg += take;
                lots.push((HashMap::from([(element.clone(), 1.0)]), take));
                let leftover = kg - take;
                if leftover > min_meaningful_kg {
                    new_inventory.push(InventoryItem::Material {
                        element,
                        kg: leftover,
                        quality,
                        thermal,
                    });
                }
            }
            other => {
                new_inventory.push(other);
            }
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
        if !filter(item) {
            continue;
        }
        match item {
            InventoryItem::Ore {
                kg, composition, ..
            } => {
                let take = kg.min(remaining);
                remaining -= take;
                consumed_kg += take;
                lots.push((composition.clone(), take));
            }
            InventoryItem::Material { element, kg, .. } => {
                let take = kg.min(remaining);
                remaining -= take;
                consumed_kg += take;
                lots.push((HashMap::from([(element.clone(), 1.0)]), take));
            }
            _ => {}
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
    use crate::AHashMap;
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

        let filter = |item: &InventoryItem| item.is_ore();
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

        let filter = |item: &InventoryItem| item.is_ore();
        let (consumed_kg, lots) = peek_ore_fifo_with_lots(&inventory, 500.0, filter);

        assert!((consumed_kg - 500.0).abs() < 1e-3);
        assert_eq!(lots.len(), 2);
        assert!((lots[0].1 - 200.0).abs() < 1e-3); // first lot fully consumed
        assert!((lots[1].1 - 300.0).abs() < 1e-3); // second lot partially consumed
    }

    // ── Thermal recipe gating integration tests ──────────────────────

    use crate::{
        Counters, InputFilter, MetaState, ModuleDef, ModuleInstanceId, ModuleState, PowerState,
        ProcessorDef, ProcessorState, RecipeId, RecipeInput, RecipeThermalReq, StationState,
        ThermalDef, ThermalState, WearState,
    };
    use std::collections::HashSet;

    /// Build content with a processor that has a thermal recipe.
    fn thermal_processor_content() -> GameContent {
        let mut content = crate::test_fixtures::base_content();
        let smelt_recipe = crate::RecipeDef {
            id: RecipeId("smelt_fe".to_string()),
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
                efficiency_floor: 0.8,
                quality_floor: 0.3,
                quality_at_max: 0.6,
            }),
            required_tech: None,
            tags: vec![],
        };
        let recipe_id = crate::test_fixtures::insert_recipe(&mut content, smelt_recipe);
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
                    recipes: vec![recipe_id],
                }),
                thermal: Some(ThermalDef {
                    heat_capacity_j_per_k: 500.0,
                    passive_cooling_coefficient: 0.05,
                    max_temp_mk: 3_000_000,
                    operating_min_mk: None,
                    operating_max_mk: None,
                    thermal_group: Some("smelting".to_string()),
                    idle_heat_generation_w: None,
                }),
                compatible_slots: Vec::new(),
                ship_modifiers: Vec::new(),
                power_stall_priority: None,
                roles: vec![],
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
            asteroids: AHashMap::default(),
            ships: AHashMap::default(),
            stations: [(
                station_id.clone(),
                StationState {
                    id: station_id,
                    position: crate::test_fixtures::test_position(),
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
                            selected_recipe: None,
                        }),
                        wear: WearState::default(),
                        power_stalled: false,
                        manufacturing_priority: 0,
                        thermal: Some(ThermalState {
                            temp_mk,
                            thermal_group: Some("smelting".to_string()),
                            ..Default::default()
                        }),
                    }],
                    modifiers: crate::modifiers::ModifierSet::default(),
                    power: PowerState::default(),
                    cached_inventory_volume_m3: None,
                    module_type_index: crate::ModuleTypeIndex::default(),
                },
            )]
            .into_iter()
            .collect(),
            research: crate::ResearchState {
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
            body_cache: AHashMap::default(),
        }
    }

    #[test]
    fn cold_processor_stalls_with_too_cold() {
        let content = thermal_processor_content();
        let mut state = thermal_processor_state(&content, 293_000); // 293K, way below 1000K min
        let station_id = StationId("station_test".to_string());
        let mut events = Vec::new();

        tick_station_modules(
            &mut state,
            &station_id,
            &content,
            &mut events,
            &mut Vec::new(),
        );

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

        tick_station_modules(
            &mut state,
            &station_id,
            &content,
            &mut events,
            &mut Vec::new(),
        );

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

        tick_station_modules(
            &mut state,
            &station_id,
            &content,
            &mut events,
            &mut Vec::new(),
        );

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

        tick_station_modules(
            &mut state,
            &station_id,
            &content,
            &mut events,
            &mut Vec::new(),
        );

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

        tick_station_modules(
            &mut state,
            &station_id,
            &content,
            &mut events,
            &mut Vec::new(),
        );

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
        // Remove thermal_req from the recipe in the catalog
        let recipe_id = RecipeId("smelt_fe".to_string());
        if let Some(recipe) = content.recipes.get_mut(&recipe_id) {
            recipe.thermal_req = None;
        }
        // Module at room temp — should still run fine
        let mut state = thermal_processor_state(&content, 293_000);
        let station_id = StationId("station_test".to_string());
        let mut events = Vec::new();

        tick_station_modules(
            &mut state,
            &station_id,
            &content,
            &mut events,
            &mut Vec::new(),
        );

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
        tick_station_modules(
            &mut state,
            &station_id,
            &content,
            &mut events,
            &mut Vec::new(),
        );

        assert!(
            events
                .iter()
                .any(|e| matches!(&e.event, Event::ProcessorTooCold { .. })),
            "missing ThermalState with thermal recipe should stall as TooCold"
        );
    }

    // ── Manufacturing priority tests ─────────────────────────────

    /// Build content with two processor modules sharing the same recipe.
    fn priority_content() -> GameContent {
        let mut content = crate::test_fixtures::base_content();
        let recipe = crate::RecipeDef {
            id: RecipeId("recipe_ore_to_fe".to_string()),
            inputs: vec![crate::RecipeInput {
                filter: crate::InputFilter::ItemKind(crate::ItemKind::Ore),
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
            thermal_req: None,
            required_tech: None,
            tags: vec![],
        };
        let recipe_id = crate::test_fixtures::insert_recipe(&mut content, recipe);
        content.module_defs.insert(
            "module_refinery".to_string(),
            crate::ModuleDef {
                id: "module_refinery".to_string(),
                name: "Refinery".to_string(),
                mass_kg: 1000.0,
                volume_m3: 5.0,
                power_consumption_per_run: 10.0,
                wear_per_run: 0.0,
                behavior: crate::ModuleBehaviorDef::Processor(crate::ProcessorDef {
                    processing_interval_minutes: 1,
                    processing_interval_ticks: 1,
                    recipes: vec![recipe_id],
                }),
                thermal: None,
                compatible_slots: Vec::new(),
                ship_modifiers: Vec::new(),
                power_stall_priority: None,
                roles: vec![],
            },
        );
        content
    }

    #[test]
    fn higher_priority_processor_consumes_first() {
        let content = priority_content();
        let station_id = StationId("station_test".to_string());
        let mut state = GameState {
            meta: crate::MetaState {
                tick: 10,
                seed: 42,
                schema_version: 1,
                content_version: content.content_version.clone(),
            },
            scan_sites: vec![],
            asteroids: AHashMap::default(),
            ships: AHashMap::default(),
            stations: [(
                station_id.clone(),
                crate::StationState {
                    id: station_id.clone(),
                    position: crate::test_fixtures::test_position(),
                    // Only 100 kg ore — enough for exactly one processor run
                    inventory: vec![crate::InventoryItem::Ore {
                        lot_id: crate::LotId("lot_0001".to_string()),
                        asteroid_id: crate::AsteroidId("ast_0001".to_string()),
                        kg: 100.0,
                        composition: HashMap::from([
                            ("Fe".to_string(), 0.7),
                            ("Si".to_string(), 0.3),
                        ]),
                    }],
                    cargo_capacity_m3: 10_000.0,
                    power_available_per_tick: 100.0,
                    modules: vec![
                        // Low-priority processor (id comes first alphabetically)
                        ModuleState {
                            id: ModuleInstanceId("proc_aaa".to_string()),
                            def_id: "module_refinery".to_string(),
                            enabled: true,
                            kind_state: ModuleKindState::Processor(ProcessorState {
                                threshold_kg: 0.0,
                                ticks_since_last_run: 0,
                                stalled: false,
                                selected_recipe: None,
                            }),
                            wear: crate::WearState::default(),
                            thermal: None,
                            power_stalled: false,
                            manufacturing_priority: 0,
                        },
                        // High-priority processor
                        ModuleState {
                            id: ModuleInstanceId("proc_bbb".to_string()),
                            def_id: "module_refinery".to_string(),
                            enabled: true,
                            kind_state: ModuleKindState::Processor(ProcessorState {
                                threshold_kg: 0.0,
                                ticks_since_last_run: 0,
                                stalled: false,
                                selected_recipe: None,
                            }),
                            wear: crate::WearState::default(),
                            thermal: None,
                            power_stalled: false,
                            manufacturing_priority: 10,
                        },
                    ],
                    modifiers: crate::modifiers::ModifierSet::default(),
                    power: PowerState::default(),
                    cached_inventory_volume_m3: None,
                    module_type_index: crate::ModuleTypeIndex::default(),
                },
            )]
            .into_iter()
            .collect(),
            research: crate::ResearchState {
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
            body_cache: AHashMap::default(),
        };

        let mut events = Vec::new();
        tick_station_modules(
            &mut state,
            &station_id,
            &content,
            &mut events,
            &mut Vec::new(),
        );

        // The high-priority processor (proc_bbb) should run first and consume the ore
        let ran_events: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let Event::RefineryRan { module_id, .. } = &e.event {
                    Some(module_id.0.clone())
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(ran_events.len(), 1, "exactly one processor should run");
        assert_eq!(
            ran_events[0], "proc_bbb",
            "high-priority processor should run first"
        );
    }

    // ── Recipe fallback tests ────────────────────────────────────

    #[test]
    fn invalid_selected_recipe_falls_back_and_emits_reset() {
        let content = priority_content();
        let station_id = StationId("station_test".to_string());
        let first_recipe = content
            .module_defs
            .get("module_refinery")
            .and_then(|d| {
                if let crate::ModuleBehaviorDef::Processor(p) = &d.behavior {
                    p.recipes.first().cloned()
                } else {
                    None
                }
            })
            .unwrap();

        let mut state = GameState {
            meta: crate::MetaState {
                tick: 10,
                seed: 42,
                schema_version: 1,
                content_version: content.content_version.clone(),
            },
            scan_sites: vec![],
            asteroids: AHashMap::default(),
            ships: AHashMap::default(),
            stations: [(
                station_id.clone(),
                crate::StationState {
                    id: station_id.clone(),
                    position: crate::test_fixtures::test_position(),
                    inventory: vec![crate::InventoryItem::Ore {
                        lot_id: crate::LotId("lot_0001".to_string()),
                        asteroid_id: crate::AsteroidId("ast_0001".to_string()),
                        kg: 500.0,
                        composition: HashMap::from([
                            ("Fe".to_string(), 0.7),
                            ("Si".to_string(), 0.3),
                        ]),
                    }],
                    cargo_capacity_m3: 10_000.0,
                    power_available_per_tick: 100.0,
                    modules: vec![ModuleState {
                        id: ModuleInstanceId("proc_0001".to_string()),
                        def_id: "module_refinery".to_string(),
                        enabled: true,
                        kind_state: ModuleKindState::Processor(ProcessorState {
                            threshold_kg: 0.0,
                            ticks_since_last_run: 0,
                            stalled: false,
                            // Select a recipe that does NOT exist in the processor's recipe list
                            selected_recipe: Some(RecipeId("nonexistent_recipe".to_string())),
                        }),
                        wear: crate::WearState::default(),
                        thermal: None,
                        power_stalled: false,
                        manufacturing_priority: 0,
                    }],
                    modifiers: crate::modifiers::ModifierSet::default(),
                    power: PowerState::default(),
                    cached_inventory_volume_m3: None,
                    module_type_index: crate::ModuleTypeIndex::default(),
                },
            )]
            .into_iter()
            .collect(),
            research: crate::ResearchState {
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
            body_cache: AHashMap::default(),
        };

        let mut events = Vec::new();
        tick_station_modules(
            &mut state,
            &station_id,
            &content,
            &mut events,
            &mut Vec::new(),
        );

        // Should emit RecipeSelectionReset
        let reset_event = events.iter().find(|e| {
            matches!(
                &e.event,
                Event::RecipeSelectionReset {
                    old_recipe,
                    new_recipe,
                    ..
                } if old_recipe.0 == "nonexistent_recipe" && *new_recipe == first_recipe
            )
        });
        assert!(reset_event.is_some(), "expected RecipeSelectionReset event");

        // Processor state should have the first recipe
        let station = state.stations.get(&station_id).unwrap();
        if let ModuleKindState::Processor(ps) = &station.modules[0].kind_state {
            assert_eq!(
                ps.selected_recipe,
                Some(first_recipe),
                "selected_recipe should be reset to first recipe"
            );
        }

        // Should still run with the fallback recipe
        let ran = events
            .iter()
            .any(|e| matches!(&e.event, Event::RefineryRan { .. }));
        assert!(ran, "processor should run with fallback recipe");
    }

    // ── Tech gate tests ──────────────────────────────────────────

    #[test]
    fn processor_skips_when_tech_not_unlocked() {
        let mut content = crate::test_fixtures::base_content();
        let tech_id = crate::TechId("tech_advanced_smelting".to_string());
        content.techs.push(crate::TechDef {
            id: tech_id.clone(),
            name: "Advanced Smelting".to_string(),
            prereqs: vec![],
            domain_requirements: HashMap::new(),
            accepted_data: vec![],
            effects: vec![],
        });
        let recipe = crate::RecipeDef {
            id: RecipeId("recipe_tech_gated".to_string()),
            inputs: vec![crate::RecipeInput {
                filter: crate::InputFilter::ItemKind(crate::ItemKind::Ore),
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
            thermal_req: None,
            required_tech: Some(tech_id.clone()),
            tags: vec![],
        };
        let recipe_id = crate::test_fixtures::insert_recipe(&mut content, recipe);
        content.module_defs.insert(
            "module_adv_refinery".to_string(),
            crate::ModuleDef {
                id: "module_adv_refinery".to_string(),
                name: "Adv Refinery".to_string(),
                mass_kg: 1000.0,
                volume_m3: 5.0,
                power_consumption_per_run: 10.0,
                wear_per_run: 0.0,
                behavior: crate::ModuleBehaviorDef::Processor(crate::ProcessorDef {
                    processing_interval_minutes: 1,
                    processing_interval_ticks: 1,
                    recipes: vec![recipe_id],
                }),
                thermal: None,
                compatible_slots: Vec::new(),
                ship_modifiers: Vec::new(),
                power_stall_priority: None,
                roles: vec![],
            },
        );

        let station_id = StationId("station_test".to_string());
        let mut state = GameState {
            meta: crate::MetaState {
                tick: 10,
                seed: 42,
                schema_version: 1,
                content_version: content.content_version.clone(),
            },
            scan_sites: vec![],
            asteroids: AHashMap::default(),
            ships: AHashMap::default(),
            stations: [(
                station_id.clone(),
                crate::StationState {
                    id: station_id.clone(),
                    position: crate::test_fixtures::test_position(),
                    inventory: vec![crate::InventoryItem::Ore {
                        lot_id: crate::LotId("lot_0001".to_string()),
                        asteroid_id: crate::AsteroidId("ast_0001".to_string()),
                        kg: 500.0,
                        composition: HashMap::from([
                            ("Fe".to_string(), 0.7),
                            ("Si".to_string(), 0.3),
                        ]),
                    }],
                    cargo_capacity_m3: 10_000.0,
                    power_available_per_tick: 100.0,
                    modules: vec![ModuleState {
                        id: ModuleInstanceId("proc_0001".to_string()),
                        def_id: "module_adv_refinery".to_string(),
                        enabled: true,
                        kind_state: ModuleKindState::Processor(ProcessorState {
                            threshold_kg: 0.0,
                            ticks_since_last_run: 0,
                            stalled: false,
                            selected_recipe: None,
                        }),
                        wear: crate::WearState::default(),
                        thermal: None,
                        power_stalled: false,
                        manufacturing_priority: 0,
                    }],
                    modifiers: crate::modifiers::ModifierSet::default(),
                    power: PowerState::default(),
                    cached_inventory_volume_m3: None,
                    module_type_index: crate::ModuleTypeIndex::default(),
                },
            )]
            .into_iter()
            .collect(),
            research: crate::ResearchState {
                unlocked: HashSet::new(), // tech NOT unlocked
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

        let mut events = Vec::new();
        tick_station_modules(
            &mut state,
            &station_id,
            &content,
            &mut events,
            &mut Vec::new(),
        );

        // Should NOT run
        let ran = events
            .iter()
            .any(|e| matches!(&e.event, Event::RefineryRan { .. }));
        assert!(!ran, "processor should not run without required tech");

        // Should emit ModuleAwaitingTech
        let awaiting = events.iter().any(|e| {
            matches!(&e.event, Event::ModuleAwaitingTech { tech_id: tid, .. } if *tid == tech_id)
        });
        assert!(awaiting, "should emit ModuleAwaitingTech event");

        // Now unlock the tech and run again
        state.research.unlocked.insert(tech_id);
        let mut events2 = Vec::new();
        tick_station_modules(
            &mut state,
            &station_id,
            &content,
            &mut events2,
            &mut Vec::new(),
        );

        let ran2 = events2
            .iter()
            .any(|e| matches!(&e.event, Event::RefineryRan { .. }));
        assert!(ran2, "processor should run after tech is unlocked");
    }

    // ── Component output tests ───────────────────────────────────

    #[test]
    fn processor_produces_component_output() {
        let mut content = crate::test_fixtures::base_content();
        content.component_defs.push(crate::ComponentDef {
            id: "ingot".to_string(),
            name: "Iron Ingot".to_string(),
            mass_kg: 10.0,
            volume_m3: 0.5,
        });
        let recipe = crate::RecipeDef {
            id: RecipeId("recipe_ore_to_ingot".to_string()),
            inputs: vec![crate::RecipeInput {
                filter: crate::InputFilter::ItemKind(crate::ItemKind::Ore),
                amount: InputAmount::Kg(100.0),
            }],
            outputs: vec![OutputSpec::Component {
                component_id: crate::ComponentId("ingot".to_string()),
                quality_formula: QualityFormula::Fixed(0.8),
            }],
            efficiency: 1.0,
            thermal_req: None,
            required_tech: None,
            tags: vec![],
        };
        let recipe_id = crate::test_fixtures::insert_recipe(&mut content, recipe);
        content.module_defs.insert(
            "module_ingot_maker".to_string(),
            crate::ModuleDef {
                id: "module_ingot_maker".to_string(),
                name: "Ingot Maker".to_string(),
                mass_kg: 1000.0,
                volume_m3: 5.0,
                power_consumption_per_run: 10.0,
                wear_per_run: 0.0,
                behavior: crate::ModuleBehaviorDef::Processor(crate::ProcessorDef {
                    processing_interval_minutes: 1,
                    processing_interval_ticks: 1,
                    recipes: vec![recipe_id],
                }),
                thermal: None,
                compatible_slots: Vec::new(),
                ship_modifiers: Vec::new(),
                power_stall_priority: None,
                roles: vec![],
            },
        );

        let station_id = StationId("station_test".to_string());
        let mut state = GameState {
            meta: crate::MetaState {
                tick: 10,
                seed: 42,
                schema_version: 1,
                content_version: content.content_version.clone(),
            },
            scan_sites: vec![],
            asteroids: AHashMap::default(),
            ships: AHashMap::default(),
            stations: [(
                station_id.clone(),
                crate::StationState {
                    id: station_id.clone(),
                    position: crate::test_fixtures::test_position(),
                    inventory: vec![crate::InventoryItem::Ore {
                        lot_id: crate::LotId("lot_0001".to_string()),
                        asteroid_id: crate::AsteroidId("ast_0001".to_string()),
                        kg: 300.0,
                        composition: HashMap::from([
                            ("Fe".to_string(), 0.7),
                            ("Si".to_string(), 0.3),
                        ]),
                    }],
                    cargo_capacity_m3: 10_000.0,
                    power_available_per_tick: 100.0,
                    modules: vec![ModuleState {
                        id: ModuleInstanceId("ingot_maker_0001".to_string()),
                        def_id: "module_ingot_maker".to_string(),
                        enabled: true,
                        kind_state: ModuleKindState::Processor(ProcessorState {
                            threshold_kg: 0.0,
                            ticks_since_last_run: 0,
                            stalled: false,
                            selected_recipe: None,
                        }),
                        wear: crate::WearState::default(),
                        thermal: None,
                        power_stalled: false,
                        manufacturing_priority: 0,
                    }],
                    modifiers: crate::modifiers::ModifierSet::default(),
                    power: PowerState::default(),
                    cached_inventory_volume_m3: None,
                    module_type_index: crate::ModuleTypeIndex::default(),
                },
            )]
            .into_iter()
            .collect(),
            research: crate::ResearchState {
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
            body_cache: AHashMap::default(),
        };

        let mut events = Vec::new();
        tick_station_modules(
            &mut state,
            &station_id,
            &content,
            &mut events,
            &mut Vec::new(),
        );

        // Should have run
        let ran = events
            .iter()
            .any(|e| matches!(&e.event, Event::RefineryRan { .. }));
        assert!(ran, "processor should run");

        // Should have produced a component
        let station = state.stations.get(&station_id).unwrap();
        let ingot_count: u32 = station
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Component {
                    component_id,
                    count,
                    ..
                } if component_id.0 == "ingot" => Some(*count),
                _ => None,
            })
            .sum();
        assert_eq!(ingot_count, 1, "expected 1 ingot produced");

        // Quality should be Fixed(0.8) * 1.0 thermal_quality = 0.8
        let quality = station.inventory.iter().find_map(|i| match i {
            InventoryItem::Component {
                component_id,
                quality,
                ..
            } if component_id.0 == "ingot" => Some(*quality),
            _ => None,
        });
        assert!(
            (quality.unwrap() - 0.8).abs() < 0.01,
            "expected quality ~0.8"
        );

        // Run again — components should merge by quality
        let mut events2 = Vec::new();
        tick_station_modules(
            &mut state,
            &station_id,
            &content,
            &mut events2,
            &mut Vec::new(),
        );

        let station = state.stations.get(&station_id).unwrap();
        let ingot_count2: u32 = station
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Component {
                    component_id,
                    count,
                    ..
                } if component_id.0 == "ingot" => Some(*count),
                _ => None,
            })
            .sum();
        assert_eq!(
            ingot_count2, 2,
            "expected 2 ingots after second run (merged)"
        );
    }
}
