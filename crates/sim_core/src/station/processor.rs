use crate::{
    composition::{blend_slag_composition, merge_material_lot, weighted_composition},
    Event, EventEnvelope, GameContent, GameState, InputAmount, InventoryItem, ModuleBehaviorDef,
    ModuleKindState, OutputSpec, QualityFormula, StationId, YieldFormula,
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
    let output_volume = estimate_output_volume_m3(recipe, &avg_composition, peeked_kg, content);

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
    resolve_processor_run(ctx, state, events);

    super::RunOutcome::Completed
}

#[allow(clippy::too_many_lines)]
fn resolve_processor_run(
    ctx: &super::ModuleTickContext,
    state: &mut GameState,
    events: &mut Vec<EventEnvelope>,
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
                material_kg = consumed_kg * yield_frac * efficiency;
                material_quality = match quality_formula {
                    QualityFormula::ElementFractionTimesMultiplier {
                        element: el,
                        multiplier,
                    } => (avg_composition.get(el).copied().unwrap_or(0.0) * multiplier)
                        .clamp(0.0, 1.0),
                    QualityFormula::Fixed(q) => *q,
                };
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
}
