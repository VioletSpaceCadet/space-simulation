use crate::research::advance_research;
use crate::station::tick_stations;
use crate::tasks::{
    resolve_deep_scan, resolve_deposit, resolve_mine, resolve_survey, resolve_transit,
};
use crate::{
    CommandEnvelope, EventLevel, GameContent, GameState, NodeId, ScanSite, ShipId, SiteId, TaskKind,
};
use rand::Rng;

const MIN_UNSCANNED_SITES: usize = 5;
const REPLENISH_BATCH_SIZE: usize = 5;

/// Advance the simulation by one tick.
///
/// Order of operations:
/// 1. Apply commands scheduled for this tick.
/// 2. Resolve ship tasks whose eta has arrived.
/// 3. Tick station modules (refinery processors).
/// 4. Advance station research on all eligible techs.
/// 5. Replenish scan sites if below threshold.
/// 6. Increment tick counter.
///
/// Returns all events produced this tick.
pub fn tick(
    state: &mut GameState,
    commands: &[CommandEnvelope],
    content: &GameContent,
    rng: &mut impl Rng,
    event_level: EventLevel,
) -> Vec<crate::EventEnvelope> {
    let mut events = Vec::new();

    crate::commands::apply_commands(state, commands, content, rng, &mut events);
    resolve_ship_tasks(state, content, rng, &mut events);
    tick_stations(state, content, rng, &mut events);
    advance_research(state, content, rng, event_level, &mut events);
    replenish_scan_sites(state, content, rng, &mut events);

    state.meta.tick += 1;
    events
}

fn resolve_ship_tasks(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    // Collect ships whose task eta has arrived, sorted for determinism.
    let mut ship_ids: Vec<ShipId> = state
        .ships
        .values()
        .filter(|ship| {
            matches!(&ship.task, Some(task)
                if task.eta_tick == current_tick
                && !matches!(task.kind, TaskKind::Idle))
        })
        .map(|ship| ship.id.clone())
        .collect();
    ship_ids.sort_by(|a, b| a.0.cmp(&b.0));

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

        match task_kind {
            TaskKind::Transit {
                ref destination,
                ref then,
                ..
            } => {
                resolve_transit(state, &ship_id, destination, then, content, events);
            }
            TaskKind::Survey { ref site } => {
                resolve_survey(state, &ship_id, site, content, rng, events);
            }
            TaskKind::DeepScan { ref asteroid } => {
                resolve_deep_scan(state, &ship_id, asteroid, content, rng, events);
            }
            TaskKind::Mine { ref asteroid, .. } => {
                resolve_mine(state, &ship_id, asteroid, content, events);
            }
            TaskKind::Deposit { ref station, .. } => {
                resolve_deposit(state, &ship_id, station, content, events);
            }
            TaskKind::Idle => {}
        }
    }
}

fn replenish_scan_sites(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<crate::EventEnvelope>,
) {
    if state.scan_sites.len() >= MIN_UNSCANNED_SITES {
        return;
    }

    let node_ids: Vec<&NodeId> = content.solar_system.nodes.iter().map(|n| &n.id).collect();
    let templates = &content.asteroid_templates;

    if node_ids.is_empty() || templates.is_empty() {
        return;
    }

    let current_tick = state.meta.tick;

    for _ in 0..REPLENISH_BATCH_SIZE {
        let template = &templates[rng.gen_range(0..templates.len())];
        let node = node_ids[rng.gen_range(0..node_ids.len())].clone();
        let uuid = crate::generate_uuid(rng);
        let site_id = SiteId(format!("site_{uuid}"));

        state.scan_sites.push(ScanSite {
            id: site_id.clone(),
            node: node.clone(),
            template_id: template.id.clone(),
        });

        events.push(crate::emit(
            &mut state.counters,
            current_tick,
            crate::Event::ScanSiteSpawned {
                site_id,
                node,
                template_id: template.id.clone(),
            },
        ));
    }
}
