use crate::{
    Event, EventEnvelope, GameContent, GameState, InputAmount, InputFilter, InventoryItem,
    ItemKind, ModuleBehaviorDef, ModuleKindState, OutputSpec, QualityFormula, StationId,
    YieldFormula,
};
use std::collections::HashMap;

pub(crate) fn tick_stations(
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let station_ids: Vec<StationId> = state.stations.keys().cloned().collect();

    for station_id in station_ids {
        tick_station_modules(state, &station_id, content, events);
    }
}

#[allow(clippy::too_many_lines)]
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
        // Gather what we need from an immutable borrow.
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
                ModuleBehaviorDef::Storage { .. } => continue,
            };
            (
                module.def_id.clone(),
                interval,
                def.power_consumption_per_run,
            )
        };

        // Increment timer; skip if interval not reached.
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

        // Check power.
        {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            if station.power_available_per_tick < power_needed {
                continue;
            }
        }

        // Check threshold against total ore in inventory.
        let threshold_kg = {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            match &station.modules[module_idx].kind_state {
                ModuleKindState::Processor(ps) => ps.threshold_kg,
                ModuleKindState::Storage => continue,
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

        // Run the recipe and reset timer.
        resolve_processor_run(state, station_id, module_idx, &def_id, content, events);

        if let Some(station) = state.stations.get_mut(station_id) {
            if let ModuleKindState::Processor(ps) = &mut station.modules[module_idx].kind_state {
                ps.ticks_since_last_run = 0;
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
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

    // Determine consume rate and input filter.
    let rate_kg = match recipe.inputs.first().map(|i| &i.amount) {
        Some(InputAmount::Kg(kg)) => *kg,
        _ => return,
    };

    let matches_input = |item: &InventoryItem| -> bool {
        match recipe.inputs.first().map(|inp| &inp.filter) {
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

    // FIFO consumption up to rate_kg from matching items.
    let mut remaining = rate_kg;
    let mut consumed_kg = 0.0f32;
    let mut weighted_composition: HashMap<String, f32> = HashMap::new();
    let mut weighted_total_kg = 0.0f32;

    if let Some(station) = state.stations.get_mut(station_id) {
        let mut new_inventory: Vec<InventoryItem> = Vec::new();

        for item in station.inventory.drain(..) {
            if remaining > 0.0 {
                if let InventoryItem::Ore {
                    ref lot_id,
                    ref asteroid_id,
                    kg,
                    ref composition,
                } = item
                {
                    if matches_input(&item) {
                        let take = kg.min(remaining);
                        remaining -= take;
                        consumed_kg += take;
                        for (element, fraction) in composition {
                            *weighted_composition.entry(element.clone()).or_insert(0.0) +=
                                fraction * take;
                        }
                        weighted_total_kg += take;
                        let leftover = kg - take;
                        if leftover > 1e-3 {
                            new_inventory.push(InventoryItem::Ore {
                                lot_id: lot_id.clone(),
                                asteroid_id: asteroid_id.clone(),
                                kg: leftover,
                                composition: composition.clone(),
                            });
                        }
                        continue;
                    }
                }
            }
            new_inventory.push(item);
        }
        station.inventory = new_inventory;
    }

    if consumed_kg < 1e-3 {
        return;
    }

    // Average composition across consumed lots (weighted by kg).
    let avg_composition: HashMap<String, f32> = if weighted_total_kg > 0.0 {
        weighted_composition
            .iter()
            .map(|(k, v)| (k.clone(), v / weighted_total_kg))
            .collect()
    } else {
        HashMap::new()
    };

    // Find which element is being extracted (for slag composition calc).
    let extracted_element: Option<String> = recipe.outputs.iter().find_map(|o| {
        if let OutputSpec::Material { element, .. } = o {
            Some(element.clone())
        } else {
            None
        }
    });

    let mut material_kg = 0.0f32;
    let mut material_quality = 0.0f32;
    let mut slag_kg = 0.0f32;

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
                material_kg = consumed_kg * yield_frac;
                material_quality = match quality_formula {
                    QualityFormula::ElementFractionTimesMultiplier {
                        element: el,
                        multiplier,
                    } => (avg_composition.get(el).copied().unwrap_or(0.0) * multiplier)
                        .clamp(0.0, 1.0),
                    QualityFormula::Fixed(q) => *q,
                };
                if material_kg > 1e-3 {
                    if let Some(station) = state.stations.get_mut(station_id) {
                        station.inventory.push(InventoryItem::Material {
                            element: element.clone(),
                            kg: material_kg,
                            quality: material_quality,
                        });
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
                // Slag = remainder after material extraction, scaled by yield_frac.
                slag_kg = (consumed_kg - material_kg) * yield_frac;

                // Slag composition: non-extracted element fractions, re-normalized.
                let mut slag_composition: HashMap<String, f32> = HashMap::new();
                let non_extracted_total: f32 = avg_composition
                    .iter()
                    .filter(|(k, _)| Some(k.as_str()) != extracted_element.as_deref())
                    .map(|(_, v)| v)
                    .sum();

                if non_extracted_total > 1e-6 {
                    for (el, frac) in &avg_composition {
                        if Some(el.as_str()) != extracted_element.as_deref() {
                            slag_composition.insert(el.clone(), frac / non_extracted_total);
                        }
                    }
                }

                if slag_kg > 1e-3 {
                    if let Some(station) = state.stations.get_mut(station_id) {
                        // Blend with existing Slag item, or push a new one.
                        let existing = station
                            .inventory
                            .iter_mut()
                            .find(|i| matches!(i, InventoryItem::Slag { .. }));
                        if let Some(InventoryItem::Slag {
                            kg: existing_kg,
                            composition: existing_comp,
                        }) = existing
                        {
                            let total = *existing_kg + slag_kg;
                            let blended: HashMap<String, f32> = {
                                let mut keys: std::collections::HashSet<String> =
                                    existing_comp.keys().cloned().collect();
                                keys.extend(slag_composition.keys().cloned());
                                keys.into_iter()
                                    .map(|k| {
                                        let a = existing_comp.get(&k).copied().unwrap_or(0.0)
                                            * *existing_kg;
                                        let b = slag_composition.get(&k).copied().unwrap_or(0.0)
                                            * slag_kg;
                                        (k, (a + b) / total)
                                    })
                                    .collect()
                            };
                            *existing_kg = total;
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
            module_id,
            ore_consumed_kg: consumed_kg,
            material_produced_kg: material_kg,
            material_quality,
            slag_produced_kg: slag_kg,
            material_element: extracted_element.unwrap_or_default(),
        },
    ));
}
