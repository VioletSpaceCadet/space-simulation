use super::{MIN_MEANINGFUL_KG, TECH_SHIP_CONSTRUCTION};
use crate::{
    Event, EventEnvelope, GameContent, GameState, InputAmount, InputFilter, InventoryItem,
    ModuleBehaviorDef, OutputSpec, PrincipalId, QualityFormula, RecipeDef, ShipId, ShipState,
    StationId, TechId,
};

pub(super) fn tick_assembler_modules(
    state: &mut GameState,
    station_id: &StationId,
    content: &GameContent,
    rng: &mut impl rand::Rng,
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

        let ModuleBehaviorDef::Assembler(_) = &ctx.def.behavior else {
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
        super::apply_run_result(state, &ctx, outcome, events);
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
    let Some(recipe) = assembler_def.recipes.first() else {
        return super::RunOutcome::Skipped { reset_timer: true };
    };

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

    // Tech gate: if recipe has ship output, require tech_ship_construction
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
            let current_tick = state.meta.tick;
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                Event::ModuleAwaitingTech {
                    station_id: ctx.station_id.clone(),
                    module_id: ctx.module_id.clone(),
                    tech_id: TechId(TECH_SHIP_CONSTRUCTION.to_string()),
                },
            ));
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
                        |i| !matches!(i, InventoryItem::Material { kg, .. } if *kg < MIN_MEANINGFUL_KG),
                    );
                }
                if consumed_kg >= MIN_MEANINGFUL_KG {
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
            OutputSpec::Ship { cargo_capacity_m3 } => {
                let uuid = crate::generate_uuid(rng);
                let ship_id = ShipId(format!("ship_{uuid}"));
                let Some(station) = state.stations.get(&ctx.station_id) else {
                    return;
                };
                let location_node = station.location_node.clone();
                let ship = ShipState {
                    id: ship_id.clone(),
                    location_node: location_node.clone(),
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
                        station_id: ctx.station_id.clone(),
                        ship_id,
                        location_node,
                        cargo_capacity_m3: f64::from(*cargo_capacity_m3),
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
                thermal: None,
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
                        }),
                        wear: WearState::default(),
                        power_stalled: false,
                        thermal: None,
                    }],
                    power: PowerState::default(),
                    cached_inventory_volume_m3: None,
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
                thermal: None,
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
                        }),
                        wear: WearState::default(),
                        power_stalled: false,
                        thermal: None,
                    }],
                    power: PowerState::default(),
                    cached_inventory_volume_m3: None,
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
