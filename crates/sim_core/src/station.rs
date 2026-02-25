use crate::{
    composition::{blend_slag_composition, merge_material_lot, weighted_composition},
    research::generate_data,
    tasks::element_density,
    Event, EventEnvelope, GameContent, GameState, InputAmount, InputFilter, InventoryItem,
    ItemKind, ModuleBehaviorDef, ModuleKindState, OutputSpec, PrincipalId, QualityFormula,
    RecipeDef, ShipId, ShipState, StationId, TechId, YieldFormula,
};
use std::collections::HashMap;

/// Minimum meaningful mass — amounts below this are discarded as rounding noise.
const MIN_MEANINGFUL_KG: f32 = 1e-3;

/// Tech ID required for ship construction recipes.
const TECH_SHIP_CONSTRUCTION: &str = "tech_ship_construction";

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
            OutputSpec::Component { .. } | OutputSpec::Ship { .. } => {}
        }
    }
    total_volume
}

fn matches_input_filter(item: &InventoryItem, filter: Option<&InputFilter>) -> bool {
    match filter {
        Some(InputFilter::ItemKind(ItemKind::Ore)) => matches!(item, InventoryItem::Ore { .. }),
        Some(InputFilter::ItemKind(ItemKind::Material)) => {
            matches!(item, InventoryItem::Material { .. })
        }
        Some(InputFilter::ItemKind(ItemKind::Slag)) => matches!(item, InventoryItem::Slag { .. }),
        Some(InputFilter::Element(el)) => {
            matches!(item, InventoryItem::Material { element, .. } if element == el)
        }
        _ => false,
    }
}

pub(crate) fn tick_stations(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl rand::Rng,
    events: &mut Vec<EventEnvelope>,
) {
    let station_ids: Vec<StationId> = state.stations.keys().cloned().collect();
    for station_id in &station_ids {
        tick_station_modules(state, station_id, content, events);
    }
    for station_id in &station_ids {
        tick_assembler_modules(state, station_id, content, rng, events);
    }
    for station_id in &station_ids {
        tick_sensor_array_modules(state, station_id, content, events);
    }
    for station_id in &station_ids {
        tick_lab_modules(state, station_id, content, events);
    }
    for station_id in &station_ids {
        tick_maintenance_modules(state, station_id, content, events);
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
                ModuleBehaviorDef::Storage { .. }
                | ModuleBehaviorDef::Maintenance(_)
                | ModuleBehaviorDef::Assembler(_)
                | ModuleBehaviorDef::Lab(_)
                | ModuleBehaviorDef::SensorArray(_) => continue,
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
                ModuleKindState::Storage
                | ModuleKindState::Maintenance(_)
                | ModuleKindState::Assembler(_)
                | ModuleKindState::Lab(_)
                | ModuleKindState::SensorArray(_) => continue,
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

            let station = state.stations.get(station_id).unwrap();
            let (peeked_kg, lots) = peek_ore_fifo_with_lots(&station.inventory, rate_kg, |item| {
                matches_input_filter(item, input_filter.as_ref())
            });

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
                ModuleKindState::Storage
                | ModuleKindState::Maintenance(_)
                | ModuleKindState::Assembler(_)
                | ModuleKindState::Lab(_)
                | ModuleKindState::SensorArray(_) => false,
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

    let rate_kg = match recipe.inputs.first().map(|i| &i.amount) {
        Some(InputAmount::Kg(kg)) => *kg,
        _ => return,
    };

    let input_filter = recipe.inputs.first().map(|i| &i.filter).cloned();

    // FIFO-consume ore and compute weighted average composition.
    let (consumed_kg, lots) = {
        let Some(station) = state.stations.get_mut(station_id) else {
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
            OutputSpec::Component { .. } | OutputSpec::Ship { .. } => {} // not yet implemented
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
    apply_wear(state, station_id, module_idx, wear_per_run, events);
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

#[allow(clippy::too_many_lines)]
fn tick_assembler_modules(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    rng: &mut impl rand::Rng,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;
    let module_count = state
        .stations
        .get(station_id)
        .map_or(0, |s| s.modules.len());

    for module_idx in 0..module_count {
        // Extract module info and assembler def
        let (assembler_def, power_needed, wear_per_run) = {
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
            let ModuleBehaviorDef::Assembler(assembler_def) = &def.behavior else {
                continue;
            };
            (
                assembler_def.clone(),
                def.power_consumption_per_run,
                def.wear_per_run,
            )
        };

        // Tick timer; skip if interval not reached
        {
            let Some(station) = state.stations.get_mut(station_id) else {
                return;
            };
            if let ModuleKindState::Assembler(asmb) = &mut station.modules[module_idx].kind_state {
                asmb.ticks_since_last_run += 1;
                if asmb.ticks_since_last_run < assembler_def.assembly_interval_ticks {
                    continue;
                }
            } else {
                continue;
            }
        }

        // Check power budget
        {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            if station.power_available_per_tick < power_needed {
                continue;
            }
        }

        let Some(recipe) = assembler_def.recipes.first() else {
            continue;
        };

        // Check input availability
        let input_available = {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            recipe
                .inputs
                .iter()
                .all(|input| match (&input.filter, &input.amount) {
                    (InputFilter::Element(el), InputAmount::Kg(required_kg)) => {
                        let available_kg: f32 = station
                            .inventory
                            .iter()
                            .filter_map(|item| match item {
                                InventoryItem::Material { element, kg, .. } if element == el => {
                                    Some(*kg)
                                }
                                _ => None,
                            })
                            .sum();
                        available_kg >= *required_kg
                    }
                    (InputFilter::Component(cid), InputAmount::Count(required)) => {
                        let available: u32 = station
                            .inventory
                            .iter()
                            .filter_map(|item| match item {
                                InventoryItem::Component {
                                    component_id,
                                    count,
                                    ..
                                } if component_id.0 == cid.0 => Some(*count),
                                _ => None,
                            })
                            .sum();
                        available >= *required
                    }
                    _ => false,
                })
        };

        if !input_available {
            // Reset timer so it retries next interval
            if let Some(station) = state.stations.get_mut(station_id) {
                if let ModuleKindState::Assembler(asmb) =
                    &mut station.modules[module_idx].kind_state
                {
                    asmb.ticks_since_last_run = 0;
                }
            }
            continue;
        }

        // Stock cap check: skip if any output component is at or above its cap
        let (is_capped, was_capped) = {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            let was_capped = match &station.modules[module_idx].kind_state {
                ModuleKindState::Assembler(asmb) => asmb.capped,
                _ => false,
            };

            let cap_override = match &station.modules[module_idx].kind_state {
                ModuleKindState::Assembler(asmb) => asmb.cap_override.clone(),
                _ => std::collections::HashMap::new(),
            };

            let is_capped = recipe.outputs.iter().any(|output| {
                if let OutputSpec::Component { component_id, .. } = output {
                    let effective_cap = cap_override
                        .get(component_id)
                        .copied()
                        .or_else(|| assembler_def.max_stock.get(component_id).copied());

                    if let Some(cap) = effective_cap {
                        let current_count: u32 = station
                            .inventory
                            .iter()
                            .filter_map(|item| match item {
                                InventoryItem::Component {
                                    component_id: cid,
                                    count,
                                    ..
                                } if cid == component_id => Some(*count),
                                _ => None,
                            })
                            .sum();
                        current_count >= cap
                    } else {
                        false
                    }
                } else {
                    false
                }
            });

            (is_capped, was_capped)
        };

        if is_capped {
            if !was_capped {
                let module_id = state.stations.get(station_id).unwrap().modules[module_idx]
                    .id
                    .clone();
                if let Some(station) = state.stations.get_mut(station_id) {
                    if let ModuleKindState::Assembler(asmb) =
                        &mut station.modules[module_idx].kind_state
                    {
                        asmb.capped = true;
                    }
                }
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    Event::AssemblerCapped {
                        station_id: station_id.clone(),
                        module_id,
                    },
                ));
            }
            continue;
        }

        if was_capped {
            let module_id = state.stations.get(station_id).unwrap().modules[module_idx]
                .id
                .clone();
            if let Some(station) = state.stations.get_mut(station_id) {
                if let ModuleKindState::Assembler(asmb) =
                    &mut station.modules[module_idx].kind_state
                {
                    asmb.capped = false;
                }
            }
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                Event::AssemblerUncapped {
                    station_id: station_id.clone(),
                    module_id,
                },
            ));
        }

        // Tech gate: if recipe has OutputSpec::Ship output, require tech_ship_construction
        let has_ship_output = recipe
            .outputs
            .iter()
            .any(|o| matches!(o, OutputSpec::Ship { .. }));
        if has_ship_output
            && !state
                .research
                .unlocked
                .contains(&TechId(TECH_SHIP_CONSTRUCTION.to_string()))
        {
            // Only emit the event once per stall (when timer first reaches the interval)
            let first_trigger = {
                let station = state.stations.get(station_id).unwrap();
                match &station.modules[module_idx].kind_state {
                    ModuleKindState::Assembler(asmb) => {
                        asmb.ticks_since_last_run == assembler_def.assembly_interval_ticks
                    }
                    _ => false,
                }
            };
            if first_trigger {
                let module_id = state.stations.get(station_id).unwrap().modules[module_idx]
                    .id
                    .clone();
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    Event::ModuleAwaitingTech {
                        station_id: station_id.clone(),
                        module_id,
                        tech_id: TechId(TECH_SHIP_CONSTRUCTION.to_string()),
                    },
                ));
            }
            // Don't reset timer — let it stay above interval so we only emit once
            continue;
        }

        // Capacity pre-check: estimate net volume change (output volume minus consumed component volume).
        // OutputSpec::Ship is not stored in station inventory, so it has no volume impact here.
        let output_volume = {
            let mut produced_volume = 0.0_f32;
            for output in &recipe.outputs {
                if let OutputSpec::Component { component_id, .. } = output {
                    let comp_volume = content
                        .component_defs
                        .iter()
                        .find(|c| c.id == component_id.0)
                        .map_or(0.0, |c| c.volume_m3);
                    produced_volume += comp_volume;
                }
            }
            let mut consumed_volume = 0.0_f32;
            for input in &recipe.inputs {
                if let (InputFilter::Component(cid), InputAmount::Count(count)) =
                    (&input.filter, &input.amount)
                {
                    let comp_volume = content
                        .component_defs
                        .iter()
                        .find(|c| c.id == cid.0)
                        .map_or(0.0, |c| c.volume_m3);
                    consumed_volume += comp_volume * *count as f32;
                }
            }
            (produced_volume - consumed_volume).max(0.0)
        };

        let was_stalled = {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            match &station.modules[module_idx].kind_state {
                ModuleKindState::Assembler(asmb) => asmb.stalled,
                _ => false,
            }
        };

        let module_id = state.stations.get(station_id).unwrap().modules[module_idx]
            .id
            .clone();

        {
            let station = state.stations.get(station_id).unwrap();
            let current_used = crate::tasks::inventory_volume_m3(&station.inventory, content);
            let capacity = station.cargo_capacity_m3;
            let shortfall = (current_used + output_volume) - capacity;

            if shortfall > 0.0 {
                let station_mut = state.stations.get_mut(station_id).unwrap();
                if let ModuleKindState::Assembler(asmb) =
                    &mut station_mut.modules[module_idx].kind_state
                {
                    asmb.stalled = true;
                    asmb.ticks_since_last_run = 0;
                }
                if !was_stalled {
                    events.push(crate::emit(
                        &mut state.counters,
                        current_tick,
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
                if let ModuleKindState::Assembler(asmb) =
                    &mut station_mut.modules[module_idx].kind_state
                {
                    asmb.stalled = false;
                }
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    Event::ModuleResumed {
                        station_id: station_id.clone(),
                        module_id: module_id.clone(),
                    },
                ));
            }
        }

        // Execute assembler run
        resolve_assembler_run(
            state,
            station_id,
            module_idx,
            recipe,
            wear_per_run,
            content,
            rng,
            events,
        );

        // Reset timer
        if let Some(station) = state.stations.get_mut(station_id) {
            if let ModuleKindState::Assembler(asmb) = &mut station.modules[module_idx].kind_state {
                asmb.ticks_since_last_run = 0;
            }
        }
    }
}

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn resolve_assembler_run(
    state: &mut GameState,
    station_id: &StationId,
    module_idx: usize,
    recipe: &RecipeDef,
    wear_per_run: f32,
    content: &GameContent,
    rng: &mut impl rand::Rng,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    let module_id = state
        .stations
        .get(station_id)
        .map(|s| s.modules[module_idx].id.clone())
        .unwrap();

    // Consume inputs
    let mut consumed_element = String::new();
    let mut consumed_kg = 0.0_f32;
    let mut consumed_any = false;

    for input in &recipe.inputs {
        match (&input.filter, &input.amount) {
            (InputFilter::Element(el), InputAmount::Kg(required_kg)) => {
                let element_id = el.clone();
                consumed_element.clone_from(&element_id);
                let mut remaining = *required_kg;

                if let Some(station) = state.stations.get_mut(station_id) {
                    for item in &mut station.inventory {
                        if remaining <= 0.0 {
                            break;
                        }
                        if let InventoryItem::Material { element, kg, .. } = item {
                            if *element == element_id {
                                let take = kg.min(remaining);
                                *kg -= take;
                                remaining -= take;
                                consumed_kg += take;
                            }
                        }
                    }
                    // Remove empty material lots
                    station.inventory.retain(
                        |i| !matches!(i, InventoryItem::Material { kg, .. } if *kg < MIN_MEANINGFUL_KG),
                    );
                }
                if consumed_kg >= MIN_MEANINGFUL_KG {
                    consumed_any = true;
                }
            }
            (InputFilter::Component(cid), InputAmount::Count(required)) => {
                let mut remaining = *required;
                if let Some(station) = state.stations.get_mut(station_id) {
                    for item in &mut station.inventory {
                        if remaining == 0 {
                            break;
                        }
                        if let InventoryItem::Component {
                            component_id,
                            count,
                            ..
                        } = item
                        {
                            if component_id.0 == cid.0 {
                                let take = (*count).min(remaining);
                                *count -= take;
                                remaining -= take;
                            }
                        }
                    }
                    // Remove empty component stacks
                    station.inventory.retain(
                        |i| !matches!(i, InventoryItem::Component { count, .. } if *count == 0),
                    );
                }
                if remaining == 0 {
                    consumed_any = true;
                }
            }
            _ => {}
        }
    }

    if !consumed_any {
        return;
    }

    // Produce outputs
    for output in &recipe.outputs {
        match output {
            OutputSpec::Component {
                component_id,
                quality_formula,
            } => {
                let quality = match quality_formula {
                    QualityFormula::Fixed(q) => *q,
                    QualityFormula::ElementFractionTimesMultiplier { .. } => 1.0,
                };

                let produced_count = 1_u32;

                if let Some(station) = state.stations.get_mut(station_id) {
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

                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    Event::AssemblerRan {
                        station_id: station_id.clone(),
                        module_id: module_id.clone(),
                        recipe_id: recipe.id.clone(),
                        material_consumed_kg: consumed_kg,
                        material_element: consumed_element.clone(),
                        component_produced_id: component_id.clone(),
                        component_produced_count: produced_count,
                        component_quality: quality,
                    },
                ));
            }
            OutputSpec::Ship { cargo_capacity_m3 } => {
                let uuid = crate::generate_uuid(rng);
                let ship_id = ShipId(format!("ship_{uuid}"));
                let location_node = state
                    .stations
                    .get(station_id)
                    .unwrap()
                    .location_node
                    .clone();
                let ship = ShipState {
                    id: ship_id.clone(),
                    location_node,
                    owner: PrincipalId("principal_autopilot".to_string()),
                    inventory: vec![],
                    cargo_capacity_m3: *cargo_capacity_m3,
                    task: None,
                };
                state.ships.insert(ship_id.clone(), ship);
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    Event::ShipConstructed {
                        station_id: station_id.clone(),
                        ship_id,
                    },
                ));
            }
            _ => {} // Material, Slag handled by processor
        }
    }

    // Generate engineering data from assembly
    crate::research::generate_data(
        &mut state.research,
        crate::DataKind::EngineeringData,
        &format!("assemble_{}", recipe.id),
        &content.constants,
    );

    // Accumulate wear
    apply_wear(state, station_id, module_idx, wear_per_run, events);
}

fn tick_sensor_array_modules(
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
        // Extract sensor array def and module info
        let (sensor_def, power_needed, wear_per_run) = {
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
            let ModuleBehaviorDef::SensorArray(sensor_def) = &def.behavior else {
                continue;
            };
            (
                sensor_def.clone(),
                def.power_consumption_per_run,
                def.wear_per_run,
            )
        };

        // Tick timer; skip if interval not reached
        {
            let Some(station) = state.stations.get_mut(station_id) else {
                return;
            };
            if let ModuleKindState::SensorArray(ss) = &mut station.modules[module_idx].kind_state {
                ss.ticks_since_last_run += 1;
                if ss.ticks_since_last_run < sensor_def.scan_interval_ticks {
                    continue;
                }
            } else {
                continue;
            }
        }

        // Check power budget
        {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            if station.power_available_per_tick < power_needed {
                continue;
            }
        }

        // Reset timer
        {
            let Some(station) = state.stations.get_mut(station_id) else {
                return;
            };
            if let ModuleKindState::SensorArray(ss) = &mut station.modules[module_idx].kind_state {
                ss.ticks_since_last_run = 0;
            }
        }

        // Generate data using diminishing returns
        let amount = generate_data(
            &mut state.research,
            sensor_def.data_kind.clone(),
            &sensor_def.action_key,
            &content.constants,
        );

        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            Event::DataGenerated {
                kind: sensor_def.data_kind.clone(),
                amount,
            },
        ));

        // Accumulate wear
        apply_wear(state, station_id, module_idx, wear_per_run, events);
    }
}

#[allow(clippy::too_many_lines)]
fn tick_lab_modules(
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
        // Extract lab def and module info
        let (lab_def, power_needed, wear_per_run) = {
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
            let ModuleBehaviorDef::Lab(lab_def) = &def.behavior else {
                continue;
            };
            (
                lab_def.clone(),
                def.power_consumption_per_run,
                def.wear_per_run,
            )
        };

        // Tick timer; skip if interval not reached
        {
            let Some(station) = state.stations.get_mut(station_id) else {
                return;
            };
            if let ModuleKindState::Lab(ls) = &mut station.modules[module_idx].kind_state {
                ls.ticks_since_last_run += 1;
                if ls.ticks_since_last_run < lab_def.research_interval_ticks {
                    continue;
                }
            } else {
                continue;
            }
        }

        // Check power budget
        {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            if station.power_available_per_tick < power_needed {
                continue;
            }
        }

        // Check assigned_tech — if None, reset timer and skip
        let assigned_tech = {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            if let ModuleKindState::Lab(ls) = &station.modules[module_idx].kind_state {
                ls.assigned_tech.clone()
            } else {
                continue;
            }
        };

        let Some(tech_id) = assigned_tech else {
            // No tech assigned — reset timer and skip
            if let Some(station) = state.stations.get_mut(station_id) {
                if let ModuleKindState::Lab(ls) = &mut station.modules[module_idx].kind_state {
                    ls.ticks_since_last_run = 0;
                }
            }
            continue;
        };

        // Skip if assigned tech is already unlocked
        if state.research.unlocked.contains(&tech_id) {
            if let Some(station) = state.stations.get_mut(station_id) {
                if let ModuleKindState::Lab(ls) = &mut station.modules[module_idx].kind_state {
                    ls.ticks_since_last_run = 0;
                }
            }
            continue;
        }

        // Sum available data from data_pool for the lab's accepted_data kinds
        let available_data: f32 = lab_def
            .accepted_data
            .iter()
            .map(|kind| state.research.data_pool.get(kind).copied().unwrap_or(0.0))
            .sum();

        let module_id = state
            .stations
            .get(station_id)
            .map(|s| s.modules[module_idx].id.clone())
            .unwrap();

        let was_starved = {
            let station = state.stations.get(station_id).unwrap();
            if let ModuleKindState::Lab(ls) = &station.modules[module_idx].kind_state {
                ls.starved
            } else {
                false
            }
        };

        if available_data <= 0.0 {
            // Starved
            if let Some(station) = state.stations.get_mut(station_id) {
                if let ModuleKindState::Lab(ls) = &mut station.modules[module_idx].kind_state {
                    if !ls.starved {
                        ls.starved = true;
                        events.push(crate::emit(
                            &mut state.counters,
                            current_tick,
                            Event::LabStarved {
                                station_id: station_id.clone(),
                                module_id: module_id.clone(),
                            },
                        ));
                    }
                    ls.ticks_since_last_run = 0;
                }
            }
            continue;
        }

        // If was starved and data now available: resume
        if was_starved {
            if let Some(station) = state.stations.get_mut(station_id) {
                if let ModuleKindState::Lab(ls) = &mut station.modules[module_idx].kind_state {
                    ls.starved = false;
                }
            }
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                Event::LabResumed {
                    station_id: station_id.clone(),
                    module_id: module_id.clone(),
                },
            ));
        }

        // Consume data proportionally from accepted kinds
        let to_consume = available_data.min(lab_def.data_consumption_per_run);
        let ratio = to_consume / lab_def.data_consumption_per_run;

        // Consume proportionally from each accepted kind
        let mut consumed_total = 0.0_f32;
        if available_data > 0.0 {
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
        }

        // Compute wear efficiency
        let wear_value = state
            .stations
            .get(station_id)
            .map_or(0.0, |s| s.modules[module_idx].wear.wear);
        let efficiency = crate::wear::wear_efficiency(wear_value, &content.constants);

        // Compute points
        let points = lab_def.research_points_per_run * ratio * efficiency;

        // Add points to evidence[tech_id].points[domain]
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
                station_id: station_id.clone(),
                module_id: module_id.clone(),
                tech_id,
                data_consumed: consumed_total,
                points_produced: points,
                domain: lab_def.domain.clone(),
            },
        ));

        // Reset timer
        if let Some(station) = state.stations.get_mut(station_id) {
            if let ModuleKindState::Lab(ls) = &mut station.modules[module_idx].kind_state {
                ls.ticks_since_last_run = 0;
            }
        }

        // Apply wear
        apply_wear(state, station_id, module_idx, wear_per_run, events);
    }
}

#[allow(clippy::too_many_lines)]
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
        let (interval, power_needed, repair_reduction, kit_cost, repair_threshold) = {
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
                maint_def.repair_threshold,
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

        // Find most worn module (not self, wear >= threshold), sorted by wear desc then ID asc for determinism
        let target = {
            let Some(station) = state.stations.get(station_id) else {
                return;
            };
            let self_id = &station.modules[module_idx].id;
            let mut candidates: Vec<(usize, f32, String)> = station
                .modules
                .iter()
                .enumerate()
                .filter(|(_, m)| {
                    m.id != *self_id && m.wear.wear >= repair_threshold && m.wear.wear > 0.0
                })
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

fn apply_wear(
    state: &mut GameState,
    station_id: &StationId,
    module_idx: usize,
    wear_per_run: f32,
    events: &mut Vec<EventEnvelope>,
) {
    if wear_per_run <= 0.0 {
        return;
    }
    let current_tick = state.meta.tick;
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

#[cfg(test)]
mod lab_tests {
    use crate::*;
    use std::collections::{HashMap, HashSet};

    fn lab_content() -> GameContent {
        let mut content = crate::test_fixtures::base_content();
        content.module_defs.push(ModuleDef {
            id: "module_exploration_lab".to_string(),
            name: "Exploration Lab".to_string(),
            mass_kg: 3500.0,
            volume_m3: 7.0,
            power_consumption_per_run: 10.0,
            wear_per_run: 0.005,
            behavior: ModuleBehaviorDef::Lab(LabDef {
                domain: ResearchDomain::Exploration,
                data_consumption_per_run: 8.0,
                research_points_per_run: 4.0,
                accepted_data: vec![DataKind::ScanData],
                research_interval_ticks: 1,
            }),
        });
        content
    }

    fn lab_state(content: &GameContent) -> GameState {
        let station_id = StationId("station_test".to_string());
        GameState {
            meta: MetaState {
                tick: 0,
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
                    }],
                },
            )]),
            research: ResearchState {
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
    fn lab_consumes_data_and_produces_points() {
        let content = lab_content();
        let mut state = lab_state(&content);
        state.research.data_pool.insert(DataKind::ScanData, 100.0);

        let mut events = Vec::new();
        let station_id = StationId("station_test".to_string());
        super::tick_lab_modules(&mut state, &station_id, &content, &mut events);

        // Should have consumed 8.0 data
        let remaining = state.research.data_pool[&DataKind::ScanData];
        assert!(
            (remaining - 92.0).abs() < 1e-3,
            "expected 92.0 remaining, got {remaining}"
        );

        // Should have produced 4.0 points in Exploration domain
        let tech_id = TechId("tech_deep_scan_v1".to_string());
        let progress = state.research.evidence.get(&tech_id).unwrap();
        let points = progress.points[&ResearchDomain::Exploration];
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
        state.research.data_pool.insert(DataKind::ScanData, 4.0);

        let mut events = Vec::new();
        let station_id = StationId("station_test".to_string());
        super::tick_lab_modules(&mut state, &station_id, &content, &mut events);

        // Should have consumed all 4.0
        let remaining = state.research.data_pool[&DataKind::ScanData];
        assert!(
            remaining.abs() < 1e-3,
            "expected ~0.0 remaining, got {remaining}"
        );

        // Should have produced 2.0 points (4.0 * 0.5 ratio)
        let tech_id = TechId("tech_deep_scan_v1".to_string());
        let progress = state.research.evidence.get(&tech_id).unwrap();
        let points = progress.points[&ResearchDomain::Exploration];
        assert!(
            (points - 2.0).abs() < 1e-3,
            "expected 2.0 points, got {points}"
        );
    }

    #[test]
    fn lab_skips_unlocked_tech() {
        let content = lab_content();
        let mut state = lab_state(&content);
        state.research.data_pool.insert(DataKind::ScanData, 100.0);
        state
            .research
            .unlocked
            .insert(TechId("tech_deep_scan_v1".to_string()));

        let mut events = Vec::new();
        let station_id = StationId("station_test".to_string());
        super::tick_lab_modules(&mut state, &station_id, &content, &mut events);

        // Data should be unchanged
        let remaining = state.research.data_pool[&DataKind::ScanData];
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
        state.research.data_pool.insert(DataKind::ScanData, 100.0);

        // Clear assigned tech
        let station_id = StationId("station_test".to_string());
        let station = state.stations.get_mut(&station_id).unwrap();
        if let ModuleKindState::Lab(ls) = &mut station.modules[0].kind_state {
            ls.assigned_tech = None;
        }

        let mut events = Vec::new();
        super::tick_lab_modules(&mut state, &station_id, &content, &mut events);

        // Data should be unchanged
        let remaining = state.research.data_pool[&DataKind::ScanData];
        assert!((remaining - 100.0).abs() < 1e-3, "data should be unchanged");

        // No LabRan events
        let lab_ran = events
            .iter()
            .any(|e| matches!(&e.event, Event::LabRan { .. }));
        assert!(!lab_ran, "should not run lab without assigned tech");
    }

    // --- Sensor Array tests ---

    fn sensor_content() -> crate::GameContent {
        let mut content = crate::test_fixtures::base_content();
        content.module_defs.push(crate::ModuleDef {
            id: "module_sensor_array".to_string(),
            name: "Sensor Array".to_string(),
            mass_kg: 2500.0,
            volume_m3: 6.0,
            power_consumption_per_run: 8.0,
            wear_per_run: 0.003,
            behavior: ModuleBehaviorDef::SensorArray(crate::SensorArrayDef {
                data_kind: crate::DataKind::ScanData,
                action_key: "sensor_scan".to_string(),
                scan_interval_ticks: 5,
            }),
        });
        content
    }

    fn sensor_state(content: &crate::GameContent) -> GameState {
        let station_id = StationId("station_test".to_string());
        GameState {
            meta: crate::MetaState {
                tick: 0,
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
                    location_node: crate::NodeId("node_test".to_string()),
                    inventory: vec![],
                    cargo_capacity_m3: 2000.0,
                    power_available_per_tick: 100.0,
                    modules: vec![crate::ModuleState {
                        id: crate::ModuleInstanceId("sensor_inst_0001".to_string()),
                        def_id: "module_sensor_array".to_string(),
                        enabled: true,
                        kind_state: ModuleKindState::SensorArray(crate::SensorArrayState {
                            ticks_since_last_run: 0,
                        }),
                        wear: crate::WearState::default(),
                    }],
                },
            )]),
            research: crate::ResearchState {
                unlocked: std::collections::HashSet::new(),
                data_pool: HashMap::new(),
                evidence: HashMap::new(),
                action_counts: HashMap::new(),
            },
            balance: 0.0,
            counters: crate::Counters {
                next_event_id: 0,
                next_command_id: 0,
                next_asteroid_id: 0,
                next_lot_id: 0,
                next_module_instance_id: 2,
            },
        }
    }

    #[test]
    fn sensor_array_generates_scan_data_after_interval() {
        let content = sensor_content();
        let mut state = sensor_state(&content);
        let station_id = StationId("station_test".to_string());

        // Tick 4 times — interval is 5, should not fire yet
        for _ in 0..4 {
            let mut events = Vec::new();
            super::tick_sensor_array_modules(&mut state, &station_id, &content, &mut events);
            let generated = events
                .iter()
                .any(|e| matches!(&e.event, Event::DataGenerated { .. }));
            assert!(!generated, "should not generate data before interval");
        }

        // Tick once more — should fire
        let mut events = Vec::new();
        super::tick_sensor_array_modules(&mut state, &station_id, &content, &mut events);
        let generated = events
            .iter()
            .any(|e| matches!(&e.event, Event::DataGenerated { .. }));
        assert!(generated, "should generate data at interval");

        // Check data pool has ScanData
        let scan_data = state
            .research
            .data_pool
            .get(&crate::DataKind::ScanData)
            .copied()
            .unwrap_or(0.0);
        assert!(scan_data > 0.0, "ScanData should be > 0 after sensor run");
    }

    #[test]
    fn sensor_array_uses_diminishing_returns() {
        let content = sensor_content();
        let mut state = sensor_state(&content);
        let station_id = StationId("station_test".to_string());

        // Run through two complete intervals and capture amounts
        let mut amounts = Vec::new();
        for run in 0..2 {
            // Tick through interval
            for tick in 0..5 {
                let mut events = Vec::new();
                super::tick_sensor_array_modules(&mut state, &station_id, &content, &mut events);
                if tick == 4 {
                    // Last tick of interval — should fire
                    for event in &events {
                        if let Event::DataGenerated { amount, .. } = &event.event {
                            amounts.push(*amount);
                        }
                    }
                }
            }
            let _ = run;
        }

        assert_eq!(amounts.len(), 2, "should have fired twice");
        assert!(
            amounts[1] < amounts[0],
            "second run should yield less due to diminishing returns (got {} then {})",
            amounts[0],
            amounts[1]
        );
    }

    #[test]
    fn sensor_array_disabled_does_not_generate() {
        let content = sensor_content();
        let mut state = sensor_state(&content);
        let station_id = StationId("station_test".to_string());

        // Disable the module
        state.stations.get_mut(&station_id).unwrap().modules[0].enabled = false;

        // Tick through full interval
        for _ in 0..10 {
            let mut events = Vec::new();
            super::tick_sensor_array_modules(&mut state, &station_id, &content, &mut events);
            let generated = events
                .iter()
                .any(|e| matches!(&e.event, Event::DataGenerated { .. }));
            assert!(!generated, "disabled sensor should not generate data");
        }

        let scan_data = state
            .research
            .data_pool
            .get(&crate::DataKind::ScanData)
            .copied()
            .unwrap_or(0.0);
        assert!(
            scan_data == 0.0,
            "no ScanData should exist when sensor is disabled"
        );
    }
}

#[cfg(test)]
mod assembler_component_tests {
    use crate::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use std::collections::{HashMap, HashSet};

    /// Content with an assembler that requires both Fe material and thruster components.
    fn assembler_content_with_component_input() -> GameContent {
        let mut content = crate::test_fixtures::base_content();
        content.component_defs.push(ComponentDef {
            id: "thruster".to_string(),
            name: "Thruster".to_string(),
            mass_kg: 50.0,
            volume_m3: 2.0,
        });
        content.component_defs.push(ComponentDef {
            id: "hull_plate".to_string(),
            name: "Hull Plate".to_string(),
            mass_kg: 200.0,
            volume_m3: 5.0,
        });
        // Assembler recipe: 100kg Fe + 4 thrusters => 1 hull_plate
        content.module_defs.push(ModuleDef {
            id: "module_shipyard".to_string(),
            name: "Shipyard".to_string(),
            mass_kg: 5000.0,
            volume_m3: 20.0,
            power_consumption_per_run: 10.0,
            wear_per_run: 0.0,
            behavior: ModuleBehaviorDef::Assembler(AssemblerDef {
                assembly_interval_ticks: 1,
                recipes: vec![RecipeDef {
                    id: "recipe_hull_plate".to_string(),
                    inputs: vec![
                        RecipeInput {
                            filter: InputFilter::Element("Fe".to_string()),
                            amount: InputAmount::Kg(100.0),
                        },
                        RecipeInput {
                            filter: InputFilter::Component(ComponentId("thruster".to_string())),
                            amount: InputAmount::Count(4),
                        },
                    ],
                    outputs: vec![OutputSpec::Component {
                        component_id: ComponentId("hull_plate".to_string()),
                        quality_formula: QualityFormula::Fixed(0.9),
                    }],
                    efficiency: 1.0,
                }],
                max_stock: HashMap::new(),
            }),
        });
        content
    }

    fn assembler_state(content: &GameContent) -> GameState {
        let station_id = StationId("station_test".to_string());
        GameState {
            meta: MetaState {
                tick: 0,
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
                    inventory: vec![
                        InventoryItem::Material {
                            element: "Fe".to_string(),
                            kg: 200.0,
                            quality: 0.8,
                        },
                        InventoryItem::Component {
                            component_id: ComponentId("thruster".to_string()),
                            count: 6,
                            quality: 0.9,
                        },
                    ],
                    cargo_capacity_m3: 10_000.0,
                    power_available_per_tick: 100.0,
                    modules: vec![ModuleState {
                        id: ModuleInstanceId("shipyard_inst_0001".to_string()),
                        def_id: "module_shipyard".to_string(),
                        enabled: true,
                        kind_state: ModuleKindState::Assembler(AssemblerState {
                            ticks_since_last_run: 0,
                            stalled: false,
                            capped: false,
                            cap_override: HashMap::new(),
                        }),
                        wear: WearState::default(),
                    }],
                },
            )]),
            research: ResearchState {
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
    fn assembler_consumes_component_inputs() {
        let content = assembler_content_with_component_input();
        let mut state = assembler_state(&content);
        let station_id = StationId("station_test".to_string());

        let mut events = Vec::new();
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        super::tick_assembler_modules(&mut state, &station_id, &content, &mut rng, &mut events);

        let station = state.stations.get(&station_id).unwrap();

        // Fe should be consumed: 200 - 100 = 100 remaining
        let fe_remaining: f32 = station
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
                _ => None,
            })
            .sum();
        assert!(
            (fe_remaining - 100.0).abs() < 1e-3,
            "expected 100kg Fe remaining, got {fe_remaining}"
        );

        // Thrusters should be consumed: 6 - 4 = 2 remaining
        let thruster_count: u32 = station
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Component {
                    component_id,
                    count,
                    ..
                } if component_id.0 == "thruster" => Some(*count),
                _ => None,
            })
            .sum();
        assert_eq!(
            thruster_count, 2,
            "expected 2 thrusters remaining, got {thruster_count}"
        );

        // Hull plate should be produced
        let hull_plate_count: u32 = station
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Component {
                    component_id,
                    count,
                    ..
                } if component_id.0 == "hull_plate" => Some(*count),
                _ => None,
            })
            .sum();
        assert_eq!(
            hull_plate_count, 1,
            "expected 1 hull_plate produced, got {hull_plate_count}"
        );

        // Should have AssemblerRan event
        let assembler_ran = events
            .iter()
            .any(|e| matches!(&e.event, Event::AssemblerRan { .. }));
        assert!(assembler_ran, "expected AssemblerRan event");
    }

    #[test]
    fn assembler_skips_when_insufficient_components() {
        let content = assembler_content_with_component_input();
        let mut state = assembler_state(&content);
        let station_id = StationId("station_test".to_string());

        // Reduce thrusters to 3 (need 4)
        let station = state.stations.get_mut(&station_id).unwrap();
        for item in &mut station.inventory {
            if let InventoryItem::Component {
                component_id,
                count,
                ..
            } = item
            {
                if component_id.0 == "thruster" {
                    *count = 3;
                }
            }
        }

        let mut events = Vec::new();
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        super::tick_assembler_modules(&mut state, &station_id, &content, &mut rng, &mut events);

        // No AssemblerRan event
        let assembler_ran = events
            .iter()
            .any(|e| matches!(&e.event, Event::AssemblerRan { .. }));
        assert!(
            !assembler_ran,
            "should not run assembler with insufficient components"
        );

        // Fe should be unchanged
        let station = state.stations.get(&station_id).unwrap();
        let fe_remaining: f32 = station
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
                _ => None,
            })
            .sum();
        assert!(
            (fe_remaining - 200.0).abs() < 1e-3,
            "Fe should be unchanged"
        );
    }

    #[test]
    fn assembler_removes_component_stack_when_fully_consumed() {
        let content = assembler_content_with_component_input();
        let mut state = assembler_state(&content);
        let station_id = StationId("station_test".to_string());

        // Set thruster count to exactly 4
        let station = state.stations.get_mut(&station_id).unwrap();
        for item in &mut station.inventory {
            if let InventoryItem::Component {
                component_id,
                count,
                ..
            } = item
            {
                if component_id.0 == "thruster" {
                    *count = 4;
                }
            }
        }

        let mut events = Vec::new();
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        super::tick_assembler_modules(&mut state, &station_id, &content, &mut rng, &mut events);

        let station = state.stations.get(&station_id).unwrap();

        // Thruster stack should be removed entirely (count was 4, consumed 4)
        let thruster_exists = station.inventory.iter().any(|i| {
            matches!(i, InventoryItem::Component { component_id, .. } if component_id.0 == "thruster")
        });
        assert!(
            !thruster_exists,
            "thruster stack should be removed when fully consumed"
        );

        // Hull plate should be produced
        let hull_plate_count: u32 = station
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Component {
                    component_id,
                    count,
                    ..
                } if component_id.0 == "hull_plate" => Some(*count),
                _ => None,
            })
            .sum();
        assert_eq!(hull_plate_count, 1);
    }

    // --- Ship construction tests ---

    fn shipyard_content() -> GameContent {
        let mut content = crate::test_fixtures::base_content();
        content.component_defs.push(ComponentDef {
            id: "thruster".to_string(),
            name: "Thruster".to_string(),
            mass_kg: 50.0,
            volume_m3: 2.0,
        });
        // Shipyard recipe: 100kg Fe + 2 thrusters => Ship with 50 m3 cargo
        content.module_defs.push(ModuleDef {
            id: "module_shipyard".to_string(),
            name: "Shipyard".to_string(),
            mass_kg: 5000.0,
            volume_m3: 20.0,
            power_consumption_per_run: 10.0,
            wear_per_run: 0.0,
            behavior: ModuleBehaviorDef::Assembler(AssemblerDef {
                assembly_interval_ticks: 1,
                recipes: vec![RecipeDef {
                    id: "recipe_build_ship".to_string(),
                    inputs: vec![
                        RecipeInput {
                            filter: InputFilter::Element("Fe".to_string()),
                            amount: InputAmount::Kg(100.0),
                        },
                        RecipeInput {
                            filter: InputFilter::Component(ComponentId("thruster".to_string())),
                            amount: InputAmount::Count(2),
                        },
                    ],
                    outputs: vec![OutputSpec::Ship {
                        cargo_capacity_m3: 50.0,
                    }],
                    efficiency: 1.0,
                }],
                max_stock: HashMap::new(),
            }),
        });
        content
    }

    fn shipyard_state(content: &GameContent) -> GameState {
        let station_id = StationId("station_test".to_string());
        GameState {
            meta: MetaState {
                tick: 0,
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
                    inventory: vec![
                        InventoryItem::Material {
                            element: "Fe".to_string(),
                            kg: 200.0,
                            quality: 0.8,
                        },
                        InventoryItem::Component {
                            component_id: ComponentId("thruster".to_string()),
                            count: 4,
                            quality: 0.9,
                        },
                    ],
                    cargo_capacity_m3: 10_000.0,
                    power_available_per_tick: 100.0,
                    modules: vec![ModuleState {
                        id: ModuleInstanceId("shipyard_inst_0001".to_string()),
                        def_id: "module_shipyard".to_string(),
                        enabled: true,
                        kind_state: ModuleKindState::Assembler(AssemblerState {
                            ticks_since_last_run: 0,
                            stalled: false,
                            capped: false,
                            cap_override: HashMap::new(),
                        }),
                        wear: WearState::default(),
                    }],
                },
            )]),
            research: ResearchState {
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
    fn shipyard_constructs_ship_when_tech_unlocked() {
        let content = shipyard_content();
        let mut state = shipyard_state(&content);
        state
            .research
            .unlocked
            .insert(TechId("tech_ship_construction".to_string()));

        let station_id = StationId("station_test".to_string());
        let mut events = Vec::new();
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        super::tick_assembler_modules(&mut state, &station_id, &content, &mut rng, &mut events);

        // A new ship should exist
        assert_eq!(state.ships.len(), 1, "expected 1 ship constructed");
        let ship = state.ships.values().next().unwrap();
        assert!(
            ship.id.0.starts_with("ship_"),
            "ship ID should start with ship_"
        );
        assert_eq!(ship.location_node.0, "node_test");
        assert!((ship.cargo_capacity_m3 - 50.0).abs() < 1e-3);
        assert!(ship.task.is_none());

        // ShipConstructed event should be emitted
        let constructed = events
            .iter()
            .any(|e| matches!(&e.event, Event::ShipConstructed { .. }));
        assert!(constructed, "expected ShipConstructed event");

        // Inputs should be consumed: Fe 200 - 100 = 100
        let station = state.stations.get(&station_id).unwrap();
        let fe_remaining: f32 = station
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
                _ => None,
            })
            .sum();
        assert!(
            (fe_remaining - 100.0).abs() < 1e-3,
            "expected 100kg Fe remaining, got {fe_remaining}"
        );

        // Thrusters: 4 - 2 = 2
        let thruster_count: u32 = station
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Component {
                    component_id,
                    count,
                    ..
                } if component_id.0 == "thruster" => Some(*count),
                _ => None,
            })
            .sum();
        assert_eq!(thruster_count, 2, "expected 2 thrusters remaining");
    }

    #[test]
    fn shipyard_stalls_without_tech() {
        let content = shipyard_content();
        let mut state = shipyard_state(&content);
        // tech_ship_construction NOT unlocked

        let station_id = StationId("station_test".to_string());
        let mut events = Vec::new();
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        super::tick_assembler_modules(&mut state, &station_id, &content, &mut rng, &mut events);

        // No ship should be spawned
        assert!(
            state.ships.is_empty(),
            "no ship should be spawned without tech"
        );

        // ModuleAwaitingTech event should be emitted
        let awaiting = events
            .iter()
            .any(|e| matches!(&e.event, Event::ModuleAwaitingTech { .. }));
        assert!(awaiting, "expected ModuleAwaitingTech event");

        // Inputs should NOT be consumed
        let station = state.stations.get(&station_id).unwrap();
        let fe_remaining: f32 = station
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Material { element, kg, .. } if element == "Fe" => Some(*kg),
                _ => None,
            })
            .sum();
        assert!(
            (fe_remaining - 200.0).abs() < 1e-3,
            "Fe should be unchanged at 200kg, got {fe_remaining}"
        );

        let thruster_count: u32 = station
            .inventory
            .iter()
            .filter_map(|i| match i {
                InventoryItem::Component {
                    component_id,
                    count,
                    ..
                } if component_id.0 == "thruster" => Some(*count),
                _ => None,
            })
            .sum();
        assert_eq!(thruster_count, 4, "thrusters should be unchanged at 4");
    }
}
