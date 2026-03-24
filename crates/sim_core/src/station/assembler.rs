use crate::tasks::{ship_construction_enabled, ship_construction_tech_id};
use crate::{
    Event, EventEnvelope, GameContent, GameState, InputAmount, InputFilter, InventoryItem,
    ModuleBehaviorDef, OutputSpec, PrincipalId, QualityFormula, RecipeDef, ShipId, ShipState,
    StationId,
};

pub(super) fn tick_assembler_modules(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    rng: &mut impl rand::Rng,
    events: &mut Vec<EventEnvelope>,
    scratch: &mut Vec<usize>,
) {
    let module_count = state
        .stations
        .get(station_id)
        .map_or(0, |s| s.modules.len());

    // Collect assembler module indices, sorted by priority (desc) then id (asc)
    scratch.clear();
    scratch.extend((0..module_count).filter(|&module_index| {
        state
            .stations
            .get(station_id)
            .and_then(|s| s.modules.get(module_index))
            .and_then(|m| content.module_defs.get(&m.def_id))
            .is_some_and(|d| matches!(d.behavior, ModuleBehaviorDef::Assembler(_)))
    }));

    if let Some(station) = state.stations.get(station_id) {
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

        let assembler_def = if let ModuleBehaviorDef::Assembler(ad) = &ctx.def.behavior {
            ad.clone()
        } else {
            continue;
        };

        if !super::should_run(state, &ctx) {
            continue;
        }

        let outcome = execute(&ctx, &assembler_def, state, content, rng, events);
        super::apply_run_result(state, &ctx, outcome, content, events);
    }
}

/// Assembler-specific logic. Returns a `RunOutcome` for the framework to apply.
#[allow(clippy::too_many_lines)]
fn execute(
    ctx: &super::ModuleTickContext,
    assembler_def: &crate::AssemblerDef,
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl rand::Rng,
    events: &mut Vec<EventEnvelope>,
) -> super::RunOutcome {
    let selected = {
        let Some(station) = state.stations.get(&ctx.station_id) else {
            return super::RunOutcome::Skipped { reset_timer: true };
        };
        match &station.modules[ctx.module_idx].kind_state {
            crate::ModuleKindState::Assembler(asmb) => asmb.selected_recipe.clone(),
            _ => return super::RunOutcome::Skipped { reset_timer: true },
        }
    };

    // Recipe fallback: if selected recipe is not in assembler's recipe list, reset it
    let recipe_id = if let Some(ref sel_id) = selected {
        if assembler_def.recipes.contains(sel_id) {
            selected
        } else {
            let new_recipe = assembler_def.recipes.first().cloned();
            if let Some(station) = state.stations.get_mut(&ctx.station_id) {
                if let crate::ModuleKindState::Assembler(asmb) =
                    &mut station.modules[ctx.module_idx].kind_state
                {
                    asmb.selected_recipe.clone_from(&new_recipe);
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
            new_recipe
        }
    } else {
        selected
    };

    let recipe_id = recipe_id.as_ref().or_else(|| assembler_def.recipes.first());
    let Some(recipe) = recipe_id.and_then(|id| content.recipes.get(id)) else {
        return super::RunOutcome::Skipped { reset_timer: true };
    };

    // Tech gate: if recipe requires a tech that isn't unlocked, skip
    if let Some(ref required_tech) = recipe.required_tech {
        if !state.research.unlocked.iter().any(|t| t == required_tech) {
            let first_trigger = {
                let Some(station) = state.stations.get(&ctx.station_id) else {
                    return super::RunOutcome::Skipped { reset_timer: false };
                };
                match &station.modules[ctx.module_idx].kind_state {
                    crate::ModuleKindState::Assembler(asmb) => {
                        asmb.ticks_since_last_run == assembler_def.assembly_interval_ticks
                    }
                    _ => false,
                }
            };
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
            return super::RunOutcome::Skipped { reset_timer: false };
        }
    }

    // Check input availability
    let input_available = {
        let Some(station) = state.stations.get(&ctx.station_id) else {
            return super::RunOutcome::Skipped { reset_timer: true };
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
                #[allow(clippy::cast_possible_truncation)]
                (InputFilter::Module(def_id), InputAmount::Count(required)) => {
                    let available = station
                        .inventory
                        .iter()
                        .filter(|item| {
                            matches!(item, InventoryItem::Module { module_def_id, .. } if *module_def_id == *def_id)
                        })
                        .count() as u32;
                    available >= *required
                }
                _ => false,
            })
    };

    if !input_available {
        return super::RunOutcome::Skipped { reset_timer: true };
    }

    // Stock cap check
    let is_capped = {
        let Some(station) = state.stations.get(&ctx.station_id) else {
            return super::RunOutcome::Skipped { reset_timer: true };
        };

        let cap_override = match &station.modules[ctx.module_idx].kind_state {
            crate::ModuleKindState::Assembler(asmb) => asmb.cap_override.clone(),
            _ => std::collections::HashMap::new(),
        };

        recipe.outputs.iter().any(|output| {
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
        })
    };

    if is_capped {
        return super::RunOutcome::Stalled(super::StallReason::StockCap);
    }

    // Tech gate: if recipe has ship output, require EnableShipConstruction effect
    let has_ship_output = recipe
        .outputs
        .iter()
        .any(|o| matches!(o, OutputSpec::Ship { .. }));
    if has_ship_output && !ship_construction_enabled(&state.research, content) {
        // Only emit ModuleAwaitingTech once — when timer first reaches the interval.
        // should_run() incremented the timer; if it equals exactly the interval,
        // this is the first time we've reached it.
        let first_trigger = {
            let Some(station) = state.stations.get(&ctx.station_id) else {
                return super::RunOutcome::Skipped { reset_timer: false };
            };
            match &station.modules[ctx.module_idx].kind_state {
                crate::ModuleKindState::Assembler(asmb) => {
                    asmb.ticks_since_last_run == assembler_def.assembly_interval_ticks
                }
                _ => false,
            }
        };
        if first_trigger {
            if let Some(tech_id) = ship_construction_tech_id(content) {
                let current_tick = state.meta.tick;
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    Event::ModuleAwaitingTech {
                        station_id: ctx.station_id.clone(),
                        module_id: ctx.module_id.clone(),
                        tech_id: tech_id.clone(),
                    },
                ));
            }
        }
        // Don't reset timer — let it stay above interval so we only emit once
        return super::RunOutcome::Skipped { reset_timer: false };
    }

    // Capacity pre-check: estimate net volume change
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
            match (&input.filter, &input.amount) {
                (InputFilter::Component(cid), InputAmount::Count(count)) => {
                    let comp_volume = content
                        .component_defs
                        .iter()
                        .find(|c| c.id == cid.0)
                        .map_or(0.0, |c| c.volume_m3);
                    consumed_volume += comp_volume * *count as f32;
                }
                (InputFilter::Module(def_id), InputAmount::Count(count)) => {
                    let mod_volume = content.module_defs.get(def_id).map_or(0.0, |d| d.volume_m3);
                    consumed_volume += mod_volume * *count as f32;
                }
                _ => {}
            }
        }
        (produced_volume - consumed_volume).max(0.0)
    };

    let Some(station) = state.stations.get_mut(&ctx.station_id) else {
        return super::RunOutcome::Skipped { reset_timer: true };
    };
    let current_used = station.used_volume_m3(content);
    let capacity = station.cargo_capacity_m3;
    let shortfall = (current_used + output_volume) - capacity;

    if shortfall > 0.0 {
        return super::RunOutcome::Stalled(super::StallReason::VolumeCap {
            shortfall_m3: shortfall,
        });
    }

    // All checks passed — execute the assembler run
    resolve_assembler_run(ctx, state, recipe, content, rng, events);

    super::RunOutcome::Completed
}

#[allow(clippy::too_many_lines)]
fn resolve_assembler_run(
    ctx: &super::ModuleTickContext,
    state: &mut GameState,
    recipe: &RecipeDef,
    content: &GameContent,
    rng: &mut impl rand::Rng,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;
    let min_kg = content.constants.min_meaningful_kg;

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

                if let Some(station) = state.stations.get_mut(&ctx.station_id) {
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
                        |i| !matches!(i, InventoryItem::Material { kg, .. } if *kg < min_kg),
                    );
                }
                if consumed_kg >= min_kg {
                    consumed_any = true;
                }
            }
            (InputFilter::Component(cid), InputAmount::Count(required)) => {
                let mut remaining = *required;
                if let Some(station) = state.stations.get_mut(&ctx.station_id) {
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
            (InputFilter::Module(def_id), InputAmount::Count(required)) => {
                let mut remaining = *required;
                if let Some(station) = state.stations.get_mut(&ctx.station_id) {
                    let mut indices_to_remove = Vec::new();
                    for (index, item) in station.inventory.iter().enumerate() {
                        if remaining == 0 {
                            break;
                        }
                        if matches!(item, InventoryItem::Module { module_def_id, .. } if *module_def_id == *def_id)
                        {
                            indices_to_remove.push(index);
                            remaining -= 1;
                        }
                    }
                    for index in indices_to_remove.into_iter().rev() {
                        station.inventory.remove(index);
                    }
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

                if let Some(station) = state.stations.get_mut(&ctx.station_id) {
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
                        station_id: ctx.station_id.clone(),
                        module_id: ctx.module_id.clone(),
                        recipe_id: recipe.id.clone(),
                        material_consumed_kg: consumed_kg,
                        material_element: consumed_element.clone(),
                        component_produced_id: component_id.clone(),
                        component_produced_count: produced_count,
                        component_quality: quality,
                    },
                ));
            }
            OutputSpec::Ship { hull_id } => {
                let Some(hull) = content.hulls.get(hull_id) else {
                    return; // hull_id not found in content
                };
                let uuid = crate::generate_uuid(rng);
                let ship_id = ShipId(format!("ship_{uuid}"));
                let Some(station) = state.stations.get(&ctx.station_id) else {
                    return;
                };
                let ship_position = station.position.clone();
                let mut ship = ShipState {
                    id: ship_id.clone(),
                    position: ship_position.clone(),
                    owner: PrincipalId("principal_autopilot".to_string()),
                    inventory: vec![],
                    cargo_capacity_m3: hull.cargo_capacity_m3,
                    task: None,
                    speed_ticks_per_au: Some(hull.base_speed_ticks_per_au),
                    modifiers: crate::modifiers::ModifierSet::default(),
                    hull_id: hull_id.clone(),
                    fitted_modules: content
                        .fitting_templates
                        .get(hull_id)
                        .cloned()
                        .unwrap_or_default(),
                    propellant_kg: hull.base_propellant_capacity_kg,
                    propellant_capacity_kg: hull.base_propellant_capacity_kg,
                };
                crate::commands::recompute_ship_stats(&mut ship, content);
                ship.propellant_kg = ship.propellant_capacity_kg;
                let actual_cargo = ship.cargo_capacity_m3;
                let event_fitted = ship.fitted_modules.clone();
                state.ships.insert(ship_id.clone(), ship);
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    Event::ShipConstructed {
                        station_id: ctx.station_id.clone(),
                        ship_id,
                        position: ship_position,
                        cargo_capacity_m3: f64::from(actual_cargo),
                        hull_id: hull_id.clone(),
                        fitted_modules: event_fitted,
                    },
                ));
            }
            _ => {} // Material, Slag handled by processor
        }
    }

    // Generate engineering data from assembly
    crate::research::generate_data(
        &mut state.research,
        crate::DataKind::ManufacturingData,
        &format!("assemble_{}", recipe.id),
        &content.constants,
    );
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
        let hull_plate_recipe = RecipeDef {
            id: RecipeId("recipe_hull_plate".to_string()),
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
            thermal_req: None,
            required_tech: None,
            tags: vec![],
        };
        let recipe_id = crate::test_fixtures::insert_recipe(&mut content, hull_plate_recipe);
        content.module_defs.insert(
            "module_shipyard".to_string(),
            ModuleDef {
                id: "module_shipyard".to_string(),
                name: "Shipyard".to_string(),
                mass_kg: 5000.0,
                volume_m3: 20.0,
                power_consumption_per_run: 10.0,
                wear_per_run: 0.0,
                behavior: ModuleBehaviorDef::Assembler(AssemblerDef {
                    assembly_interval_minutes: 1,
                    assembly_interval_ticks: 1,
                    recipes: vec![recipe_id],
                    max_stock: HashMap::new(),
                }),
                thermal: None,
                compatible_slots: Vec::new(),
                ship_modifiers: Vec::new(),
            },
        );
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
            asteroids: AHashMap::default(),
            ships: AHashMap::default(),
            stations: [(
                station_id.clone(),
                StationState {
                    id: station_id,
                    position: crate::test_fixtures::test_position(),
                    inventory: vec![
                        InventoryItem::Material {
                            element: "Fe".to_string(),
                            kg: 200.0,
                            quality: 0.8,
                            thermal: None,
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
                            selected_recipe: None,
                        }),
                        wear: WearState::default(),
                        power_stalled: false,
                        manufacturing_priority: 0,
                        thermal: None,
                    }],
                    modifiers: crate::modifiers::ModifierSet::default(),
                    power: PowerState::default(),
                    cached_inventory_volume_m3: None,
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
            body_cache: AHashMap::default(),
        }
    }

    #[test]
    fn assembler_consumes_component_inputs() {
        let content = assembler_content_with_component_input();
        let mut state = assembler_state(&content);
        let station_id = StationId("station_test".to_string());

        let mut events = Vec::new();
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        super::tick_assembler_modules(
            &mut state,
            &station_id,
            &content,
            &mut rng,
            &mut events,
            &mut Vec::new(),
        );

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
        super::tick_assembler_modules(
            &mut state,
            &station_id,
            &content,
            &mut rng,
            &mut events,
            &mut Vec::new(),
        );

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
        super::tick_assembler_modules(
            &mut state,
            &station_id,
            &content,
            &mut rng,
            &mut events,
            &mut Vec::new(),
        );

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
        content.techs.push(TechDef {
            id: TechId("tech_ship_construction".to_string()),
            name: "Ship Construction".to_string(),
            prereqs: vec![],
            domain_requirements: HashMap::new(),
            accepted_data: vec![],
            difficulty: 10.0,
            effects: vec![TechEffect::EnableShipConstruction],
        });
        content.component_defs.push(ComponentDef {
            id: "thruster".to_string(),
            name: "Thruster".to_string(),
            mass_kg: 50.0,
            volume_m3: 2.0,
        });
        // Add a hull def for the shipyard test
        content.hulls.insert(
            crate::HullId("hull_test_ship".to_string()),
            crate::HullDef {
                id: crate::HullId("hull_test_ship".to_string()),
                name: "Test Ship Hull".to_string(),
                mass_kg: 1000.0,
                cargo_capacity_m3: 50.0,
                base_speed_ticks_per_au: 2000,
                base_propellant_capacity_kg: 100.0,
                slots: vec![],
                bonuses: vec![],
                required_tech: None,
                tags: vec![],
            },
        );
        // Shipyard recipe: 100kg Fe + 2 thrusters => Ship with test hull
        let ship_recipe = RecipeDef {
            id: RecipeId("recipe_build_ship".to_string()),
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
                hull_id: crate::HullId("hull_test_ship".to_string()),
            }],
            efficiency: 1.0,
            thermal_req: None,
            required_tech: None,
            tags: vec![],
        };
        let recipe_id = crate::test_fixtures::insert_recipe(&mut content, ship_recipe);
        content.module_defs.insert(
            "module_shipyard".to_string(),
            ModuleDef {
                id: "module_shipyard".to_string(),
                name: "Shipyard".to_string(),
                mass_kg: 5000.0,
                volume_m3: 20.0,
                power_consumption_per_run: 10.0,
                wear_per_run: 0.0,
                behavior: ModuleBehaviorDef::Assembler(AssemblerDef {
                    assembly_interval_minutes: 1,
                    assembly_interval_ticks: 1,
                    recipes: vec![recipe_id],
                    max_stock: HashMap::new(),
                }),
                thermal: None,
                compatible_slots: Vec::new(),
                ship_modifiers: Vec::new(),
            },
        );
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
            asteroids: AHashMap::default(),
            ships: AHashMap::default(),
            stations: [(
                station_id.clone(),
                StationState {
                    id: station_id,
                    position: crate::test_fixtures::test_position(),
                    inventory: vec![
                        InventoryItem::Material {
                            element: "Fe".to_string(),
                            kg: 200.0,
                            quality: 0.8,
                            thermal: None,
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
                            selected_recipe: None,
                        }),
                        wear: WearState::default(),
                        power_stalled: false,
                        manufacturing_priority: 0,
                        thermal: None,
                    }],
                    modifiers: crate::modifiers::ModifierSet::default(),
                    power: PowerState::default(),
                    cached_inventory_volume_m3: None,
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
            body_cache: AHashMap::default(),
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
        super::tick_assembler_modules(
            &mut state,
            &station_id,
            &content,
            &mut rng,
            &mut events,
            &mut Vec::new(),
        );

        // A new ship should exist
        assert_eq!(state.ships.len(), 1, "expected 1 ship constructed");
        let ship = state.ships.values().next().unwrap();
        assert!(
            ship.id.0.starts_with("ship_"),
            "ship ID should start with ship_"
        );
        assert_eq!(ship.position, crate::test_fixtures::test_position());
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
        super::tick_assembler_modules(
            &mut state,
            &station_id,
            &content,
            &mut rng,
            &mut events,
            &mut Vec::new(),
        );

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
