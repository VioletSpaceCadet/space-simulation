use crate::{
    composition::{blend_slag_composition, merge_material_lot, weighted_composition},
    tasks::element_density,
    Event, EventEnvelope, GameContent, GameState, InputAmount, InputFilter, InventoryItem,
    ItemKind, ModuleBehaviorDef, ModuleKindState, OutputSpec, QualityFormula, RecipeDef, StationId,
    YieldFormula,
};
use std::collections::HashMap;

/// Minimum meaningful mass — amounts below this are discarded as rounding noise.
const MIN_MEANINGFUL_KG: f32 = 1e-3;

/// Estimate the total output volume (m³) a recipe would produce given the
/// consumed lots and their weighted-average composition.
fn estimate_output_volume_m3(
    recipe: &RecipeDef,
    avg_composition: &HashMap<String, f32>,
    consumed_kg: f32,
    content: &GameContent,
) -> f32 {
    let mut material_kg = 0.0_f32;
    let mut total_volume = 0.0_f32;

    for output in &recipe.outputs {
        match output {
            OutputSpec::Material {
                element,
                yield_formula,
                ..
            } => {
                let yield_frac = match yield_formula {
                    YieldFormula::ElementFraction { element: el } => {
                        avg_composition.get(el).copied().unwrap_or(0.0)
                    }
                    YieldFormula::FixedFraction(f) => *f,
                };
                material_kg = consumed_kg * yield_frac;
                let density = element_density(content, element);
                total_volume += material_kg / density;
            }
            OutputSpec::Slag { yield_formula } => {
                let yield_frac = match yield_formula {
                    YieldFormula::FixedFraction(f) => *f,
                    YieldFormula::ElementFraction { element } => {
                        avg_composition.get(element).copied().unwrap_or(0.0)
                    }
                };
                let slag_kg = (consumed_kg - material_kg) * yield_frac;
                let slag_density = element_density(content, crate::ELEMENT_SLAG);
                total_volume += slag_kg / slag_density;
            }
            OutputSpec::Component { .. } => {}
        }
    }
    total_volume
}

pub(crate) fn tick_stations(
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let station_ids: Vec<StationId> = state.stations.keys().cloned().collect();
    for station_id in &station_ids {
        tick_station_modules(state, station_id, content, events);
    }
    for station_id in &station_ids {
        tick_maintenance_modules(state, station_id, content, events);
    }
}

fn tick_station_modules(
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
        let (def_id, interval, power_needed) = {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            let module = &station.modules[module_idx];
            if !module.enabled {
                continue;
            }
            let Some(def) = content.module_defs.iter().find(|d| d.id == module.def_id) else {
                continue;
            };
            let interval = match &def.behavior {
                ModuleBehaviorDef::Processor(p) => p.processing_interval_ticks,
                ModuleBehaviorDef::Storage { .. } | ModuleBehaviorDef::Maintenance(_) => continue,
            };
            (
                module.def_id.clone(),
                interval,
                def.power_consumption_per_run,
            )
        };

        // Tick timer; skip if interval not reached yet.
        {
            let Some(station) = state.stations.get_mut(station_id) else {
                return;
            };
            if let ModuleKindState::Processor(ps) = &mut station.modules[module_idx].kind_state {
                ps.ticks_since_last_run += 1;
                if ps.ticks_since_last_run < interval {
                    continue;
                }
            }
        }

        // Check power budget.
        {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            if station.power_available_per_tick < power_needed {
                continue;
            }
        }

        // Check ore threshold.
        let threshold_kg = {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            match &station.modules[module_idx].kind_state {
                ModuleKindState::Processor(ps) => ps.threshold_kg,
                ModuleKindState::Storage | ModuleKindState::Maintenance(_) => continue,
            }
        };

        let total_ore_kg: f32 = state.stations.get(station_id).map_or(0.0, |s| {
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
            continue;
        }

        // --- Capacity pre-check: estimate output volume and stall if it won't fit ---
        {
            let Some(def) = content.module_defs.iter().find(|d| d.id == def_id) else {
                continue;
            };
            let ModuleBehaviorDef::Processor(processor_def) = &def.behavior else {
                continue;
            };
            let Some(recipe) = processor_def.recipes.first() else {
                continue;
            };

            let rate_kg = match recipe.inputs.first().map(|i| &i.amount) {
                Some(InputAmount::Kg(kg)) => *kg,
                _ => continue,
            };

            let input_filter = recipe.inputs.first().map(|i| &i.filter).cloned();
            let matches_input = move |item: &InventoryItem| -> bool {
                match &input_filter {
                    Some(InputFilter::ItemKind(ItemKind::Ore)) => {
                        matches!(item, InventoryItem::Ore { .. })
                    }
                    Some(InputFilter::ItemKind(ItemKind::Material)) => {
                        matches!(item, InventoryItem::Material { .. })
                    }
                    Some(InputFilter::ItemKind(ItemKind::Slag)) => {
                        matches!(item, InventoryItem::Slag { .. })
                    }
                    Some(InputFilter::Element(el)) => {
                        matches!(item, InventoryItem::Material { element, .. } if element == el)
                    }
                    _ => false,
                }
            };

            let station = state.stations.get(station_id).unwrap();
            let (peeked_kg, lots) =
                peek_ore_fifo_with_lots(&station.inventory, rate_kg, matches_input);

            if peeked_kg < MIN_MEANINGFUL_KG {
                continue;
            }

            let lot_refs: Vec<(&HashMap<String, f32>, f32)> =
                lots.iter().map(|(comp, kg)| (comp, *kg)).collect();
            let avg_composition = weighted_composition(&lot_refs);
            let output_volume =
                estimate_output_volume_m3(recipe, &avg_composition, peeked_kg, content);

            let current_used = crate::tasks::inventory_volume_m3(&station.inventory, content);
            let capacity = station.cargo_capacity_m3;
            let shortfall = (current_used + output_volume) - capacity;

            let was_stalled = match &station.modules[module_idx].kind_state {
                ModuleKindState::Processor(ps) => ps.stalled,
                ModuleKindState::Storage | ModuleKindState::Maintenance(_) => false,
            };
            let module_id = station.modules[module_idx].id.clone();

            if shortfall > 0.0 {
                // Stall: reset timer, emit event on transition only
                let station_mut = state.stations.get_mut(station_id).unwrap();
                if let ModuleKindState::Processor(ps) =
                    &mut station_mut.modules[module_idx].kind_state
                {
                    ps.stalled = true;
                    ps.ticks_since_last_run = 0;
                }
                if !was_stalled {
                    events.push(crate::emit(
                        &mut state.counters,
                        state.meta.tick,
                        Event::ModuleStalled {
                            station_id: station_id.clone(),
                            module_id,
                            shortfall_m3: shortfall,
                        },
                    ));
                }
                continue;
            }

            // Space available — resume if previously stalled
            if was_stalled {
                let station_mut = state.stations.get_mut(station_id).unwrap();
                if let ModuleKindState::Processor(ps) =
                    &mut station_mut.modules[module_idx].kind_state
                {
                    ps.stalled = false;
                }
                events.push(crate::emit(
                    &mut state.counters,
                    state.meta.tick,
                    Event::ModuleResumed {
                        station_id: station_id.clone(),
                        module_id,
                    },
                ));
            }
        }

        resolve_processor_run(state, station_id, module_idx, &def_id, content, events);

        if let Some(station) = state.stations.get_mut(station_id) {
            if let ModuleKindState::Processor(ps) = &mut station.modules[module_idx].kind_state {
                ps.ticks_since_last_run = 0;
            }
        }
    }
}

fn resolve_processor_run(
    state: &mut GameState,
    station_id: &StationId,
    module_idx: usize,
    def_id: &str,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    let Some(module_id) = state
        .stations
        .get(station_id)
        .map(|s| s.modules[module_idx].id.clone())
    else {
        return;
    };

    let Some(def) = content.module_defs.iter().find(|d| d.id == def_id) else {
        return;
    };
    let ModuleBehaviorDef::Processor(processor_def) = &def.behavior else {
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
    let matches_input = move |item: &InventoryItem| -> bool {
        match &input_filter {
            Some(InputFilter::ItemKind(ItemKind::Ore)) => matches!(item, InventoryItem::Ore { .. }),
            Some(InputFilter::ItemKind(ItemKind::Material)) => {
                matches!(item, InventoryItem::Material { .. })
            }
            Some(InputFilter::ItemKind(ItemKind::Slag)) => {
                matches!(item, InventoryItem::Slag { .. })
            }
            Some(InputFilter::Element(el)) => {
                matches!(item, InventoryItem::Material { element, .. } if element == el)
            }
            _ => false,
        }
    };

    // FIFO-consume ore and compute weighted average composition.
    let (consumed_kg, lots) = {
        let Some(station) = state.stations.get_mut(station_id) else {
            return;
        };
        consume_ore_fifo_with_lots(&mut station.inventory, rate_kg, matches_input)
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

    // Compute wear efficiency — reduces output, not input (wastes ore)
    let wear_value = state
        .stations
        .get(station_id)
        .map_or(0.0, |s| s.modules[module_idx].wear.wear);
    let efficiency = crate::wear::wear_efficiency(wear_value, &content.constants);

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
                    if let Some(station) = state.stations.get_mut(station_id) {
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
                    if let Some(station) = state.stations.get_mut(station_id) {
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
            OutputSpec::Component { .. } => {} // not yet implemented
        }
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::RefineryRan {
            station_id: station_id.clone(),
            module_id: module_id.clone(),
            ore_consumed_kg: consumed_kg,
            material_produced_kg: material_kg,
            material_quality,
            slag_produced_kg: slag_kg,
            material_element: extracted_element.unwrap_or_default(),
        },
    ));

    // Accumulate wear
    let wear_per_run = content
        .module_defs
        .iter()
        .find(|d| d.id == def_id)
        .map_or(0.0, |d| d.wear_per_run);

    if wear_per_run > 0.0 {
        if let Some(station) = state.stations.get_mut(station_id) {
            let module = &mut station.modules[module_idx];
            let wear_before = module.wear.wear;
            module.wear.wear = (module.wear.wear + wear_per_run).min(1.0);
            let wear_after = module.wear.wear;

            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                Event::WearAccumulated {
                    station_id: station_id.clone(),
                    module_id: module.id.clone(),
                    wear_before,
                    wear_after,
                },
            ));

            if module.wear.wear >= 1.0 {
                let mid = module.id.clone();
                module.enabled = false;
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    Event::ModuleAutoDisabled {
                        station_id: station_id.clone(),
                        module_id: mid,
                    },
                ));
            }
        }
    }
}

/// FIFO-consume up to `rate_kg` from matching Ore items.
/// Returns `(consumed_kg, Vec<(composition, kg_taken)>)` for weighted averaging.
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

fn tick_maintenance_modules(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;
    let module_count = state
        .stations
        .get(station_id)
        .map_or(0, |s| s.modules.len());

    for module_idx in 0..module_count {
        let (interval, power_needed, repair_reduction, kit_cost) = {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            let module = &station.modules[module_idx];
            if !module.enabled {
                continue;
            }
            let Some(def) = content.module_defs.iter().find(|d| d.id == module.def_id) else {
                continue;
            };
            let ModuleBehaviorDef::Maintenance(maint_def) = &def.behavior else {
                continue;
            };
            (
                maint_def.repair_interval_ticks,
                def.power_consumption_per_run,
                maint_def.wear_reduction_per_run,
                maint_def.repair_kit_cost,
            )
        };

        // Tick timer
        {
            let Some(station) = state.stations.get_mut(station_id) else {
                return;
            };
            if let ModuleKindState::Maintenance(ms) = &mut station.modules[module_idx].kind_state {
                ms.ticks_since_last_run += 1;
                if ms.ticks_since_last_run < interval {
                    continue;
                }
            } else {
                continue;
            }
        }

        // Check power
        {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            if station.power_available_per_tick < power_needed {
                continue;
            }
        }

        // Find most worn module (not self, wear > 0.0), sorted by wear desc then ID asc for determinism
        let target = {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            let self_id = &station.modules[module_idx].id;
            let mut candidates: Vec<(usize, f32, String)> = station
                .modules
                .iter()
                .enumerate()
                .filter(|(_, m)| m.id != *self_id && m.wear.wear > 0.0)
                .map(|(idx, m)| (idx, m.wear.wear, m.id.0.clone()))
                .collect();
            candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
            candidates.first().map(|(idx, _, _)| *idx)
        };

        let Some(target_idx) = target else {
            // Nothing worn — reset timer but don't consume kit
            if let Some(station) = state.stations.get_mut(station_id) {
                if let ModuleKindState::Maintenance(ms) =
                    &mut station.modules[module_idx].kind_state
                {
                    ms.ticks_since_last_run = 0;
                }
            }
            continue;
        };

        // Consume repair kit
        let has_kit = {
            let Some(station) = state.stations.get_mut(station_id) else {
                return;
            };
            let kit_slot = station.inventory.iter_mut().find(|i| {
                matches!(i, InventoryItem::Component { component_id, count, .. }
                    if component_id.0 == "repair_kit" && *count >= kit_cost)
            });
            if let Some(InventoryItem::Component { count, .. }) = kit_slot {
                *count -= kit_cost;
                true
            } else {
                false
            }
        };

        if !has_kit {
            // Reset timer even if no kit
            if let Some(station) = state.stations.get_mut(station_id) {
                if let ModuleKindState::Maintenance(ms) =
                    &mut station.modules[module_idx].kind_state
                {
                    ms.ticks_since_last_run = 0;
                }
            }
            continue;
        }

        // Remove empty component stacks
        if let Some(station) = state.stations.get_mut(station_id) {
            station
                .inventory
                .retain(|i| !matches!(i, InventoryItem::Component { count, .. } if *count == 0));
        }

        // Apply repair
        let (target_module_id, wear_before, wear_after, kits_remaining) = {
            let Some(station) = state.stations.get_mut(station_id) else {
                return;
            };
            let target_module = &mut station.modules[target_idx];
            let wear_before = target_module.wear.wear;
            target_module.wear.wear = (target_module.wear.wear - repair_reduction).max(0.0);
            let wear_after = target_module.wear.wear;
            let target_module_id = target_module.id.clone();

            // Re-enable module if it was auto-disabled due to wear
            if !target_module.enabled && wear_after < 1.0 {
                target_module.enabled = true;
            }

            let kits_remaining: u32 = station
                .inventory
                .iter()
                .filter_map(|i| {
                    if let InventoryItem::Component {
                        component_id,
                        count,
                        ..
                    } = i
                    {
                        if component_id.0 == "repair_kit" {
                            Some(*count)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .sum();

            // Reset timer
            if let ModuleKindState::Maintenance(ms) = &mut station.modules[module_idx].kind_state {
                ms.ticks_since_last_run = 0;
            }

            (target_module_id, wear_before, wear_after, kits_remaining)
        };

        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            Event::MaintenanceRan {
                station_id: station_id.clone(),
                target_module_id,
                wear_before,
                wear_after,
                repair_kits_remaining: kits_remaining,
            },
        ));
    }
}

/// Peek at what would be consumed by FIFO without mutating inventory.
/// Returns `(consumed_kg, Vec<(composition, kg_taken)>)`.
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
