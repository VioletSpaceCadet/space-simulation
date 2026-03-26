use crate::instrumentation::{timed, TickTimings};
use crate::research::advance_research;
use crate::station::tick_stations;
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
/// 3. Tick station modules (refinery processors).
/// 4. Advance station research on all eligible techs.
///    4.5. Evaluate sim events (content-driven random events).
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
    timed!(
        timings,
        advance_research,
        advance_research(state, content, &mut events)
    );
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
                station_id,
                module_item_id,
            } => {
                commands::handle_install_module(
                    state,
                    content,
                    station_id,
                    module_item_id,
                    current_tick,
                    events,
                );
            }
            Command::UninstallModule {
                station_id,
                module_id,
            } => {
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
                station_id,
                module_id,
                enabled,
            } => {
                commands::handle_set_module_enabled(
                    state,
                    station_id,
                    module_id,
                    *enabled,
                    current_tick,
                    events,
                );
            }
            Command::SetModuleThreshold {
                station_id,
                module_id,
                threshold_kg,
            } => {
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
                station_id,
                item_spec,
            } => {
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
            Command::Export {
                station_id,
                item_spec,
            } => {
                commands::handle_export(
                    state,
                    content,
                    station_id,
                    item_spec,
                    current_tick,
                    events,
                );
            }
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
        }
    }

    commands::apply_ship_assignments(state, content, assignments, current_tick, events);
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
