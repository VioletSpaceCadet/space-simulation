use sim_core::{
    compute_entity_absolute, inventory_volume_m3, is_co_located, mine_duration, trade,
    travel_ticks, AsteroidId, AsteroidState, Command, CommandEnvelope, CommandId, ComponentId,
    DomainProgress, GameContent, GameState, InputAmount, InputFilter, InventoryItem,
    ModuleBehaviorDef, ModuleKindState, Position, PrincipalId, ShipId, ShipState, SiteId,
    StationState, TaskKind, TechDef, TechId, TradeItemSpec,
};

pub(crate) const AUTOPILOT_OWNER: &str = "principal_autopilot";

/// A single autopilot behavior that generates commands for one concern.
#[allow(dead_code)]
pub(crate) trait AutopilotBehavior: Send {
    fn name(&self) -> &'static str;
    fn generate(
        &self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
    ) -> Vec<CommandEnvelope>;
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Allocates a command ID and builds a `CommandEnvelope`.
fn make_cmd(
    owner: &PrincipalId,
    tick: u64,
    next_id: &mut u64,
    command: Command,
) -> CommandEnvelope {
    let cmd_id = CommandId(*next_id);
    *next_id += 1;
    CommandEnvelope {
        id: cmd_id,
        issued_by: owner.clone(),
        issued_tick: tick,
        execute_at_tick: tick,
        command,
    }
}

/// Wraps `task` in a Transit if `from` and `to` are not co-located; else returns `task` as-is.
fn maybe_transit(
    task: TaskKind,
    from: &Position,
    to: &Position,
    ship_ticks_per_au: u64,
    state: &GameState,
    content: &GameContent,
) -> TaskKind {
    if is_co_located(
        from,
        to,
        &state.body_cache,
        content.constants.docking_range_au_um,
    ) {
        return task;
    }
    let from_abs = compute_entity_absolute(from, &state.body_cache);
    let to_abs = compute_entity_absolute(to, &state.body_cache);
    let ticks = travel_ticks(
        from_abs,
        to_abs,
        ship_ticks_per_au,
        content.constants.min_transit_ticks,
    );
    TaskKind::Transit {
        destination: to.clone(),
        total_ticks: ticks,
        then: Box::new(task),
    }
}

fn maybe_assign_refuel(
    ship: &ShipState,
    ship_id: &ShipId,
    state: &GameState,
    content: &GameContent,
    next_id: &mut u64,
    commands: &mut Vec<CommandEnvelope>,
) {
    if let Some(task_kind) = try_refuel(ship, state, content) {
        commands.push(make_cmd(
            &ship.owner,
            state.meta.tick,
            next_id,
            Command::AssignShipTask {
                ship_id: ship_id.clone(),
                task_kind,
            },
        ));
    }
}

/// Try to issue a Refuel task if ship is at a station with LH2.
fn try_refuel(ship: &ShipState, state: &GameState, content: &GameContent) -> Option<TaskKind> {
    if content.constants.fuel_cost_per_au <= 0.0 {
        return None;
    }
    // Only refuel if below capacity
    if ship.propellant_kg >= ship.propellant_capacity_kg * 0.99 {
        return None;
    }
    // Find co-located station with LH2
    let station = state.stations.values().find(|s| {
        is_co_located(
            &ship.position,
            &s.position,
            &state.body_cache,
            content.constants.docking_range_au_um,
        ) && s.inventory.iter().any(|item| {
            matches!(item, InventoryItem::Material { element, kg, .. }
                if element == "LH2" && *kg > content.constants.min_meaningful_kg)
        })
    })?;
    Some(TaskKind::Refuel {
        station_id: station.id.clone(),
        target_kg: ship.propellant_capacity_kg,
    })
}

/// Returns idle autopilot ships. `BTreeMap` iteration is already sorted by ID.
fn collect_idle_ships(state: &GameState, owner: &PrincipalId) -> Vec<ShipId> {
    state
        .ships
        .values()
        .filter(|ship| {
            ship.owner == *owner
                && ship
                    .task
                    .as_ref()
                    .is_none_or(|t| matches!(t.kind, TaskKind::Idle))
        })
        .map(|ship| ship.id.clone())
        .collect()
}

/// Returns asteroid IDs above confidence threshold with unknown composition,
/// sorted by distance from `reference_pos` (nearest first), with ID tiebreak for determinism.
/// Targets are read from `content.autopilot.deep_scan_targets`.
fn collect_deep_scan_candidates(
    state: &GameState,
    content: &GameContent,
    reference_pos: &Position,
) -> Vec<AsteroidId> {
    if state.asteroids.is_empty() {
        return Vec::new();
    }
    let targets = &content.autopilot.deep_scan_targets;
    let ref_abs = compute_entity_absolute(reference_pos, &state.body_cache);
    let mut candidates: Vec<(AsteroidId, u128)> = state
        .asteroids
        .values()
        .filter(|asteroid| {
            asteroid.knowledge.composition.is_none()
                && asteroid.knowledge.tag_beliefs.iter().any(|(tag, conf)| {
                    targets
                        .iter()
                        .any(|target| tag.0 == target.tag && *conf > target.min_confidence)
                })
        })
        .map(|a| {
            let dist =
                ref_abs.distance_squared(compute_entity_absolute(&a.position, &state.body_cache));
            (a.id.clone(), dist)
        })
        .collect();
    candidates.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0 .0.cmp(&b.0 .0)));
    candidates.into_iter().map(|(id, _)| id).collect()
}

/// Check if any station has a module with the given role installed.
fn station_has_module_with_role(state: &GameState, content: &GameContent, role: &str) -> bool {
    state.stations.values().any(|station| {
        station
            .modules
            .iter()
            .any(|module| content.module_has_role(&module.def_id, role))
    })
}

/// Total inventory of a specific element across all stations.
fn total_element_inventory(state: &GameState, element: &str) -> f32 {
    state
        .stations
        .values()
        .flat_map(|s| s.inventory.iter())
        .filter_map(|item| match item {
            InventoryItem::Material {
                element: el, kg, ..
            } if el == element => Some(*kg),
            _ => None,
        })
        .sum()
}

/// Mining value for sorting: `mass_kg × element_fraction`.
fn element_mining_value(asteroid: &AsteroidState, element: &str) -> f32 {
    asteroid.mass_kg
        * asteroid
            .knowledge
            .composition
            .as_ref()
            .and_then(|c| c.get(element))
            .copied()
            .unwrap_or(0.0)
}

/// Priority 1: if ship has ore, return a Deposit (or Transit→Deposit) task to the nearest station.
fn deposit_priority(
    ship: &ShipState,
    state: &GameState,
    content: &GameContent,
) -> Option<TaskKind> {
    if !ship
        .inventory
        .iter()
        .any(|i| matches!(i, InventoryItem::Ore { .. }))
    {
        return None;
    }
    let ship_abs = compute_entity_absolute(&ship.position, &state.body_cache);
    let station = state.stations.values().min_by_key(|s| {
        let s_abs = compute_entity_absolute(&s.position, &state.body_cache);
        ship_abs.distance_squared(s_abs)
    })?;
    Some(maybe_transit(
        TaskKind::Deposit {
            station: station.id.clone(),
            blocked: false,
        },
        &ship.position,
        &station.position,
        ship.ticks_per_au(content.constants.ticks_per_au),
        state,
        content,
    ))
}

/// Geometric mean of per-domain ratios (accumulated / required), clamped to [0, 1].
fn compute_sufficiency(tech: &TechDef, progress: Option<&DomainProgress>) -> f32 {
    if tech.domain_requirements.is_empty() {
        return 1.0;
    }
    let ratios: Vec<f32> = tech
        .domain_requirements
        .iter()
        .map(|(domain, required)| {
            let accumulated =
                progress.map_or(0.0, |p| p.points.get(domain).copied().unwrap_or(0.0));
            (accumulated / required).min(1.0)
        })
        .collect();
    let product: f32 = ratios.iter().product();
    product.powf(1.0 / ratios.len() as f32)
}

/// Builds the list of export candidates for a station in priority order.
/// Reads component and element export config from `autopilot`.
fn build_export_candidates(
    station: &StationState,
    autopilot: &sim_core::AutopilotConfig,
    batch_size_kg: f32,
) -> Vec<TradeItemSpec> {
    let mut candidates = Vec::new();

    // 1. Export component surplus above reserve
    let export_comp = &autopilot.export_component;
    let comp_count: u32 = station
        .inventory
        .iter()
        .filter_map(|item| match item {
            InventoryItem::Component {
                component_id,
                count,
                ..
            } if component_id.0 == export_comp.component_id => Some(*count),
            _ => None,
        })
        .sum();
    if comp_count > export_comp.reserve {
        candidates.push(TradeItemSpec::Component {
            component_id: ComponentId(export_comp.component_id.clone()),
            count: comp_count - export_comp.reserve,
        });
    }

    // 2+. Materials in priority order from config
    for entry in &autopilot.export_elements {
        let available_kg: f32 = station
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Material {
                    element: el, kg, ..
                } if *el == entry.element => Some(*kg),
                _ => None,
            })
            .sum();
        let surplus_kg = available_kg - entry.reserve_kg;
        if surplus_kg > 0.0 {
            let export_kg = surplus_kg.min(batch_size_kg);
            candidates.push(TradeItemSpec::Material {
                element: entry.element.clone(),
                kg: export_kg,
            });
        }
    }

    candidates
}

// ---------------------------------------------------------------------------
// Behavior implementations
// ---------------------------------------------------------------------------

/// Installs, enables, and configures station modules.
pub(crate) struct StationModuleManager;

impl AutopilotBehavior for StationModuleManager {
    fn name(&self) -> &'static str {
        "station_module_manager"
    }

    fn generate(
        &self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        let mut commands = Vec::new();
        for station in state.stations.values() {
            for item in &station.inventory {
                if let InventoryItem::Module { item_id, .. } = item {
                    commands.push(make_cmd(
                        owner,
                        state.meta.tick,
                        next_id,
                        Command::InstallModule {
                            station_id: station.id.clone(),
                            module_item_id: item_id.clone(),
                        },
                    ));
                }
            }
            for module in &station.modules {
                // Re-enable disabled modules, but not if auto-disabled due to max wear.
                // Skip propellant modules — managed by PropellantPipeline behavior.
                if !module.enabled
                    && module.wear.wear < 1.0
                    && !content.module_has_role(&module.def_id, &content.autopilot.propellant_role)
                {
                    commands.push(make_cmd(
                        owner,
                        state.meta.tick,
                        next_id,
                        Command::SetModuleEnabled {
                            station_id: station.id.clone(),
                            module_id: module.id.clone(),
                            enabled: true,
                        },
                    ));
                }
                if let ModuleKindState::Processor(processor_state) = &module.kind_state {
                    if processor_state.threshold_kg == 0.0 {
                        commands.push(make_cmd(
                            owner,
                            state.meta.tick,
                            next_id,
                            Command::SetModuleThreshold {
                                station_id: station.id.clone(),
                                module_id: module.id.clone(),
                                threshold_kg: content.constants.autopilot_refinery_threshold_kg,
                            },
                        ));
                    }
                }
            }
        }
        commands
    }
}

/// Auto-assigns unassigned labs to the highest-priority eligible tech.
pub(crate) struct LabAssignment;

impl AutopilotBehavior for LabAssignment {
    fn name(&self) -> &'static str {
        "lab_assignment"
    }

    fn generate(
        &self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        let mut commands = Vec::new();

        for station in state.stations.values() {
            for module in &station.modules {
                let ModuleKindState::Lab(lab_state) = &module.kind_state else {
                    continue;
                };
                // Skip labs that are already assigned to an eligible (non-unlocked) tech
                if let Some(ref tech_id) = lab_state.assigned_tech {
                    if !state.research.unlocked.contains(tech_id) {
                        continue;
                    }
                }

                // Find lab's domain from def
                let Some(def) = content.module_defs.get(&module.def_id) else {
                    continue;
                };
                let ModuleBehaviorDef::Lab(lab_def) = &def.behavior else {
                    continue;
                };

                // Find eligible techs that need this lab's domain
                let mut candidates: Vec<(TechId, f32)> = content
                    .techs
                    .iter()
                    .filter(|tech| {
                        !state.research.unlocked.contains(&tech.id)
                            && tech
                                .prereqs
                                .iter()
                                .all(|p| state.research.unlocked.contains(p))
                            && tech.domain_requirements.contains_key(&lab_def.domain)
                    })
                    .map(|tech| {
                        let sufficiency =
                            compute_sufficiency(tech, state.research.evidence.get(&tech.id));
                        (tech.id.clone(), sufficiency)
                    })
                    .collect();
                // Highest sufficiency first (closest to unlock), then by ID for determinism
                candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0 .0.cmp(&b.0 .0)));

                if let Some((tech_id, _)) = candidates.first() {
                    commands.push(make_cmd(
                        owner,
                        state.meta.tick,
                        next_id,
                        Command::AssignLabTech {
                            station_id: station.id.clone(),
                            module_id: module.id.clone(),
                            tech_id: Some(tech_id.clone()),
                        },
                    ));
                }
            }
        }
        commands
    }
}

/// Imports thrusters when a shipyard is ready and conditions are met.
pub(crate) struct ThrusterImport;

impl AutopilotBehavior for ThrusterImport {
    fn name(&self) -> &'static str {
        "thruster_import"
    }

    fn generate(
        &self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        let mut commands = Vec::new();

        // Gate 1: Trade unlock
        if state.meta.tick < sim_core::trade_unlock_tick(&content.constants) {
            return commands;
        }

        // Gate 2: Tech requirement
        let tech_unlocked = state
            .research
            .unlocked
            .contains(&TechId(content.autopilot.ship_construction_tech.clone()));
        if !tech_unlocked {
            return commands;
        }

        let shipyard_role = &content.autopilot.shipyard_role;
        let import_component = &content.autopilot.shipyard_import_component;

        // BTreeMap iteration is already sorted by station ID.
        let sorted_stations: Vec<_> = state.stations.values().collect();

        // Look up the shipyard recipe's component requirement from the first module with the
        // shipyard role. module_defs is AHashMap so still needs sorting for determinism.
        let mut shipyard_defs: Vec<_> = content
            .module_defs
            .values()
            .filter(|def| def.roles.iter().any(|r| r == shipyard_role))
            .collect();
        shipyard_defs.sort_by(|a, b| a.id.cmp(&b.id));
        let required_components = shipyard_defs
            .first()
            .and_then(|def| match &def.behavior {
                ModuleBehaviorDef::Assembler(assembler_def) => assembler_def
                    .recipes
                    .first()
                    .and_then(|recipe_id| content.recipes.get(recipe_id)),
                _ => None,
            })
            .map_or(4u32, |recipe| {
                recipe
                    .inputs
                    .iter()
                    .find_map(|input| match (&input.filter, &input.amount) {
                        (InputFilter::Component(cid), InputAmount::Count(n))
                            if cid.0 == *import_component =>
                        {
                            Some(*n)
                        }
                        _ => None,
                    })
                    .unwrap_or(4)
            });

        for station in sorted_stations {
            // Find a module with the shipyard role — must be enabled
            let has_shipyard = station.modules.iter().any(|module| {
                content.module_has_role(&module.def_id, shipyard_role) && module.enabled
            });
            if !has_shipyard {
                continue;
            }

            // Count current import components in inventory
            let component_count: u32 = station
                .inventory
                .iter()
                .filter_map(|item| match item {
                    InventoryItem::Component {
                        component_id,
                        count,
                        ..
                    } if component_id.0 == *import_component => Some(*count),
                    _ => None,
                })
                .sum();
            if component_count >= required_components {
                continue; // Already have enough for the recipe
            }

            let needed = required_components - component_count;
            let item_spec = TradeItemSpec::Component {
                component_id: ComponentId(import_component.clone()),
                count: needed,
            };

            // Gate 5: Budget cap — cost must be < 5% of current balance
            let Some(cost) = trade::compute_import_cost(&item_spec, &content.pricing, content)
            else {
                continue;
            };
            if cost > state.balance * content.constants.autopilot_budget_cap_fraction {
                continue;
            }

            commands.push(make_cmd(
                owner,
                state.meta.tick,
                next_id,
                Command::Import {
                    station_id: station.id.clone(),
                    item_spec,
                },
            ));
        }
        commands
    }
}

/// Jettisons all slag from stations whose storage usage exceeds the threshold.
pub(crate) struct SlagJettison;

impl AutopilotBehavior for SlagJettison {
    fn name(&self) -> &'static str {
        "slag_jettison"
    }

    fn generate(
        &self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        let mut commands = Vec::new();
        let threshold = content.constants.autopilot_slag_jettison_pct;

        for station in state.stations.values() {
            let used_m3 = inventory_volume_m3(&station.inventory, content);
            let used_pct = used_m3 / station.cargo_capacity_m3;

            if used_pct >= threshold
                && station
                    .inventory
                    .iter()
                    .any(|i| matches!(i, InventoryItem::Slag { .. }))
            {
                commands.push(make_cmd(
                    owner,
                    state.meta.tick,
                    next_id,
                    Command::JettisonSlag {
                        station_id: station.id.clone(),
                    },
                ));
            }
        }
        commands
    }
}

/// Exports surplus materials and components for revenue.
pub(crate) struct MaterialExport;

impl AutopilotBehavior for MaterialExport {
    fn name(&self) -> &'static str {
        "material_export"
    }

    fn generate(
        &self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        let mut commands = Vec::new();

        // Gate: Trade unlock
        if state.meta.tick < sim_core::trade_unlock_tick(&content.constants) {
            return commands;
        }

        let batch_size_kg = content.constants.autopilot_export_batch_size_kg;
        let min_revenue = content.constants.autopilot_export_min_revenue;

        // BTreeMap iteration is already sorted by station ID.
        for station in state.stations.values() {
            // Export candidates in priority order (from autopilot config)
            let candidates = build_export_candidates(station, &content.autopilot, batch_size_kg);

            for candidate in candidates {
                let revenue =
                    match trade::compute_export_revenue(&candidate, &content.pricing, content) {
                        Some(rev) if rev >= min_revenue => rev,
                        _ => continue,
                    };
                // Verify station actually has the items
                if !trade::has_enough_for_export(&station.inventory, &candidate) {
                    continue;
                }
                let _ = revenue; // revenue validated; engine computes final amount
                commands.push(make_cmd(
                    owner,
                    state.meta.tick,
                    next_id,
                    Command::Export {
                        station_id: station.id.clone(),
                        item_spec: candidate,
                    },
                ));
            }
        }
        commands
    }
}

/// Propellant pipeline management — toggles electrolysis based on LH2 levels.
pub(crate) struct PropellantPipeline;

impl AutopilotBehavior for PropellantPipeline {
    fn name(&self) -> &'static str {
        "propellant_pipeline"
    }

    fn generate(
        &self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        let mut commands = Vec::new();
        let propellant_kg = total_element_inventory(state, &content.autopilot.propellant_element);
        let threshold = content.constants.autopilot_lh2_threshold_kg;
        let propellant_role = &content.autopilot.propellant_role;
        let support_role = &content.autopilot.propellant_support_role;

        for station in state.stations.values() {
            let has_propellant_module = station
                .modules
                .iter()
                .any(|m| content.module_has_role(&m.def_id, propellant_role));
            if !has_propellant_module {
                continue;
            }

            if propellant_kg > threshold * content.constants.autopilot_lh2_abundant_multiplier {
                // Propellant abundant — disable propellant modules to save power
                for module in &station.modules {
                    if content.module_has_role(&module.def_id, propellant_role)
                        && module.enabled
                        && module.wear.wear < 1.0
                    {
                        commands.push(make_cmd(
                            owner,
                            state.meta.tick,
                            next_id,
                            Command::SetModuleEnabled {
                                station_id: station.id.clone(),
                                module_id: module.id.clone(),
                                enabled: false,
                            },
                        ));
                    }
                }
            } else if propellant_kg < threshold {
                // Propellant low — ensure propellant and support modules are enabled
                for module in &station.modules {
                    if content.module_has_role(&module.def_id, support_role)
                        && !module.enabled
                        && module.wear.wear < 1.0
                    {
                        commands.push(make_cmd(
                            owner,
                            state.meta.tick,
                            next_id,
                            Command::SetModuleEnabled {
                                station_id: station.id.clone(),
                                module_id: module.id.clone(),
                                enabled: true,
                            },
                        ));
                    }
                }
            }
        }
        commands
    }
}

/// Fits idle ships at stations with modules according to hull fitting templates.
pub(crate) struct ShipFitting;

impl AutopilotBehavior for ShipFitting {
    fn name(&self) -> &'static str {
        "ship_fitting"
    }

    fn generate(
        &self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        let mut commands = Vec::new();
        let idle_ships = collect_idle_ships(state, owner);

        // Track modules consumed this tick to avoid double-allocation
        let mut consumed: Vec<(sim_core::StationId, String)> = Vec::new();

        for ship_id in &idle_ships {
            let Some(ship) = state.ships.get(ship_id) else {
                continue;
            };

            // Ship must be at a station
            let Some(station) = state
                .stations
                .values()
                .find(|s| s.position == ship.position)
            else {
                continue;
            };

            // Look up fitting template for this hull
            let Some(template) = content.fitting_templates.get(&ship.hull_id) else {
                continue;
            };

            for entry in template {
                // Skip if slot already filled
                if ship
                    .fitted_modules
                    .iter()
                    .any(|fm| fm.slot_index == entry.slot_index)
                {
                    continue;
                }

                // Check station inventory for matching module (accounting for already consumed this tick)
                let module_def_id_str = &entry.module_def_id.0;
                let in_inventory = station
                    .inventory
                    .iter()
                    .filter(|item| {
                        matches!(item, InventoryItem::Module { module_def_id, .. } if module_def_id == module_def_id_str)
                    })
                    .count();
                let already_consumed = consumed
                    .iter()
                    .filter(|(sid, mid)| *sid == station.id && mid == module_def_id_str)
                    .count();
                let available = in_inventory > already_consumed;

                if available {
                    consumed.push((station.id.clone(), module_def_id_str.clone()));
                    commands.push(make_cmd(
                        owner,
                        state.meta.tick,
                        next_id,
                        Command::FitShipModule {
                            ship_id: ship_id.clone(),
                            slot_index: entry.slot_index,
                            module_def_id: entry.module_def_id.clone(),
                            station_id: station.id.clone(),
                        },
                    ));
                }
            }
        }
        commands
    }
}

/// Try to assign a mine task from the next best asteroid.
fn try_mine<'a>(
    ship: &ShipState,
    next_mine: &mut impl Iterator<Item = &'a &'a AsteroidState>,
    ship_speed: u64,
    state: &GameState,
    content: &GameContent,
) -> Option<TaskKind> {
    let asteroid = next_mine.next()?;
    Some(maybe_transit(
        TaskKind::Mine {
            asteroid: asteroid.id.clone(),
            duration_ticks: mine_duration(asteroid, ship, content),
        },
        &ship.position,
        &asteroid.position,
        ship_speed,
        state,
        content,
    ))
}

/// Try to assign a deep scan task if tech is unlocked and candidates remain.
fn try_deep_scan<'a>(
    ship: &ShipState,
    deep_scan_unlocked: bool,
    next_deep_scan: &mut impl Iterator<Item = &'a AsteroidId>,
    ship_speed: u64,
    state: &GameState,
    content: &GameContent,
) -> Option<TaskKind> {
    if !deep_scan_unlocked {
        return None;
    }
    let asteroid_id = next_deep_scan.next()?;
    let asteroid_pos = state.asteroids[asteroid_id].position.clone();
    Some(maybe_transit(
        TaskKind::DeepScan {
            asteroid: asteroid_id.clone(),
        },
        &ship.position,
        &asteroid_pos,
        ship_speed,
        state,
        content,
    ))
}

/// Try to assign a survey task from the next nearest unscanned site.
fn try_survey<'a>(
    ship: &ShipState,
    next_site: &mut impl Iterator<Item = &'a &'a sim_core::ScanSite>,
    ship_speed: u64,
    state: &GameState,
    content: &GameContent,
) -> Option<TaskKind> {
    let site = next_site.next()?;
    Some(maybe_transit(
        TaskKind::Survey {
            site: SiteId(site.id.0.clone()),
        },
        &ship.position,
        &site.position,
        ship_speed,
        state,
        content,
    ))
}

/// Ship task scheduling with configurable priority from `content.autopilot.task_priority`.
pub(crate) struct ShipTaskScheduler;

impl AutopilotBehavior for ShipTaskScheduler {
    fn name(&self) -> &'static str {
        "ship_task_scheduler"
    }

    fn generate(
        &self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        let mut commands = Vec::new();

        let idle_ships = collect_idle_ships(state, owner);
        if idle_ships.is_empty() {
            return commands;
        }
        let deep_scan_unlocked = state
            .research
            .unlocked
            .contains(&TechId(content.autopilot.deep_scan_tech.clone()));

        // Use first idle ship's position as reference for distance sorting.
        let reference_pos = &state.ships[&idle_ships[0]].position;

        let deep_scan_candidates = collect_deep_scan_candidates(state, content, reference_pos);
        let mut next_deep_scan = deep_scan_candidates.iter();

        // Sort survey sites by distance from reference position (nearest first).
        let mut sorted_sites: Vec<&sim_core::ScanSite> = state.scan_sites.iter().collect();
        if !sorted_sites.is_empty() {
            let ref_abs = compute_entity_absolute(reference_pos, &state.body_cache);
            sorted_sites.sort_by(|a, b| {
                let da = ref_abs
                    .distance_squared(compute_entity_absolute(&a.position, &state.body_cache));
                let db = ref_abs
                    .distance_squared(compute_entity_absolute(&b.position, &state.body_cache));
                da.cmp(&db).then_with(|| a.id.0.cmp(&b.id.0))
            });
        }
        let mut next_site = sorted_sites.iter();

        // Determine if we need volatile-rich mining (for H2O or propellant pipeline)
        let propellant_role = &content.autopilot.propellant_role;
        let support_role = &content.autopilot.propellant_support_role;
        let has_propellant_module = station_has_module_with_role(state, content, propellant_role);
        let volatile_element = &content.autopilot.volatile_element;
        let propellant_element = &content.autopilot.propellant_element;
        let primary_element = &content.autopilot.primary_mining_element;
        let needs_volatiles = station_has_module_with_role(state, content, support_role)
            && (total_element_inventory(state, volatile_element)
                < content.constants.autopilot_volatile_threshold_kg
                || (has_propellant_module
                    && total_element_inventory(state, propellant_element)
                        < content.constants.autopilot_lh2_threshold_kg));

        let mut mine_candidates: Vec<&AsteroidState> = state
            .asteroids
            .values()
            .filter(|a| a.mass_kg > 0.0 && a.knowledge.composition.is_some())
            .collect();
        if needs_volatiles {
            // Prioritize volatile-rich asteroids when volatile inventory is low
            mine_candidates.sort_by(|a, b| {
                element_mining_value(b, volatile_element)
                    .total_cmp(&element_mining_value(a, volatile_element))
                    .then_with(|| a.id.0.cmp(&b.id.0))
            });
        } else {
            mine_candidates.sort_by(|a, b| {
                element_mining_value(b, primary_element)
                    .total_cmp(&element_mining_value(a, primary_element))
                    .then_with(|| a.id.0.cmp(&b.id.0))
            });
        }
        let mut next_mine = mine_candidates.iter();

        for ship_id in idle_ships {
            let ship = &state.ships[&ship_id];
            let ship_speed = ship.ticks_per_au(content.constants.ticks_per_au);

            // Iterate configurable priority order from content.autopilot.task_priority.
            let mut assigned = false;
            for priority in &content.autopilot.task_priority {
                let task = match priority.as_str() {
                    "Deposit" => deposit_priority(ship, state, content),
                    "Mine" => try_mine(ship, &mut next_mine, ship_speed, state, content),
                    "DeepScan" => try_deep_scan(
                        ship,
                        deep_scan_unlocked,
                        &mut next_deep_scan,
                        ship_speed,
                        state,
                        content,
                    ),
                    "Survey" => try_survey(ship, &mut next_site, ship_speed, state, content),
                    _ => None,
                };
                if let Some(task_kind) = task {
                    commands.push(make_cmd(
                        &ship.owner,
                        state.meta.tick,
                        next_id,
                        Command::AssignShipTask {
                            ship_id: ship_id.clone(),
                            task_kind,
                        },
                    ));
                    assigned = true;
                    break;
                }
            }

            // Fallback: if no task assigned, try refueling at co-located station
            if !assigned {
                maybe_assign_refuel(ship, &ship_id, state, content, next_id, &mut commands);
            }
        }

        commands
    }
}

// ---------------------------------------------------------------------------
// Crew assignment — assign available crew to understaffed modules by priority
// ---------------------------------------------------------------------------

struct CrewAssignment;

impl AutopilotBehavior for CrewAssignment {
    fn name(&self) -> &'static str {
        "crew_assignment"
    }

    fn generate(
        &self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        let mut commands = Vec::new();
        let tick = state.meta.tick;

        for station in state.stations.values() {
            // Sort modules by priority desc, then ID asc
            let mut module_order: Vec<usize> = (0..station.modules.len()).collect();
            module_order.sort_by(|&a, &b| {
                station.modules[b]
                    .module_priority
                    .cmp(&station.modules[a].module_priority)
                    .then_with(|| station.modules[a].id.0.cmp(&station.modules[b].id.0))
            });

            // Track available crew (station.crew minus already-assigned)
            let mut available: std::collections::BTreeMap<sim_core::CrewRole, u32> =
                station.crew.clone();
            for module in &station.modules {
                for (role, &count) in &module.assigned_crew {
                    let entry = available.entry(role.clone()).or_insert(0);
                    *entry = entry.saturating_sub(count);
                }
            }

            for &module_index in &module_order {
                let module = &station.modules[module_index];
                if !module.enabled {
                    continue;
                }
                let Some(def) = content.module_defs.get(&module.def_id) else {
                    continue;
                };
                if def.crew_requirement.is_empty() || module.crew_satisfied {
                    continue;
                }
                // Try to assign missing crew roles
                for (role, &needed) in &def.crew_requirement {
                    let assigned = module.assigned_crew.get(role).copied().unwrap_or(0);
                    if assigned >= needed {
                        continue;
                    }
                    let gap = needed - assigned;
                    let can_assign = available.get(role).copied().unwrap_or(0).min(gap);
                    if can_assign > 0 {
                        commands.push(make_cmd(
                            owner,
                            tick,
                            next_id,
                            Command::AssignCrew {
                                station_id: station.id.clone(),
                                module_id: module.id.clone(),
                                role: role.clone(),
                                count: can_assign,
                            },
                        ));
                        *available.entry(role.clone()).or_insert(0) -= can_assign;
                    }
                }
            }
        }
        commands
    }
}

// ---------------------------------------------------------------------------
// Crew recruitment — import crew when demand exceeds supply
// ---------------------------------------------------------------------------

struct CrewRecruitment;

impl AutopilotBehavior for CrewRecruitment {
    fn name(&self) -> &'static str {
        "crew_recruitment"
    }

    fn generate(
        &self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        let mut commands = Vec::new();
        let tick = state.meta.tick;

        if tick < sim_core::trade_unlock_tick(&content.constants) {
            return commands;
        }

        for station in state.stations.values() {
            // Compute demand: sum of crew_requirement for all enabled modules
            let mut demand: std::collections::BTreeMap<sim_core::CrewRole, u32> =
                std::collections::BTreeMap::new();
            for module in &station.modules {
                if !module.enabled {
                    continue;
                }
                let Some(def) = content.module_defs.get(&module.def_id) else {
                    continue;
                };
                for (role, &count) in &def.crew_requirement {
                    *demand.entry(role.clone()).or_insert(0) += count;
                }
            }

            // Compare demand vs supply, recruit shortfalls
            for (role, needed) in &demand {
                let supply = station.crew.get(role).copied().unwrap_or(0);
                if supply >= *needed {
                    continue;
                }
                let shortfall = needed - supply;
                // Check pricing and balance
                let item_spec = TradeItemSpec::Crew {
                    role: role.clone(),
                    count: shortfall,
                };
                let Some(cost) = trade::compute_import_cost(&item_spec, &content.pricing, content)
                else {
                    continue;
                };
                // Budget guard: only recruit if we can afford it with margin
                let budget_cap = state.balance * content.constants.autopilot_budget_cap_fraction;
                if cost > budget_cap {
                    continue;
                }
                commands.push(make_cmd(
                    owner,
                    tick,
                    next_id,
                    Command::Import {
                        station_id: station.id.clone(),
                        item_spec,
                    },
                ));
            }
        }
        commands
    }
}

/// Creates the default behavior chain in the exact order required for determinism.
pub(crate) fn default_behaviors() -> Vec<Box<dyn AutopilotBehavior>> {
    vec![
        Box::new(StationModuleManager),
        Box::new(LabAssignment),
        Box::new(CrewAssignment),
        Box::new(CrewRecruitment),
        Box::new(ThrusterImport),
        Box::new(SlagJettison),
        Box::new(MaterialExport),
        Box::new(PropellantPipeline),
        Box::new(ShipFitting),
        Box::new(ShipTaskScheduler),
    ]
}

// Test-accessible wrapper that delegates to PropellantPipeline behavior
#[cfg(test)]
pub(crate) fn test_propellant_pipeline_commands(
    state: &GameState,
    content: &GameContent,
    owner: &PrincipalId,
    next_id: &mut u64,
) -> Vec<CommandEnvelope> {
    PropellantPipeline.generate(state, content, owner, next_id)
}

// Test-accessible wrapper for collect_deep_scan_candidates
#[cfg(test)]
pub(crate) fn test_collect_deep_scan_candidates(
    state: &GameState,
    content: &GameContent,
    reference_pos: &Position,
) -> Vec<AsteroidId> {
    collect_deep_scan_candidates(state, content, reference_pos)
}
