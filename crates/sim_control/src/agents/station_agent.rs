use std::collections::{BTreeMap, HashMap};

use sim_core::{
    compute_entity_absolute, inventory_volume_m3, trade, AsteroidId, Command, CommandEnvelope,
    ComponentId, GameContent, GameState, InputAmount, InputFilter, InventoryItem,
    ModuleBehaviorDef, ModuleKindState, PrincipalId, ResearchDomain, ShipId, SiteId, StationId,
    TechId, TradeItemSpec,
};

use crate::behaviors::{
    build_export_candidates, collect_deep_scan_candidates, collect_idle_ships, compute_sufficiency,
    deposit_priority, element_mining_value, make_cmd, should_opportunistic_refuel,
    station_has_module_with_role, total_element_inventory,
};
use crate::objectives::ShipObjective;

use super::ship_agent::ShipAgent;
use super::Agent;

/// Per-station agent that consolidates all station-level concerns into
/// ordered sub-concern methods.
///
/// Execution order: modules → labs → crew → economy (trade-gated) →
/// slag → exports (trade-gated) → propellant → ship fitting.
/// Each sub-concern is a method, not a separate trait object.
///
/// Created per `StationState`; removed when the station is removed from state.
pub(crate) struct StationAgent {
    pub(crate) station_id: StationId,
    pub(crate) lab_cache: LabAssignmentCache,
}

/// Per-station cache for lab assignment decisions.
///
/// Per-station cache for lab assignment. Maps research domains to eligible
/// tech IDs. Rebuilt when the set of unlocked techs changes.
#[derive(Default)]
pub(crate) struct LabAssignmentCache {
    /// domain → eligible tech IDs (prereqs met, not yet unlocked, needs this domain).
    pub(crate) cached_eligible: HashMap<ResearchDomain, Vec<TechId>>,
    /// Number of unlocked techs when cache was last built.
    pub(crate) last_unlocked_count: usize,
    /// Whether the cache has been initialized at all.
    pub(crate) initialized: bool,
}

impl StationAgent {
    pub(crate) fn new(station_id: StationId) -> Self {
        Self {
            station_id,
            lab_cache: LabAssignmentCache::default(),
        }
    }

    // --- Sub-concern methods ---
    // Execution order matches fixed sub-concern ordering for determinism.

    /// 1. Install modules from inventory, re-enable disabled modules (except
    ///    propellant-role and max-wear), set processor thresholds.
    fn manage_modules(
        &mut self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
        commands: &mut Vec<CommandEnvelope>,
    ) {
        let Some(station) = state.stations.get(&self.station_id) else {
            return;
        };

        for item in &station.inventory {
            if let InventoryItem::Module { item_id, .. } = item {
                commands.push(make_cmd(
                    owner,
                    state.meta.tick,
                    next_id,
                    Command::InstallModule {
                        station_id: self.station_id.clone(),
                        module_item_id: item_id.clone(),
                    },
                ));
            }
        }

        // Power-aware module management: shed load during deficit, re-enable during surplus.
        // Only apply when the station has power generation infrastructure (solar/battery).
        let has_power_gen = station.power.generated_kw > 0.0;
        let deficit_kw = station.power.deficit_kw;
        if has_power_gen && deficit_kw > 0.01 {
            // Power deficit: disable least-critical consumers to shed load.
            // Uses power_priority() — None = infrastructure (never shed), lower = shed first.
            let mut shedding_candidates: Vec<(usize, f32, u8)> = station
                .modules
                .iter()
                .enumerate()
                .filter_map(|(index, module)| {
                    if !module.enabled {
                        return None;
                    }
                    let def = content.module_defs.get(&module.def_id)?;
                    let priority = def.power_priority()?; // None = infrastructure, skip
                    if def.power_consumption_per_run <= 0.0 {
                        return None;
                    }
                    // Never shed propellant pipeline modules
                    if content.module_has_role(&module.def_id, &content.autopilot.propellant_role) {
                        return None;
                    }
                    Some((index, def.power_consumption_per_run, priority))
                })
                .collect();
            // Sort ascending: lowest priority number = least critical = shed first
            shedding_candidates.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));

            let mut remaining_deficit = deficit_kw;
            for (index, power_kw, _) in &shedding_candidates {
                if remaining_deficit <= 0.0 {
                    break;
                }
                let module = &station.modules[*index];
                commands.push(make_cmd(
                    owner,
                    state.meta.tick,
                    next_id,
                    Command::SetModuleEnabled {
                        station_id: self.station_id.clone(),
                        module_id: module.id.clone(),
                        enabled: false,
                    },
                ));
                remaining_deficit -= power_kw;
            }
        } else {
            // No deficit (or no power infrastructure): re-enable disabled modules.
            // When power infra exists, respect headroom; otherwise re-enable all.
            let mut available_headroom = station.power.generated_kw - station.power.consumed_kw;
            for module in &station.modules {
                if !module.enabled
                    && module.wear.wear < 1.0
                    && !content.module_has_role(&module.def_id, &content.autopilot.propellant_role)
                {
                    let power_cost = content
                        .module_defs
                        .get(&module.def_id)
                        .map_or(0.0, |d| d.power_consumption_per_run);
                    if !has_power_gen || power_cost <= available_headroom || power_cost <= 0.0 {
                        commands.push(make_cmd(
                            owner,
                            state.meta.tick,
                            next_id,
                            Command::SetModuleEnabled {
                                station_id: self.station_id.clone(),
                                module_id: module.id.clone(),
                                enabled: true,
                            },
                        ));
                        available_headroom -= power_cost;
                    }
                }
            }
        }

        for module in &station.modules {
            if let ModuleKindState::Processor(processor_state) = &module.kind_state {
                if processor_state.threshold_kg == 0.0 {
                    commands.push(make_cmd(
                        owner,
                        state.meta.tick,
                        next_id,
                        Command::SetModuleThreshold {
                            station_id: self.station_id.clone(),
                            module_id: module.id.clone(),
                            threshold_kg: content.constants.autopilot_refinery_threshold_kg,
                        },
                    ));
                }
            }
        }
    }

    /// 2. Assign unassigned labs to the highest-priority eligible tech.
    fn assign_labs(
        &mut self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
        commands: &mut Vec<CommandEnvelope>,
    ) {
        // Rebuild eligible tech cache when unlocked set changes.
        let unlocked_count = state.research.unlocked.len();
        if !self.lab_cache.initialized || unlocked_count != self.lab_cache.last_unlocked_count {
            self.lab_cache.cached_eligible.clear();
            for tech in &content.techs {
                if state.research.unlocked.contains(&tech.id) {
                    continue;
                }
                if !tech
                    .prereqs
                    .iter()
                    .all(|p| state.research.unlocked.contains(p))
                {
                    continue;
                }
                for domain in tech.domain_requirements.keys() {
                    self.lab_cache
                        .cached_eligible
                        .entry(domain.clone())
                        .or_default()
                        .push(tech.id.clone());
                }
            }
            self.lab_cache.last_unlocked_count = unlocked_count;
            self.lab_cache.initialized = true;
        }

        let Some(station) = state.stations.get(&self.station_id) else {
            return;
        };

        for module in &station.modules {
            let ModuleKindState::Lab(lab_state) = &module.kind_state else {
                continue;
            };
            if let Some(ref tech_id) = lab_state.assigned_tech {
                if !state.research.unlocked.contains(tech_id) {
                    continue;
                }
            }

            let Some(def) = content.module_defs.get(&module.def_id) else {
                continue;
            };
            let ModuleBehaviorDef::Lab(lab_def) = &def.behavior else {
                continue;
            };

            let eligible = self
                .lab_cache
                .cached_eligible
                .get(&lab_def.domain)
                .map_or(&[][..], |v| v.as_slice());
            let mut candidates: Vec<(TechId, f32)> = eligible
                .iter()
                .filter(|tid| !state.research.unlocked.contains(tid))
                .filter_map(|tid| {
                    let tech = content.techs.iter().find(|t| t.id == *tid)?;
                    let sufficiency = compute_sufficiency(tech, state.research.evidence.get(tid));
                    Some((tid.clone(), sufficiency))
                })
                .collect();
            candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0 .0.cmp(&b.0 .0)));

            if let Some((tech_id, _)) = candidates.first() {
                commands.push(make_cmd(
                    owner,
                    state.meta.tick,
                    next_id,
                    Command::AssignLabTech {
                        station_id: self.station_id.clone(),
                        module_id: module.id.clone(),
                        tech_id: Some(tech_id.clone()),
                    },
                ));
            }
        }
    }

    /// 3. Assign available crew to understaffed modules by priority.
    fn assign_crew(
        &mut self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
        commands: &mut Vec<CommandEnvelope>,
    ) {
        let Some(station) = state.stations.get(&self.station_id) else {
            return;
        };

        // Early exit: skip if no module needs crew assignment
        let any_unsatisfied = station.modules.iter().any(|m| {
            m.enabled
                && !m.prev_crew_satisfied
                && content
                    .module_defs
                    .get(&m.def_id)
                    .is_some_and(|d| !d.crew_requirement.is_empty())
        });
        if !any_unsatisfied || station.crew.is_empty() {
            return;
        }

        let tick = state.meta.tick;

        let mut module_order: Vec<usize> = (0..station.modules.len()).collect();
        module_order.sort_by(|&a, &b| {
            station.modules[b]
                .module_priority
                .cmp(&station.modules[a].module_priority)
                .then_with(|| station.modules[a].id.0.cmp(&station.modules[b].id.0))
        });

        let mut available: std::collections::BTreeMap<sim_core::CrewRole, u32> =
            station.crew.clone();
        for module in &station.modules {
            for (role, &count) in &module.assigned_crew {
                if let Some(entry) = available.get_mut(role) {
                    *entry = entry.saturating_sub(count);
                }
            }
        }

        for &module_index in &module_order {
            let module = &station.modules[module_index];
            if !module.enabled || module.prev_crew_satisfied {
                continue;
            }
            let Some(def) = content.module_defs.get(&module.def_id) else {
                continue;
            };
            if def.crew_requirement.is_empty() {
                continue;
            }
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
                            station_id: self.station_id.clone(),
                            module_id: module.id.clone(),
                            role: role.clone(),
                            count: can_assign,
                        },
                    ));
                    if let Some(entry) = available.get_mut(role) {
                        *entry -= can_assign;
                    }
                }
            }
        }
    }

    /// 4. Recruit crew when demand exceeds supply.
    fn recruit_crew(
        &mut self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
        commands: &mut Vec<CommandEnvelope>,
    ) {
        let Some(station) = state.stations.get(&self.station_id) else {
            return;
        };

        // Early exit: skip if all modules are crew-satisfied
        let any_unsatisfied = station.modules.iter().any(|m| {
            m.enabled
                && !m.prev_crew_satisfied
                && content
                    .module_defs
                    .get(&m.def_id)
                    .is_some_and(|d| !d.crew_requirement.is_empty())
        });
        if !any_unsatisfied {
            return;
        }

        let tick = state.meta.tick;

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

        for (role, needed) in &demand {
            let supply = station.crew.get(role).copied().unwrap_or(0);
            if supply >= *needed {
                continue;
            }
            let shortfall = needed - supply;
            let item_spec = TradeItemSpec::Crew {
                role: role.clone(),
                count: shortfall,
            };
            let Some(cost) = trade::compute_import_cost(&item_spec, &content.pricing, content)
            else {
                continue;
            };
            let budget_cap = state.balance * content.constants.autopilot_budget_cap_fraction;
            if cost > budget_cap {
                continue;
            }
            // Salary projection: skip if hiring would cause bankruptcy within 30 days
            let hours_per_tick = f64::from(content.constants.minutes_per_tick) / 60.0;
            let projection_ticks = content.constants.game_minutes_to_ticks(30 * 24 * 60);
            let current_salary_per_tick: f64 = state
                .stations
                .values()
                .flat_map(|s| s.crew.iter())
                .map(|(r, &c)| {
                    content
                        .crew_roles
                        .get(r)
                        .map_or(0.0, |d| d.salary_per_hour * f64::from(c) * hours_per_tick)
                })
                .sum();
            let new_hire_salary_per_tick = content.crew_roles.get(role).map_or(0.0, |d| {
                d.salary_per_hour * f64::from(shortfall) * hours_per_tick
            });
            let projected = state.balance
                - cost
                - (current_salary_per_tick + new_hire_salary_per_tick) * projection_ticks as f64;
            if projected < 0.0 {
                continue;
            }
            commands.push(make_cmd(
                owner,
                tick,
                next_id,
                Command::Import {
                    station_id: self.station_id.clone(),
                    item_spec,
                },
            ));
        }
    }

    /// 5. Import thruster components for shipyard.
    fn import_components(
        &mut self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
        commands: &mut Vec<CommandEnvelope>,
    ) {
        let Some(station) = state.stations.get(&self.station_id) else {
            return;
        };

        let tech_unlocked = state
            .research
            .unlocked
            .contains(&TechId(content.autopilot.ship_construction_tech.clone()));
        if !tech_unlocked {
            return;
        }

        let shipyard_role = &content.autopilot.shipyard_role;
        let import_component = &content.autopilot.shipyard_import_component;

        // Find the shipyard recipe's component requirement
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

        let has_shipyard = station
            .modules_with_role(shipyard_role)
            .iter()
            .any(|&idx| station.modules[idx].enabled);
        if !has_shipyard {
            return;
        }

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
            return;
        }

        let needed = required_components - component_count;
        let item_spec = TradeItemSpec::Component {
            component_id: ComponentId(import_component.clone()),
            count: needed,
        };

        let Some(cost) = trade::compute_import_cost(&item_spec, &content.pricing, content) else {
            return;
        };
        if cost > state.balance * content.constants.autopilot_budget_cap_fraction {
            return;
        }

        commands.push(make_cmd(
            owner,
            state.meta.tick,
            next_id,
            Command::Import {
                station_id: self.station_id.clone(),
                item_spec,
            },
        ));
    }

    /// 6. Jettison slag when storage usage exceeds threshold.
    fn jettison_slag(
        &mut self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
        commands: &mut Vec<CommandEnvelope>,
    ) {
        let Some(station) = state.stations.get(&self.station_id) else {
            return;
        };

        let threshold = content.constants.autopilot_slag_jettison_pct;
        if station.cargo_capacity_m3 <= 0.0 {
            return;
        }
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
                    station_id: self.station_id.clone(),
                },
            ));
        }
    }

    /// 7. Export surplus materials for revenue.
    fn export_materials(
        &mut self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
        commands: &mut Vec<CommandEnvelope>,
    ) {
        let Some(station) = state.stations.get(&self.station_id) else {
            return;
        };

        let batch_size_kg = content.constants.autopilot_export_batch_size_kg;
        let min_revenue = content.constants.autopilot_export_min_revenue;

        let candidates = build_export_candidates(station, &content.autopilot, batch_size_kg);
        for candidate in candidates {
            if trade::compute_export_revenue(&candidate, &content.pricing, content)
                .is_none_or(|rev| rev < min_revenue)
            {
                continue;
            }
            if !trade::has_enough_for_export(&station.inventory, &candidate) {
                continue;
            }
            commands.push(make_cmd(
                owner,
                state.meta.tick,
                next_id,
                Command::Export {
                    station_id: self.station_id.clone(),
                    item_spec: candidate,
                },
            ));
        }
    }

    /// 8. Toggle propellant modules based on global LH2 levels (hysteresis).
    pub(crate) fn manage_propellant(
        &mut self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
        commands: &mut Vec<CommandEnvelope>,
    ) {
        let Some(station) = state.stations.get(&self.station_id) else {
            return;
        };

        let propellant_role = &content.autopilot.propellant_role;
        let support_role = &content.autopilot.propellant_support_role;

        if !station.has_role(propellant_role) {
            return;
        }

        let propellant_kg = total_element_inventory(state, &content.autopilot.propellant_element);
        let threshold = content.constants.autopilot_lh2_threshold_kg;

        if propellant_kg > threshold * content.constants.autopilot_lh2_abundant_multiplier {
            for &module_idx in station.modules_with_role(propellant_role) {
                let module = &station.modules[module_idx];
                if module.enabled && module.wear.wear < 1.0 {
                    commands.push(make_cmd(
                        owner,
                        state.meta.tick,
                        next_id,
                        Command::SetModuleEnabled {
                            station_id: self.station_id.clone(),
                            module_id: module.id.clone(),
                            enabled: false,
                        },
                    ));
                }
            }
        } else if propellant_kg < threshold {
            for &module_idx in station.modules_with_role(support_role) {
                let module = &station.modules[module_idx];
                if !module.enabled && module.wear.wear < 1.0 {
                    commands.push(make_cmd(
                        owner,
                        state.meta.tick,
                        next_id,
                        Command::SetModuleEnabled {
                            station_id: self.station_id.clone(),
                            module_id: module.id.clone(),
                            enabled: true,
                        },
                    ));
                }
            }
        }
    }

    /// 9. Fit idle ships at this station with available modules.
    fn fit_ships(
        &mut self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
        commands: &mut Vec<CommandEnvelope>,
    ) {
        let Some(station) = state.stations.get(&self.station_id) else {
            return;
        };

        let idle_ships = collect_idle_ships(state, owner);
        let mut consumed: Vec<String> = Vec::new();

        for ship_id in &idle_ships {
            let Some(ship) = state.ships.get(ship_id) else {
                continue;
            };

            // Ship must be at this station
            if ship.position != station.position {
                continue;
            }

            let Some(template) = content.fitting_templates.get(&ship.hull_id) else {
                continue;
            };

            for entry in template {
                if ship
                    .fitted_modules
                    .iter()
                    .any(|fm| fm.slot_index == entry.slot_index)
                {
                    continue;
                }

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
                    .filter(|mid| *mid == module_def_id_str)
                    .count();
                let available = in_inventory > already_consumed;

                if available {
                    consumed.push(module_def_id_str.clone());
                    commands.push(make_cmd(
                        owner,
                        state.meta.tick,
                        next_id,
                        Command::FitShipModule {
                            ship_id: ship_id.clone(),
                            slot_index: entry.slot_index,
                            module_def_id: entry.module_def_id.clone(),
                            station_id: self.station_id.clone(),
                        },
                    ));
                }
            }
        }
    }

    /// Assign objectives to idle ship agents owned by this station's owner.
    ///
    /// Ships can be at any position — the ship agent will generate Transit
    /// tasks to reach assigned targets. Uses shared-iterator deduplication (AD1)
    /// so no two ships target the same asteroid or scan site.
    ///
    /// Called separately from `generate()` because it mutates ship agents,
    /// not the command buffer.
    pub(crate) fn assign_ship_objectives(
        &self,
        ship_agents: &mut BTreeMap<ShipId, ShipAgent>,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
    ) {
        let Some(station) = state.stations.get(&self.station_id) else {
            return;
        };

        // Collect idle ships owned by this station's owner with no current objective.
        // Ships can be at any position — the ship agent will generate Transit
        // tasks to reach assigned targets (fixes VIO-457: ships stranded after
        // completing tasks at remote locations).
        let idle_ships = collect_idle_ships(state, owner);
        let assignable: Vec<ShipId> = idle_ships
            .into_iter()
            .filter(|id| ship_agents.get(id).is_some_and(|a| a.objective.is_none()))
            .collect();

        if assignable.is_empty() {
            return;
        }

        let reference_pos = &station.position;

        let deep_scan_unlocked = state
            .research
            .unlocked
            .contains(&TechId(content.autopilot.deep_scan_tech.clone()));

        // Pre-compute sorted candidate lists (Schwartzian transforms)
        let deep_scan_candidates = collect_deep_scan_candidates(state, content, reference_pos);
        let survey_candidates = collect_survey_candidates(state, reference_pos);
        let mine_candidates = collect_mine_candidates(state, content);

        let mut next_deep_scan = deep_scan_candidates.iter();
        let mut next_site = survey_candidates.iter();
        let mut next_mine = mine_candidates.iter();

        // Assign objectives using shared iterators (AD1)
        for ship_id in assignable {
            let Some(ship) = state.ships.get(&ship_id) else {
                continue;
            };

            if should_opportunistic_refuel(ship, state, content) {
                continue;
            }

            if deposit_priority(ship, state, content).is_some() {
                continue;
            }

            for priority in &content.autopilot.task_priority {
                let objective = match priority.as_str() {
                    "Mine" => next_mine.next().map(|id| ShipObjective::Mine {
                        asteroid_id: id.clone(),
                    }),
                    "DeepScan" if deep_scan_unlocked => {
                        next_deep_scan.next().map(|id| ShipObjective::DeepScan {
                            asteroid_id: id.clone(),
                        })
                    }
                    "Survey" => next_site.next().map(|id| ShipObjective::Survey {
                        site_id: id.clone(),
                    }),
                    _ => None,
                };
                if let Some(obj) = objective {
                    if let Some(agent) = ship_agents.get_mut(&ship_id) {
                        agent.objective = Some(obj);
                    }
                    break;
                }
            }
        }
    }
}

impl Agent for StationAgent {
    fn name(&self) -> &'static str {
        "station_agent"
    }

    fn generate(
        &mut self,
        state: &GameState,
        content: &GameContent,
        owner: &PrincipalId,
        next_id: &mut u64,
    ) -> Vec<CommandEnvelope> {
        if !state.stations.contains_key(&self.station_id) {
            return Vec::new();
        }

        let mut commands = Vec::new();
        let trade_unlocked = state.meta.tick >= sim_core::trade_unlock_tick(&content.constants);

        // Sub-concerns in order matching default_behaviors() (AD5)
        self.manage_modules(state, content, owner, next_id, &mut commands);
        self.assign_labs(state, content, owner, next_id, &mut commands);
        self.assign_crew(state, content, owner, next_id, &mut commands);

        // Economy methods gated by trade unlock
        if trade_unlocked {
            self.recruit_crew(state, content, owner, next_id, &mut commands);
            self.import_components(state, content, owner, next_id, &mut commands);
        }

        self.jettison_slag(state, content, owner, next_id, &mut commands);

        if trade_unlocked {
            self.export_materials(state, content, owner, next_id, &mut commands);
        }

        self.manage_propellant(state, content, owner, next_id, &mut commands);
        self.fit_ships(state, content, owner, next_id, &mut commands);
        // Ship assignment is called separately via assign_ship_objectives()
        // because it mutates ship agents, not the command buffer.

        commands
    }
}

/// Survey sites sorted by distance from reference position (nearest first).
fn collect_survey_candidates(state: &GameState, reference_pos: &sim_core::Position) -> Vec<SiteId> {
    if state.scan_sites.is_empty() {
        return Vec::new();
    }
    let ref_abs = compute_entity_absolute(reference_pos, &state.body_cache);
    let mut decorated: Vec<(u128, SiteId)> = state
        .scan_sites
        .iter()
        .map(|site| {
            let dist = ref_abs
                .distance_squared(compute_entity_absolute(&site.position, &state.body_cache));
            (dist, site.id.clone())
        })
        .collect();
    decorated.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1 .0.cmp(&b.1 .0)));
    decorated.into_iter().map(|(_, id)| id).collect()
}

/// Mine candidates sorted by mining value (mass * element fraction), descending.
/// Volatile detection determines which element to prioritize.
fn collect_mine_candidates(state: &GameState, content: &GameContent) -> Vec<AsteroidId> {
    let propellant_role = &content.autopilot.propellant_role;
    let support_role = &content.autopilot.propellant_support_role;
    let has_propellant_module = station_has_module_with_role(state, propellant_role);
    let volatile_element = &content.autopilot.volatile_element;
    let propellant_element = &content.autopilot.propellant_element;
    let primary_element = &content.autopilot.primary_mining_element;
    let needs_volatiles = station_has_module_with_role(state, support_role)
        && (total_element_inventory(state, volatile_element)
            < content.constants.autopilot_volatile_threshold_kg
            || (has_propellant_module
                && total_element_inventory(state, propellant_element)
                    < content.constants.autopilot_lh2_threshold_kg));

    let sort_element = if needs_volatiles {
        volatile_element
    } else {
        primary_element
    };
    let mut decorated: Vec<(f32, AsteroidId)> = state
        .asteroids
        .values()
        .filter(|a| a.mass_kg > 0.0 && a.knowledge.composition.is_some())
        .map(|a| (element_mining_value(a, sort_element), a.id.clone()))
        .collect();
    decorated.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1 .0.cmp(&b.1 .0)));
    decorated.into_iter().map(|(_, id)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::test_fixtures::{base_content, base_state};

    #[test]
    fn new_agent_has_empty_lab_cache() {
        let agent = StationAgent::new(StationId("test_station".to_string()));
        assert_eq!(agent.station_id, StationId("test_station".to_string()));
        assert!(!agent.lab_cache.initialized);
        assert_eq!(agent.lab_cache.last_unlocked_count, 0);
        assert!(agent.lab_cache.cached_eligible.is_empty());
    }

    #[test]
    fn base_state_produces_no_commands() {
        let content = base_content();
        let state = base_state(&content);
        let owner = PrincipalId("principal_autopilot".to_string());
        let station_id = state.stations.keys().next().unwrap().clone();
        let mut agent = StationAgent::new(station_id);
        let mut next_id = 1;

        let commands = agent.generate(&state, &content, &owner, &mut next_id);
        // base_state has no modules in inventory, no disabled modules, no labs, etc.
        assert!(commands.is_empty());
    }

    #[test]
    fn missing_station_produces_no_commands() {
        let content = base_content();
        let state = base_state(&content);
        let owner = PrincipalId("principal_autopilot".to_string());
        let mut agent = StationAgent::new(StationId("nonexistent".to_string()));
        let mut next_id = 1;

        let commands = agent.generate(&state, &content, &owner, &mut next_id);
        assert!(commands.is_empty());
    }

    #[test]
    fn manage_modules_installs_from_inventory() {
        let content = base_content();
        let mut state = base_state(&content);
        let owner = PrincipalId("principal_autopilot".to_string());
        let station_id = state.stations.keys().next().unwrap().clone();

        // Add a module to station inventory
        state
            .stations
            .get_mut(&station_id)
            .unwrap()
            .inventory
            .push(InventoryItem::Module {
                item_id: sim_core::ModuleItemId("item_1".to_string()),
                module_def_id: "mod_def_test".to_string(),
            });

        let mut agent = StationAgent::new(station_id.clone());
        let mut next_id = 1;
        let mut commands = Vec::new();

        agent.manage_modules(&state, &content, &owner, &mut next_id, &mut commands);

        assert_eq!(commands.len(), 1);
        assert!(matches!(
            &commands[0].command,
            Command::InstallModule {
                station_id: sid,
                ..
            } if *sid == station_id
        ));
    }

    #[test]
    fn jettison_slag_fires_above_threshold() {
        let mut content = base_content();
        content.constants.autopilot_slag_jettison_pct = 0.5;
        let mut state = base_state(&content);
        let owner = PrincipalId("principal_autopilot".to_string());
        let station_id = state.stations.keys().next().unwrap().clone();

        // Fill station above threshold with slag — use tiny capacity so volume ratio is high
        let station = state.stations.get_mut(&station_id).unwrap();
        station.cargo_capacity_m3 = 0.001;
        station.inventory.push(InventoryItem::Slag {
            kg: 100.0,
            composition: std::collections::HashMap::new(),
        });
        station.cached_inventory_volume_m3 = None;

        let mut agent = StationAgent::new(station_id.clone());
        let mut next_id = 1;
        let mut commands = Vec::new();

        agent.jettison_slag(&state, &content, &owner, &mut next_id, &mut commands);

        assert_eq!(commands.len(), 1);
        assert!(matches!(
            &commands[0].command,
            Command::JettisonSlag { station_id: sid } if *sid == station_id
        ));
    }

    #[test]
    fn recruit_crew_skips_when_salary_would_bankrupt() {
        use sim_core::test_fixtures::ModuleDefBuilder;

        let mut content = base_content();
        let role = sim_core::CrewRole("engineer".to_string());
        content.crew_roles.insert(
            role.clone(),
            sim_core::CrewRoleDef {
                id: role.clone(),
                name: "Engineer".to_string(),
                recruitment_cost: 100.0,
                salary_per_hour: 1_000_000.0, // Absurdly high → guarantees bankruptcy
            },
        );
        content.constants.trade_unlock_delay_minutes = 0;
        content.pricing.items.insert(
            "engineer".to_string(),
            sim_core::PricingEntry {
                base_price_per_unit: 10.0,
                importable: true,
                exportable: false,
                category: String::new(),
            },
        );
        // Module def requiring an engineer
        let mut mod_def = ModuleDefBuilder::new("mod_crew_test")
            .behavior(sim_core::ModuleBehaviorDef::Equipment)
            .build();
        mod_def.crew_requirement.insert(role.clone(), 1);
        content
            .module_defs
            .insert("mod_crew_test".to_string(), mod_def);

        let mut state = base_state(&content);
        state.balance = 1000.0;
        let owner = PrincipalId("principal_autopilot".to_string());
        let station_id = state.stations.keys().next().unwrap().clone();

        let station = state.stations.get_mut(&station_id).unwrap();
        station.modules.push(sim_core::ModuleState {
            id: sim_core::ModuleInstanceId("mod_1".to_string()),
            def_id: "mod_crew_test".to_string(),
            enabled: true,
            kind_state: sim_core::ModuleKindState::Equipment,
            wear: sim_core::WearState::default(),
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
            thermal: None,
        });
        station.rebuild_module_index(&content);

        let mut agent = StationAgent::new(station_id);
        let mut next_id = 1;
        let mut commands = Vec::new();

        agent.recruit_crew(&state, &content, &owner, &mut next_id, &mut commands);

        // Should produce NO import command because salary projection shows bankruptcy
        assert!(
            commands.is_empty(),
            "should skip recruitment when salary would bankrupt: got {commands:?}"
        );
    }

    // --- Ship assignment tests (ported from ShipAssignmentBridge) ---

    use sim_core::test_fixtures::test_position;
    use sim_core::{AsteroidKnowledge, AsteroidState, HullId, LotId, TaskKind, TaskState};

    fn test_owner() -> PrincipalId {
        PrincipalId("principal_autopilot".to_string())
    }

    fn make_ship_id(name: &str) -> ShipId {
        ShipId(name.to_string())
    }

    fn make_asteroid_id(name: &str) -> AsteroidId {
        AsteroidId(name.to_string())
    }

    fn assignment_setup() -> (GameState, GameContent, BTreeMap<ShipId, ShipAgent>) {
        let content = base_content();
        let state = base_state(&content);
        let agents = BTreeMap::new();
        (state, content, agents)
    }

    fn add_idle_ship(
        state: &mut GameState,
        agents: &mut BTreeMap<ShipId, ShipAgent>,
        ship_id: ShipId,
    ) {
        use sim_core::ShipState;
        let ship = ShipState {
            id: ship_id.clone(),
            owner: test_owner(),
            position: test_position(),
            inventory: vec![],
            task: None,
            hull_id: HullId("hull_general_purpose".to_string()),
            fitted_modules: vec![],
            modifiers: Default::default(),
            propellant_kg: 0.0,
            propellant_capacity_kg: 0.0,
            cargo_capacity_m3: 100.0,
            speed_ticks_per_au: None,
            crew: std::collections::BTreeMap::new(),
            leaders: vec![],
        };
        state.ships.insert(ship_id.clone(), ship);
        agents.insert(ship_id.clone(), ShipAgent::new(ship_id));
    }

    fn add_mineable_asteroid(state: &mut GameState, asteroid_id: AsteroidId, fe_fraction: f32) {
        state.asteroids.insert(
            asteroid_id.clone(),
            AsteroidState {
                id: asteroid_id,
                position: test_position(),
                true_composition: std::collections::HashMap::new(),
                anomaly_tags: vec![],
                mass_kg: 1000.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![],
                    composition: Some({
                        let mut composition = std::collections::HashMap::new();
                        composition.insert("Fe".to_string(), fe_fraction);
                        composition
                    }),
                },
            },
        );
    }

    fn station_id_from_state(state: &GameState) -> StationId {
        state.stations.keys().next().unwrap().clone()
    }

    #[test]
    fn assign_no_idle_ships_no_assignments() {
        let (state, content, mut ship_agents) = assignment_setup();
        let owner = test_owner();
        let station_id = station_id_from_state(&state);
        let agent = StationAgent::new(station_id);

        agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner);

        assert!(ship_agents.is_empty());
    }

    #[test]
    fn assign_two_ships_two_asteroids_no_double_assignment() {
        let (mut state, content, mut ship_agents) = assignment_setup();
        let owner = test_owner();
        let station_id = station_id_from_state(&state);

        let ship_a = make_ship_id("ship_a");
        let ship_b = make_ship_id("ship_b");
        add_idle_ship(&mut state, &mut ship_agents, ship_a.clone());
        add_idle_ship(&mut state, &mut ship_agents, ship_b.clone());

        let asteroid_1 = make_asteroid_id("asteroid_1");
        let asteroid_2 = make_asteroid_id("asteroid_2");
        add_mineable_asteroid(&mut state, asteroid_1.clone(), 0.8);
        add_mineable_asteroid(&mut state, asteroid_2.clone(), 0.5);

        let agent = StationAgent::new(station_id);
        agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner);

        let obj_a = ship_agents[&ship_a]
            .objective
            .as_ref()
            .expect("ship_a should have objective");
        let obj_b = ship_agents[&ship_b]
            .objective
            .as_ref()
            .expect("ship_b should have objective");

        let id_a = match obj_a {
            ShipObjective::Mine { asteroid_id } => asteroid_id.clone(),
            other => panic!("expected Mine, got {other:?}"),
        };
        let id_b = match obj_b {
            ShipObjective::Mine { asteroid_id } => asteroid_id.clone(),
            other => panic!("expected Mine, got {other:?}"),
        };

        assert_ne!(id_a, id_b);
        assert_eq!(id_a, asteroid_1);
        assert_eq!(id_b, asteroid_2);
    }

    #[test]
    fn assign_ship_with_cargo_skipped_no_iterator_consumption() {
        let (mut state, content, mut ship_agents) = assignment_setup();
        let owner = test_owner();
        let station_id = station_id_from_state(&state);

        let ship_a = make_ship_id("ship_a");
        let ship_b = make_ship_id("ship_b");
        add_idle_ship(&mut state, &mut ship_agents, ship_a.clone());
        add_idle_ship(&mut state, &mut ship_agents, ship_b.clone());

        let asteroid_1 = make_asteroid_id("asteroid_1");
        add_mineable_asteroid(&mut state, asteroid_1.clone(), 0.8);

        // Give ship_a cargo so deposit_priority fires → skipped
        state
            .ships
            .get_mut(&ship_a)
            .unwrap()
            .inventory
            .push(InventoryItem::Ore {
                lot_id: LotId("lot_1".to_string()),
                asteroid_id: make_asteroid_id("some_asteroid"),
                kg: 50.0,
                composition: {
                    let mut composition = std::collections::HashMap::new();
                    composition.insert("Fe".to_string(), 0.8_f32);
                    composition
                },
            });

        let agent = StationAgent::new(station_id);
        agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner);

        assert!(ship_agents[&ship_a].objective.is_none());
        assert!(matches!(
            ship_agents[&ship_b].objective,
            Some(ShipObjective::Mine { ref asteroid_id }) if *asteroid_id == asteroid_1
        ));
    }

    #[test]
    fn assign_busy_ship_not_assigned() {
        let (mut state, content, mut ship_agents) = assignment_setup();
        let owner = test_owner();
        let station_id = station_id_from_state(&state);

        let ship_id = make_ship_id("ship_a");
        add_idle_ship(&mut state, &mut ship_agents, ship_id.clone());

        state.ships.get_mut(&ship_id).unwrap().task = Some(TaskState {
            kind: TaskKind::Mine {
                asteroid: make_asteroid_id("asteroid_x"),
                duration_ticks: 10,
            },
            started_tick: 0,
            eta_tick: 10,
        });

        add_mineable_asteroid(&mut state, make_asteroid_id("asteroid_1"), 0.8);

        let agent = StationAgent::new(station_id);
        agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner);

        assert!(ship_agents[&ship_id].objective.is_none());
    }

    #[test]
    fn assign_deep_scan_when_tech_unlocked() {
        let (mut state, mut content, mut ship_agents) = assignment_setup();
        let owner = test_owner();
        let station_id = station_id_from_state(&state);

        let ship_id = make_ship_id("ship_a");
        add_idle_ship(&mut state, &mut ship_agents, ship_id.clone());
        state.scan_sites.clear();

        let tech_id = TechId("tech_deep_scan".to_string());
        content.autopilot.deep_scan_tech = "tech_deep_scan".to_string();
        content.autopilot.deep_scan_targets = vec![sim_core::DeepScanTargetConfig {
            tag: "IronRich".to_string(),
            min_confidence: 0.5,
        }];
        state.research.unlocked.insert(tech_id);

        let asteroid_id = make_asteroid_id("asteroid_scan");
        state.asteroids.insert(
            asteroid_id.clone(),
            AsteroidState {
                id: asteroid_id.clone(),
                position: test_position(),
                true_composition: std::collections::HashMap::new(),
                anomaly_tags: vec![],
                mass_kg: 500.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![(sim_core::AnomalyTag("IronRich".to_string()), 0.9)],
                    composition: None,
                },
            },
        );

        let agent = StationAgent::new(station_id);
        agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner);

        assert!(matches!(
            ship_agents[&ship_id].objective,
            Some(ShipObjective::DeepScan { ref asteroid_id }) if asteroid_id.0 == "asteroid_scan"
        ));
    }

    #[test]
    fn assign_deep_scan_skipped_without_tech() {
        let (mut state, mut content, mut ship_agents) = assignment_setup();
        let owner = test_owner();
        let station_id = station_id_from_state(&state);

        let ship_id = make_ship_id("ship_a");
        add_idle_ship(&mut state, &mut ship_agents, ship_id.clone());
        state.scan_sites.clear();

        content.autopilot.deep_scan_tech = "tech_deep_scan".to_string();
        content.autopilot.deep_scan_targets = vec![sim_core::DeepScanTargetConfig {
            tag: "IronRich".to_string(),
            min_confidence: 0.5,
        }];

        state.asteroids.insert(
            make_asteroid_id("asteroid_scan"),
            AsteroidState {
                id: make_asteroid_id("asteroid_scan"),
                position: test_position(),
                true_composition: std::collections::HashMap::new(),
                anomaly_tags: vec![],
                mass_kg: 500.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![(sim_core::AnomalyTag("IronRich".to_string()), 0.9)],
                    composition: None,
                },
            },
        );

        let agent = StationAgent::new(station_id);
        agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner);

        assert!(ship_agents[&ship_id].objective.is_none());
    }

    #[test]
    fn assign_ship_not_at_station_still_assigned() {
        let (mut state, content, mut ship_agents) = assignment_setup();
        let owner = test_owner();
        let station_id = station_id_from_state(&state);

        let ship_id = make_ship_id("ship_a");
        add_idle_ship(&mut state, &mut ship_agents, ship_id.clone());

        // Move ship to a different position than the station (simulates
        // completing a task at a remote location — VIO-457 regression test)
        let mut different_pos = test_position();
        different_pos.radius_au_um = sim_core::RadiusAuMicro(999_999);
        state.ships.get_mut(&ship_id).unwrap().position = different_pos;

        add_mineable_asteroid(&mut state, make_asteroid_id("asteroid_1"), 0.8);

        let agent = StationAgent::new(station_id);
        agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner);

        // Ship at remote position still gets assigned (ship agent handles transit)
        assert!(ship_agents[&ship_id].objective.is_some());
    }

    #[test]
    fn assign_three_ships_waterfall_mine_then_survey() {
        let (mut state, content, mut ship_agents) = assignment_setup();
        let owner = test_owner();
        let station_id = station_id_from_state(&state);

        let ship_a = make_ship_id("ship_a");
        let ship_b = make_ship_id("ship_b");
        let ship_c = make_ship_id("ship_c");
        add_idle_ship(&mut state, &mut ship_agents, ship_a.clone());
        add_idle_ship(&mut state, &mut ship_agents, ship_b.clone());
        add_idle_ship(&mut state, &mut ship_agents, ship_c.clone());

        state.scan_sites.clear();
        add_mineable_asteroid(&mut state, make_asteroid_id("asteroid_1"), 0.8);

        let site_id = sim_core::SiteId("site_1".to_string());
        state.scan_sites.push(sim_core::ScanSite {
            id: site_id.clone(),
            position: test_position(),
            template_id: "template_default".to_string(),
        });

        let agent = StationAgent::new(station_id);
        agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner);

        assert!(matches!(
            ship_agents[&ship_a].objective,
            Some(ShipObjective::Mine { .. })
        ));
        assert!(matches!(
            ship_agents[&ship_b].objective,
            Some(ShipObjective::Survey { .. })
        ));
        // ship_c: all candidates consumed
        assert!(ship_agents[&ship_c].objective.is_none());
    }

    #[test]
    fn assign_existing_objective_not_overwritten() {
        let (mut state, content, mut ship_agents) = assignment_setup();
        let owner = test_owner();
        let station_id = station_id_from_state(&state);

        let ship_id = make_ship_id("ship_a");
        add_idle_ship(&mut state, &mut ship_agents, ship_id.clone());
        add_mineable_asteroid(&mut state, make_asteroid_id("asteroid_1"), 0.8);

        // Pre-set an objective
        ship_agents.get_mut(&ship_id).unwrap().objective = Some(ShipObjective::DeepScan {
            asteroid_id: make_asteroid_id("other"),
        });

        let agent = StationAgent::new(station_id);
        agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner);

        assert!(matches!(
            ship_agents[&ship_id].objective,
            Some(ShipObjective::DeepScan { .. })
        ));
    }

    #[test]
    fn assign_survey_when_no_mine_candidates() {
        let (mut state, content, mut ship_agents) = assignment_setup();
        let owner = test_owner();
        let station_id = station_id_from_state(&state);

        let ship_id = make_ship_id("ship_a");
        add_idle_ship(&mut state, &mut ship_agents, ship_id.clone());

        state.scan_sites.clear();
        state.scan_sites.push(sim_core::ScanSite {
            id: sim_core::SiteId("site_1".to_string()),
            position: test_position(),
            template_id: "template_default".to_string(),
        });

        let agent = StationAgent::new(station_id);
        agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner);

        assert!(matches!(
            ship_agents[&ship_id].objective,
            Some(ShipObjective::Survey { ref site_id }) if site_id.0 == "site_1"
        ));
    }

    #[test]
    fn assign_no_candidates_no_objective() {
        let (mut state, content, mut ship_agents) = assignment_setup();
        let owner = test_owner();
        let station_id = station_id_from_state(&state);

        let ship_id = make_ship_id("ship_a");
        add_idle_ship(&mut state, &mut ship_agents, ship_id.clone());

        state.scan_sites.clear();

        let agent = StationAgent::new(station_id);
        agent.assign_ship_objectives(&mut ship_agents, &state, &content, &owner);

        assert!(ship_agents[&ship_id].objective.is_none());
    }

    #[test]
    fn manage_modules_sheds_load_during_power_deficit() {
        use sim_core::test_fixtures::ModuleDefBuilder;

        let mut content = base_content();
        // Two modules with different power priorities.
        // Lower number = less critical = shed first (matching sim_core convention).
        content.module_defs.insert(
            "module_least_critical".to_string(),
            ModuleDefBuilder::new("module_least_critical")
                .name("Least Critical")
                .mass(100.0)
                .volume(1.0)
                .power(20.0)
                .power_stall_priority(0) // lowest = shed first
                .behavior(sim_core::ModuleBehaviorDef::Assembler(
                    sim_core::AssemblerDef {
                        assembly_interval_minutes: 1,
                        assembly_interval_ticks: 1,
                        max_stock: HashMap::new(),
                        recipes: vec![],
                    },
                ))
                .build(),
        );
        content.module_defs.insert(
            "module_most_critical".to_string(),
            ModuleDefBuilder::new("module_most_critical")
                .name("Most Critical")
                .mass(100.0)
                .volume(1.0)
                .power(15.0)
                .power_stall_priority(4) // highest = shed last
                .behavior(sim_core::ModuleBehaviorDef::Assembler(
                    sim_core::AssemblerDef {
                        assembly_interval_minutes: 1,
                        assembly_interval_ticks: 1,
                        max_stock: HashMap::new(),
                        recipes: vec![],
                    },
                ))
                .build(),
        );

        let mut state = base_state(&content);
        let owner = PrincipalId("principal_autopilot".to_string());
        let station_id = state.stations.keys().next().unwrap().clone();

        // Install the two modules
        let station = state.stations.get_mut(&station_id).unwrap();
        station.modules.push(sim_core::ModuleState {
            id: sim_core::ModuleInstanceId("mod_least_critical".to_string()),
            def_id: "module_least_critical".to_string(),
            enabled: true,
            kind_state: sim_core::ModuleKindState::Assembler(sim_core::AssemblerState {
                ticks_since_last_run: 0,
                stalled: false,
                capped: false,
                cap_override: HashMap::new(),
                selected_recipe: None,
            }),
            wear: sim_core::WearState::default(),
            thermal: None,
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
        });
        station.modules.push(sim_core::ModuleState {
            id: sim_core::ModuleInstanceId("mod_most_critical".to_string()),
            def_id: "module_most_critical".to_string(),
            enabled: true,
            kind_state: sim_core::ModuleKindState::Assembler(sim_core::AssemblerState {
                ticks_since_last_run: 0,
                stalled: false,
                capped: false,
                cap_override: HashMap::new(),
                selected_recipe: None,
            }),
            wear: sim_core::WearState::default(),
            thermal: None,
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
        });

        // Set power state with deficit: 30kW gen, 50kW consumed = 20kW deficit
        station.power = sim_core::PowerState {
            generated_kw: 30.0,
            consumed_kw: 50.0,
            deficit_kw: 20.0,
            ..Default::default()
        };
        station.rebuild_module_index(&content);

        let mut agent = StationAgent::new(station_id);
        let mut next_id = 0u64;
        let mut commands = Vec::new();

        agent.manage_modules(&state, &content, &owner, &mut next_id, &mut commands);

        // Should disable the least-critical module (stall_priority=0, shed first)
        // 20kW power consumption >= 20kW deficit, so only one module needs shedding
        let disable_cmds: Vec<_> = commands
            .iter()
            .filter(|c| matches!(&c.command, Command::SetModuleEnabled { enabled: false, .. }))
            .collect();
        assert!(
            !disable_cmds.is_empty(),
            "should disable at least one module during deficit"
        );
        // The least-critical module (stall_priority=0) should be disabled first
        let first_disabled = match &disable_cmds[0].command {
            Command::SetModuleEnabled { module_id, .. } => module_id.0.clone(),
            _ => panic!("expected SetModuleEnabled"),
        };
        assert_eq!(
            first_disabled, "mod_least_critical",
            "least-critical module (lowest priority number) should be shed first"
        );
    }
}
