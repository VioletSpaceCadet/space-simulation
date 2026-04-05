use crate::instrumentation::{timed, TickTimings};
use crate::research::advance_research;
use crate::satellite::tick_satellites;
use crate::station::{tick_ground_facilities, tick_stations};
use crate::tasks::resolve_task;
use crate::{Command, CommandEnvelope, GameContent, GameState, ScanSite, ShipId, SiteId, TaskKind};
use rand::Rng;

/// Trade (import/export) unlocks after `trade_unlock_delay_minutes` game-minutes.
pub fn trade_unlock_tick(constants: &crate::Constants) -> u64 {
    constants.game_minutes_to_ticks(constants.trade_unlock_delay_minutes)
}

/// Advance the simulation by one tick.
///
/// Order of operations:
/// 1. Apply commands scheduled for this tick.
/// 2. Resolve ship tasks whose eta has arrived.
/// 3. Tick station modules (processors, assemblers, sensors, labs, maintenance, thermal, boiloff).
///    3.5. Tick ground facility modules (same pipeline via proxy-station pattern).
///    3.6. Tick satellites (survey discovery, science data, zone effect caches).
/// 4. Advance research on all eligible techs.
///    4.5. Evaluate milestones (content-driven progression).
///    4.6. Evaluate sim events (content-driven random events).
/// 5. Replenish scan sites if below threshold.
/// 6. Increment tick counter.
///
/// Returns all events produced this tick.
pub fn tick(
    state: &mut GameState,
    commands: &[CommandEnvelope],
    content: &GameContent,
    rng: &mut impl Rng,
    mut timings: Option<&mut TickTimings>,
) -> Vec<crate::EventEnvelope> {
    let mut events = Vec::new();

    timed!(
        timings,
        apply_commands,
        apply_commands(state, commands, content, rng, &mut events)
    );
    deduct_crew_salaries(state, content, &mut events);
    // Ongoing tasks (Refuel) run every tick, before scheduled task resolution.
    timed!(
        timings,
        resolve_ship_tasks,
        crate::tasks::resolve_refuels(state, content, &mut events)
    );
    timed!(
        timings,
        resolve_ship_tasks,
        resolve_ship_tasks(state, content, rng, &mut events)
    );
    timed!(
        timings,
        tick_stations,
        tick_stations(state, content, rng, &mut events, timings.as_deref_mut())
    );
    // Deduct per-module operating costs for ground facilities.
    deduct_operating_costs(state, content, &mut events);
    timed!(
        timings,
        tick_ground_facilities,
        tick_ground_facilities(state, content, rng, &mut events)
    );
    // Launch transit resolution + pad recovery. Not separately timed —
    // O(facilities × transits), negligible vs station/ground ticking.
    resolve_launch_transits(state, content, rng, &mut events);
    tick_launch_pad_recovery(state, content);
    timed!(
        timings,
        tick_satellites,
        tick_satellites(state, content, rng, &mut events)
    );
    timed!(
        timings,
        advance_research,
        advance_research(state, content, &mut events)
    );
    // Milestones don't need per-tick evaluation. Share the scoring
    // interval so progression and scoring stay aligned — both are
    // content-configurable via `scoring.json::computation_interval_ticks`.
    // This avoids the per-tick overhead of iterating milestones, sorting,
    // and potentially computing metrics (profiling showed 38% of tick time).
    let milestone_interval = content.scoring.computation_interval_ticks.max(1);
    if state.meta.tick.is_multiple_of(milestone_interval) {
        timed!(
            timings,
            evaluate_milestones,
            crate::milestone::evaluate_milestones(state, content, &mut events)
        );
    }
    timed!(
        timings,
        evaluate_events,
        crate::sim_events::evaluate_events(state, content, rng, &mut events)
    );
    timed!(
        timings,
        replenish_scan_sites,
        replenish_scan_sites(state, content, rng, &mut events)
    );

    // Debug-only: verify cached ship stats match fresh recomputation.
    #[cfg(debug_assertions)]
    {
        for ship in state.ships.values_mut() {
            if content.hulls.contains_key(&ship.hull_id) {
                let before_cargo = ship.cargo_capacity_m3;
                let before_speed = ship.speed_ticks_per_au;
                let before_propellant_cap = ship.propellant_capacity_kg;
                crate::commands::recompute_ship_stats(ship, content);
                debug_assert!(
                    (ship.cargo_capacity_m3 - before_cargo).abs() < f32::EPSILON
                        && ship.speed_ticks_per_au == before_speed
                        && (ship.propellant_capacity_kg - before_propellant_cap).abs()
                            < f32::EPSILON,
                    "ship {} cached stats diverged from recomputation",
                    ship.id.0
                );
            }
        }
    }

    state.meta.tick += 1;
    events
}

/// Deduct per-module operating costs for all ground facilities.
/// Only enabled modules incur costs. Costs are content-driven via `ModuleDef.operating_cost_per_tick`.
fn deduct_operating_costs(
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;
    for gf in state.ground_facilities.values() {
        let total_cost: f64 = gf
            .core
            .modules
            .iter()
            .filter(|m| m.enabled)
            .filter_map(|m| content.module_defs.get(&m.def_id))
            .map(|def| def.operating_cost_per_tick)
            .sum();

        if total_cost > 0.0 {
            state.balance -= total_cost;
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                crate::Event::OperatingCostDeducted {
                    facility_name: gf.name.clone(),
                    amount: total_cost,
                    balance_after: state.balance,
                },
            ));
        }
    }
}

#[allow(clippy::too_many_lines)] // Thin dispatcher — all logic in commands.rs
fn apply_commands(
    state: &mut GameState,
    commands: &[CommandEnvelope],
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<crate::EventEnvelope>,
) {
    use crate::commands;

    let current_tick = state.meta.tick;

    // Validate and collect assignments first to avoid split borrows.
    let mut assignments: Vec<(ShipId, TaskKind)> = Vec::new();

    for envelope in commands {
        if envelope.execute_at_tick != current_tick {
            continue;
        }
        match &envelope.command {
            Command::AssignShipTask { ship_id, task_kind } => {
                commands::handle_assign_ship_task(
                    state,
                    content,
                    ship_id,
                    task_kind,
                    &envelope.issued_by,
                    &mut assignments,
                );
            }
            Command::InstallModule {
                facility_id,
                module_item_id,
                slot_index,
            } => match facility_id {
                crate::FacilityId::Station(station_id) => {
                    commands::handle_install_module(
                        state,
                        content,
                        station_id,
                        module_item_id,
                        *slot_index,
                        current_tick,
                        events,
                    );
                }
                crate::FacilityId::Ground(gf_id) => {
                    // Ground facilities don't use frames — the slot_index
                    // field is ignored on the ground install path.
                    commands::handle_ground_install_module(
                        state,
                        content,
                        gf_id,
                        module_item_id,
                        current_tick,
                        events,
                    );
                }
            },
            Command::UninstallModule {
                facility_id,
                module_id,
            } => {
                let crate::FacilityId::Station(station_id) = facility_id else {
                    continue;
                };
                commands::handle_uninstall_module(
                    state,
                    content,
                    station_id,
                    module_id,
                    current_tick,
                    events,
                );
            }
            Command::SetModuleEnabled {
                facility_id,
                module_id,
                enabled,
            } => match facility_id {
                crate::FacilityId::Station(station_id) => {
                    commands::handle_set_module_enabled(
                        state,
                        station_id,
                        module_id,
                        *enabled,
                        current_tick,
                        events,
                    );
                }
                crate::FacilityId::Ground(gf_id) => {
                    commands::handle_ground_set_module_enabled(
                        state,
                        gf_id,
                        module_id,
                        *enabled,
                        current_tick,
                        events,
                    );
                }
            },
            Command::SetModuleThreshold {
                facility_id,
                module_id,
                threshold_kg,
            } => {
                let crate::FacilityId::Station(station_id) = facility_id else {
                    continue;
                };
                commands::handle_set_module_threshold(
                    state,
                    station_id,
                    module_id,
                    *threshold_kg,
                    current_tick,
                    events,
                );
            }
            Command::AssignLabTech {
                station_id,
                module_id,
                tech_id,
            } => {
                commands::handle_assign_lab_tech(state, station_id, module_id, tech_id.as_ref());
            }
            Command::SetAssemblerCap {
                station_id,
                module_id,
                component_id,
                max_stock,
            } => {
                commands::handle_set_assembler_cap(
                    state,
                    station_id,
                    module_id,
                    component_id,
                    *max_stock,
                );
            }
            Command::Import {
                facility_id,
                item_spec,
            } => match facility_id {
                crate::FacilityId::Station(station_id) => {
                    commands::handle_import(
                        state,
                        content,
                        station_id,
                        item_spec,
                        current_tick,
                        rng,
                        events,
                    );
                }
                crate::FacilityId::Ground(gf_id) => {
                    commands::handle_ground_import(
                        state,
                        content,
                        gf_id,
                        item_spec,
                        current_tick,
                        rng,
                        events,
                    );
                }
            },
            Command::Export {
                facility_id,
                item_spec,
            } => match facility_id {
                crate::FacilityId::Station(station_id) => {
                    commands::handle_export(
                        state,
                        content,
                        station_id,
                        item_spec,
                        current_tick,
                        events,
                    );
                }
                crate::FacilityId::Ground(gf_id) => {
                    commands::handle_ground_export(
                        state,
                        content,
                        gf_id,
                        item_spec,
                        current_tick,
                        events,
                    );
                }
            },
            Command::JettisonSlag { station_id } => {
                commands::handle_jettison_slag(state, station_id, current_tick, events);
            }
            Command::SelectRecipe {
                station_id,
                module_id,
                recipe_id,
            } => {
                commands::handle_select_recipe(state, content, station_id, module_id, recipe_id);
            }
            Command::SetModulePriority {
                station_id,
                module_id,
                priority,
            } => {
                commands::handle_set_module_priority(state, station_id, module_id, *priority);
            }
            Command::FitShipModule {
                ship_id,
                slot_index,
                module_def_id,
                station_id,
            } => {
                commands::handle_fit_ship_module(
                    state,
                    content,
                    ship_id,
                    *slot_index,
                    module_def_id,
                    station_id,
                    events,
                );
            }
            Command::UnfitShipModule {
                ship_id,
                slot_index,
                station_id,
            } => {
                commands::handle_unfit_ship_module(
                    state,
                    content,
                    ship_id,
                    *slot_index,
                    station_id,
                    current_tick,
                    events,
                );
            }
            Command::AssignCrew {
                station_id,
                module_id,
                role,
                count,
            } => {
                commands::handle_assign_crew(
                    state, content, station_id, module_id, role, *count, events,
                );
            }
            Command::UnassignCrew {
                station_id,
                module_id,
                role,
                count,
            } => {
                commands::handle_unassign_crew(
                    state, content, station_id, module_id, role, *count, events,
                );
            }
            Command::CreateThermalLink {
                station_id,
                from_module_id,
                from_port_id,
                to_module_id,
                to_port_id,
            } => {
                let link = crate::ThermalLink {
                    from_module_id: from_module_id.clone(),
                    from_port_id: from_port_id.clone(),
                    to_module_id: to_module_id.clone(),
                    to_port_id: to_port_id.clone(),
                };
                commands::handle_create_thermal_link(state, content, &link, station_id, events);
            }
            Command::RemoveThermalLink {
                station_id,
                from_module_id,
                from_port_id,
                to_module_id,
                to_port_id,
            } => {
                let link = crate::ThermalLink {
                    from_module_id: from_module_id.clone(),
                    from_port_id: from_port_id.clone(),
                    to_module_id: to_module_id.clone(),
                    to_port_id: to_port_id.clone(),
                };
                commands::handle_remove_thermal_link(state, &link, station_id, events);
            }
            Command::TransferMolten {
                station_id,
                from_module_id,
                to_module_id,
                element,
                kg,
            } => {
                commands::handle_transfer_molten(
                    state,
                    content,
                    station_id,
                    from_module_id,
                    to_module_id,
                    element,
                    *kg,
                    events,
                );
            }
            Command::Launch {
                facility_id,
                rocket_def_id,
                payload,
                destination,
            } => {
                commands::handle_launch(
                    state,
                    content,
                    facility_id,
                    rocket_def_id,
                    payload,
                    destination,
                    current_tick,
                    events,
                );
            }
            Command::DeploySatellite {
                station_id,
                satellite_def_id,
            } => {
                commands::handle_deploy_satellite(
                    state,
                    content,
                    station_id,
                    satellite_def_id,
                    current_tick,
                    rng,
                    events,
                );
            }
            Command::SetStrategyConfig { config } => {
                // Full replacement — not merge. The interpreter cache (if
                // anyone's listening) observes the change on the next
                // `AutopilotController::generate_commands` pass because the
                // runtime owns its own dirty flag; the authoritative
                // strategy config lives on `GameState`.
                state.strategy_config = config.clone();
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    crate::Event::StrategyConfigChanged,
                ));
            }
        }
    }

    commands::apply_ship_assignments(state, content, assignments, current_tick, events);
}

/// Resolve completed launch transits — deliver payloads that have arrived.
fn resolve_launch_transits(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    // Collect facility IDs to process (avoid borrow issues).
    let facility_ids: Vec<crate::GroundFacilityId> =
        state.ground_facilities.keys().cloned().collect();

    for facility_id in facility_ids {
        // Partition transits into completed and ongoing.
        let Some(facility) = state.ground_facilities.get_mut(&facility_id) else {
            continue;
        };
        let mut completed = Vec::new();
        let mut ongoing = Vec::new();
        for transit in std::mem::take(&mut facility.launch_transits) {
            if current_tick >= transit.arrival_tick {
                completed.push(transit);
            } else {
                ongoing.push(transit);
            }
        }
        facility.launch_transits = ongoing;

        for transit in completed {
            resolve_transit_payload(
                state,
                content,
                rng,
                events,
                &facility_id,
                transit,
                current_tick,
            );
        }
    }
}

/// Resolve a single completed launch transit payload — deliver supplies, deploy
/// station, or create satellite.
fn resolve_transit_payload(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<crate::EventEnvelope>,
    facility_id: &crate::GroundFacilityId,
    transit: crate::LaunchTransitState,
    current_tick: u64,
) {
    let payload_for_event = transit.payload.clone();
    match transit.payload {
        crate::LaunchPayload::Supplies(items) => {
            let target_station = find_nearest_station(state, &transit.destination, content);
            let Some(station_id) = target_station else {
                return;
            };
            let Some(station) = state.stations.get_mut(&station_id) else {
                return;
            };
            crate::trade::merge_into_inventory(&mut station.core.inventory, items);
            station.core.invalidate_volume_cache();
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                crate::Event::PayloadDelivered {
                    facility_id: facility_id.clone(),
                    rocket_def_id: transit.rocket_def_id.clone(),
                    payload: payload_for_event,
                    destination: transit.destination.clone(),
                },
            ));
        }
        crate::LaunchPayload::StationKit => {
            let uuid = crate::generate_uuid(rng);
            let station_id = crate::StationId(format!("station_{uuid}"));
            let station = crate::StationState {
                id: station_id.clone(),
                position: transit.destination.clone(),
                core: crate::FacilityCore {
                    cargo_capacity_m3: 500.0,
                    ..Default::default()
                },
                frame_id: None,
                leaders: Vec::new(),
            };
            state.stations.insert(station_id.clone(), station);
            state.counters.stations_deployed += 1;
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                crate::Event::PayloadDelivered {
                    facility_id: facility_id.clone(),
                    rocket_def_id: transit.rocket_def_id.clone(),
                    payload: payload_for_event,
                    destination: transit.destination.clone(),
                },
            ));
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                crate::Event::StationDeployed {
                    station_id,
                    position: transit.destination,
                },
            ));
        }
        crate::LaunchPayload::Satellite { satellite_def_id } => {
            let Some(sat) = create_satellite(
                &satellite_def_id,
                transit.destination.clone(),
                current_tick,
                content,
                rng,
            ) else {
                return;
            };
            let satellite_id = sat.id.clone();
            let satellite_type = sat.satellite_type.clone();
            let position = sat.position.clone();
            state.satellites.insert(satellite_id.clone(), sat);
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                crate::Event::PayloadDelivered {
                    facility_id: facility_id.clone(),
                    rocket_def_id: transit.rocket_def_id.clone(),
                    payload: payload_for_event,
                    destination: position.clone(),
                },
            ));
            events.push(crate::emit(
                &mut state.counters,
                current_tick,
                crate::Event::SatelliteDeployed {
                    satellite_id,
                    position,
                    satellite_type,
                },
            ));
        }
    }
}

/// Create a `SatelliteState` from a satellite def ID.
pub(crate) fn create_satellite(
    satellite_def_id: &str,
    position: crate::Position,
    current_tick: u64,
    content: &GameContent,
    rng: &mut impl Rng,
) -> Option<crate::SatelliteState> {
    let def = content.satellite_defs.get(satellite_def_id)?;
    let uuid = crate::generate_uuid(rng);
    let satellite_id = crate::SatelliteId(format!("sat_{uuid}"));
    Some(crate::SatelliteState {
        id: satellite_id,
        def_id: satellite_def_id.to_string(),
        name: def.name.clone(),
        position,
        deployed_tick: current_tick,
        wear: 0.0,
        enabled: true,
        satellite_type: def.satellite_type.clone(),
        payload_config: None,
    })
}

/// Find the nearest station to a position. Returns `None` if no stations exist.
fn find_nearest_station(
    state: &GameState,
    _destination: &crate::Position,
    _content: &GameContent,
) -> Option<crate::StationId> {
    // Simple: return the first station. Position-aware routing deferred.
    state.stations.keys().next().cloned()
}

/// Tick launch pad recovery countdowns — pads become available after countdown.
fn tick_launch_pad_recovery(state: &mut GameState, _content: &GameContent) {
    for facility in state.ground_facilities.values_mut() {
        for module in &mut facility.core.modules {
            if let crate::ModuleKindState::LaunchPad(ref mut pad_state) = module.kind_state {
                if !pad_state.available && pad_state.recovery_ticks_remaining > 0 {
                    pad_state.recovery_ticks_remaining -= 1;
                    if pad_state.recovery_ticks_remaining == 0 {
                        pad_state.available = true;
                    }
                }
            }
        }
    }
}

fn resolve_ship_tasks(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    // Collect ships whose task eta has arrived. BTreeMap iteration is already sorted by ID.
    let ship_ids: Vec<ShipId> = state
        .ships
        .values()
        .filter(|ship| {
            matches!(&ship.task, Some(task)
                if task.eta_tick == current_tick
                && !matches!(task.kind, TaskKind::Idle | TaskKind::Refuel { .. }))
        })
        .map(|ship| ship.id.clone())
        .collect();

    for ship_id in ship_ids {
        // Clone the task kind to release the borrow on state.ships.
        let Some(task_kind) = state
            .ships
            .get(&ship_id)
            .and_then(|ship| ship.task.as_ref())
            .map(|task| task.kind.clone())
        else {
            continue;
        };

        resolve_task(&task_kind, state, &ship_id, content, rng, events);
    }
}

fn replenish_scan_sites(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<crate::EventEnvelope>,
) {
    // Interval gating: only check on the configured tick interval.
    let interval = content.constants.replenish_check_interval_ticks;
    if interval > 0 && !state.meta.tick.is_multiple_of(interval) {
        return;
    }

    let target = content.constants.replenish_target_count as usize;
    if state.scan_sites.len() >= target {
        return;
    }

    // Collect bodies that have zones (potential scan site locations).
    let zone_bodies: Vec<&crate::OrbitalBodyDef> = content
        .solar_system
        .bodies
        .iter()
        .filter(|b| b.zone.is_some())
        .collect();
    let templates = &content.asteroid_templates;

    if zone_bodies.is_empty() || templates.is_empty() {
        return;
    }

    let current_tick = state.meta.tick;
    let deficit = target - state.scan_sites.len();
    let batch = deficit.min(content.constants.replenish_batch_size);

    for _ in 0..batch {
        let body = crate::pick_zone_weighted(&zone_bodies, rng);
        let zone_class = body.zone.as_ref().expect("zone body").resource_class;
        let template = crate::pick_template_biased(templates, zone_class, rng);
        let position = crate::random_position_in_zone(body, rng);
        let uuid = crate::generate_uuid(rng);
        let site_id = SiteId(format!("site_{uuid}"));

        state.scan_sites.push(ScanSite {
            id: site_id.clone(),
            position: position.clone(),
            template_id: template.id.clone(),
        });

        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            crate::Event::ScanSiteSpawned {
                site_id,
                position,
                template_id: template.id.clone(),
            },
        ));
    }
}

/// Deduct crew salaries from the balance. Emits `StationBankrupt` on the
/// zero-crossing transition (once, not every tick while bankrupt).
fn deduct_crew_salaries(
    state: &mut GameState,
    content: &GameContent,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let hours_per_tick = f64::from(content.constants.minutes_per_tick) / 60.0;
    let current_tick = state.meta.tick;

    // Collect total salary across all stations
    let mut total_salary = 0.0_f64;
    let station_ids: Vec<crate::StationId> = state.stations.keys().cloned().collect();
    for station_id in &station_ids {
        let station = &state.stations[station_id];
        for (role, &count) in &station.core.crew {
            if let Some(role_def) = content.crew_roles.get(role) {
                total_salary += role_def.salary_per_hour * f64::from(count) * hours_per_tick;
            }
        }
    }

    if total_salary > 0.0 {
        let was_positive = state.balance >= 0.0;
        state.balance -= total_salary;
        if was_positive && state.balance < 0.0 {
            // Emit bankrupt event for the first station (balance is global)
            if let Some(station_id) = station_ids.first() {
                events.push(crate::emit(
                    &mut state.counters,
                    current_tick,
                    crate::Event::StationBankrupt {
                        station_id: station_id.clone(),
                    },
                ));
            }
        }
    }
}
