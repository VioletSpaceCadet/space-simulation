//! sim_core — deterministic simulation tick.
//!
//! No IO, no network. All randomness via the passed-in Rng.

mod types;
pub use types::*;

use rand::Rng;

// ---------------------------------------------------------------------------
// Public tick entry point
// ---------------------------------------------------------------------------

/// Advance the simulation by one tick.
///
/// Order of operations:
/// 1. Apply commands scheduled for this tick.
/// 2. Resolve ship tasks whose eta has arrived.
/// 3. Advance station research on all eligible techs.
/// 4. Increment tick counter.
///
/// Returns all events produced this tick.
pub fn tick(
    state: &mut GameState,
    commands: &[CommandEnvelope],
    content: &GameContent,
    rng: &mut impl Rng,
    event_level: EventLevel,
) -> Vec<EventEnvelope> {
    let mut events = Vec::new();

    apply_commands(state, commands, content, &mut events);
    resolve_ship_tasks(state, content, rng, &mut events);
    advance_research(state, content, rng, event_level, &mut events);

    state.meta.tick += 1;
    events
}

// ---------------------------------------------------------------------------
// Private implementation
// ---------------------------------------------------------------------------

fn emit(counters: &mut Counters, tick: u64, event: Event) -> EventEnvelope {
    let id = EventId(format!("evt_{:06}", counters.next_event_id));
    counters.next_event_id += 1;
    EventEnvelope { id, tick, event }
}

fn task_duration(kind: &TaskKind, constants: &Constants) -> u64 {
    match kind {
        TaskKind::Transit { total_ticks, .. } => *total_ticks,
        TaskKind::Survey { .. } => constants.survey_scan_ticks,
        TaskKind::DeepScan { .. } => constants.deep_scan_ticks,
        TaskKind::Idle => 0,
    }
}

fn task_kind_label(kind: &TaskKind) -> &'static str {
    match kind {
        TaskKind::Idle => "Idle",
        TaskKind::Transit { .. } => "Transit",
        TaskKind::Survey { .. } => "Survey",
        TaskKind::DeepScan { .. } => "DeepScan",
    }
}

fn task_target(kind: &TaskKind) -> Option<String> {
    match kind {
        TaskKind::Idle => None,
        TaskKind::Transit { destination, .. } => Some(destination.0.clone()),
        TaskKind::Survey { site } => Some(site.0.clone()),
        TaskKind::DeepScan { asteroid } => Some(asteroid.0.clone()),
    }
}

/// Returns the number of hops on the shortest undirected path between two nodes,
/// or `None` if no path exists. Returns `Some(0)` when `from == to`.
pub fn shortest_hop_count(from: &NodeId, to: &NodeId, solar_system: &SolarSystemDef) -> Option<u64> {
    if from == to {
        return Some(0);
    }
    use std::collections::{HashSet, VecDeque};
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back((from.clone(), 0u64));
    visited.insert(from.clone());
    while let Some((node, dist)) = queue.pop_front() {
        for (a, b) in &solar_system.edges {
            let neighbor = if a == &node {
                Some(b)
            } else if b == &node {
                Some(a)
            } else {
                None
            };
            if let Some(neighbor) = neighbor {
                if neighbor == to {
                    return Some(dist + 1);
                }
                if visited.insert(neighbor.clone()) {
                    queue.push_back((neighbor.clone(), dist + 1));
                }
            }
        }
    }
    None
}

/// True if any unlocked tech grants the EnableDeepScan effect.
fn deep_scan_enabled(research: &ResearchState, content: &GameContent) -> bool {
    content
        .techs
        .iter()
        .filter(|tech| research.unlocked.contains(&tech.id))
        .flat_map(|tech| &tech.effects)
        .any(|effect| matches!(effect, TechEffect::EnableDeepScan))
}

/// Composition noise sigma from unlocked tech effects, defaulting to 0.0.
fn composition_noise_sigma(research: &ResearchState, content: &GameContent) -> f32 {
    content
        .techs
        .iter()
        .filter(|tech| research.unlocked.contains(&tech.id))
        .flat_map(|tech| &tech.effects)
        .find_map(|effect| match effect {
            TechEffect::DeepScanCompositionNoise { sigma } => Some(*sigma),
            _ => None,
        })
        .unwrap_or(0.0)
}

/// Normalise a composition map so values sum to 1.0. No-op if sum is zero.
fn normalise(composition: &mut CompositionVec) {
    let total: f32 = composition.values().sum();
    if total > 0.0 {
        for value in composition.values_mut() {
            *value /= total;
        }
    }
}

// --- Step 1: apply commands -------------------------------------------------

fn apply_commands(
    state: &mut GameState,
    commands: &[CommandEnvelope],
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    // Validate and collect assignments first to avoid split borrows.
    let mut assignments: Vec<(ShipId, TaskKind)> = Vec::new();

    for envelope in commands {
        if envelope.execute_at_tick != current_tick {
            continue;
        }
        match &envelope.command {
            Command::AssignShipTask { ship_id, task_kind } => {
                let Some(ship) = state.ships.get(ship_id) else {
                    continue;
                };
                if ship.owner != envelope.issued_by {
                    continue;
                }
                if matches!(task_kind, TaskKind::DeepScan { .. })
                    && !deep_scan_enabled(&state.research, content)
                {
                    continue;
                }
                assignments.push((ship_id.clone(), task_kind.clone()));
            }
        }
    }

    for (ship_id, task_kind) in assignments {
        let duration = task_duration(&task_kind, &content.constants);
        let label = task_kind_label(&task_kind).to_string();
        let target = task_target(&task_kind);

        if let Some(ship) = state.ships.get_mut(&ship_id) {
            ship.task = Some(TaskState {
                kind: task_kind,
                started_tick: current_tick,
                eta_tick: current_tick + duration,
            });
        }

        events.push(emit(
            &mut state.counters,
            current_tick,
            Event::TaskStarted {
                ship_id,
                task_kind: label,
                target,
            },
        ));
    }
}

// --- Step 2: resolve ship tasks ---------------------------------------------

fn resolve_ship_tasks(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
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
            TaskKind::Transit { ref destination, ref then, .. } => {
                resolve_transit(state, &ship_id, destination, then, content, events);
            }
            TaskKind::Survey { ref site } => {
                resolve_survey(state, &ship_id, site, content, rng, events);
            }
            TaskKind::DeepScan { ref asteroid } => {
                resolve_deep_scan(state, &ship_id, asteroid, content, rng, events);
            }
            TaskKind::Idle => {}
        }
    }
}

fn resolve_transit(
    state: &mut GameState,
    ship_id: &ShipId,
    destination: &NodeId,
    then: &TaskKind,
    content: &GameContent,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    if let Some(ship) = state.ships.get_mut(ship_id) {
        ship.location_node = destination.clone();
    }

    events.push(emit(
        &mut state.counters,
        current_tick,
        Event::ShipArrived {
            ship_id: ship_id.clone(),
            node: destination.clone(),
        },
    ));

    // Start the follow-on task immediately.
    let duration = task_duration(then, &content.constants);
    let label = task_kind_label(then).to_string();
    let target = task_target(then);

    if let Some(ship) = state.ships.get_mut(ship_id) {
        ship.task = Some(TaskState {
            kind: then.clone(),
            started_tick: current_tick,
            eta_tick: current_tick + duration,
        });
    }

    events.push(emit(
        &mut state.counters,
        current_tick,
        Event::TaskStarted {
            ship_id: ship_id.clone(),
            task_kind: label,
            target,
        },
    ));
}

fn resolve_survey(
    state: &mut GameState,
    ship_id: &ShipId,
    site_id: &SiteId,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    let Some(site_pos) = state.scan_sites.iter().position(|s| &s.id == site_id) else {
        return; // Site already consumed — shouldn't happen with valid state.
    };
    let site = state.scan_sites.remove(site_pos);

    let Some(template) = content
        .asteroid_templates
        .iter()
        .find(|t| t.id == site.template_id)
    else {
        return; // Unknown template — content error.
    };

    // Roll composition from ranges, then normalise.
    let mut composition: CompositionVec = template
        .composition_ranges
        .iter()
        .map(|(element, &(min, max))| (element.clone(), rng.gen_range(min..=max)))
        .collect();
    normalise(&mut composition);

    let asteroid_id = AsteroidId(format!("asteroid_{:04}", state.counters.next_asteroid_id));
    state.counters.next_asteroid_id += 1;

    let anomaly_tags = template.anomaly_tags.clone();
    state.asteroids.insert(
        asteroid_id.clone(),
        AsteroidState {
            id: asteroid_id.clone(),
            location_node: site.node.clone(),
            true_composition: composition,
            anomaly_tags: anomaly_tags.clone(),
            knowledge: AsteroidKnowledge {
                tag_beliefs: vec![],
                composition: None,
            },
        },
    );

    events.push(emit(
        &mut state.counters,
        current_tick,
        Event::AsteroidDiscovered {
            asteroid_id: asteroid_id.clone(),
            location_node: site.node.clone(),
        },
    ));

    // Detect anomaly tags probabilistically.
    let detection_prob = content.constants.survey_tag_detection_probability;
    let detected_tags: Vec<(AnomalyTag, f32)> = anomaly_tags
        .iter()
        .filter(|_| rng.gen::<f32>() < detection_prob)
        .map(|tag| (tag.clone(), detection_prob))
        .collect();

    if let Some(asteroid) = state.asteroids.get_mut(&asteroid_id) {
        asteroid.knowledge.tag_beliefs = detected_tags.clone();
    }

    events.push(emit(
        &mut state.counters,
        current_tick,
        Event::ScanResult {
            asteroid_id: asteroid_id.clone(),
            tags: detected_tags,
        },
    ));

    let amount = content.constants.survey_scan_data_amount;
    let quality = content.constants.survey_scan_data_quality;
    *state
        .research
        .data_pool
        .entry(DataKind::ScanData)
        .or_insert(0.0) += amount * quality;

    events.push(emit(
        &mut state.counters,
        current_tick,
        Event::DataGenerated {
            kind: DataKind::ScanData,
            amount,
            quality,
        },
    ));

    set_ship_idle(state, ship_id, current_tick);

    events.push(emit(
        &mut state.counters,
        current_tick,
        Event::TaskCompleted {
            ship_id: ship_id.clone(),
            task_kind: "Survey".to_string(),
            target: Some(site_id.0.clone()),
        },
    ));
}

fn resolve_deep_scan(
    state: &mut GameState,
    ship_id: &ShipId,
    asteroid_id: &AsteroidId,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    let sigma = composition_noise_sigma(&state.research, content);

    let Some(true_composition) = state
        .asteroids
        .get(asteroid_id)
        .map(|a| a.true_composition.clone())
    else {
        return; // Asteroid not found — shouldn't happen with valid state.
    };

    // Map composition: true value + uniform noise in [-sigma, sigma], clamped and normalised.
    let mut mapped: CompositionVec = true_composition
        .iter()
        .map(|(element, &true_value)| {
            let noise = if sigma > 0.0 {
                rng.gen_range(-sigma..=sigma)
            } else {
                0.0
            };
            (element.clone(), (true_value + noise).clamp(0.0, 1.0))
        })
        .collect();
    normalise(&mut mapped);

    if let Some(asteroid) = state.asteroids.get_mut(asteroid_id) {
        asteroid.knowledge.composition = Some(mapped.clone());
    }

    events.push(emit(
        &mut state.counters,
        current_tick,
        Event::CompositionMapped {
            asteroid_id: asteroid_id.clone(),
            composition: mapped,
        },
    ));

    let amount = content.constants.deep_scan_data_amount;
    let quality = content.constants.deep_scan_data_quality;
    *state
        .research
        .data_pool
        .entry(DataKind::ScanData)
        .or_insert(0.0) += amount * quality;

    events.push(emit(
        &mut state.counters,
        current_tick,
        Event::DataGenerated {
            kind: DataKind::ScanData,
            amount,
            quality,
        },
    ));

    set_ship_idle(state, ship_id, current_tick);

    events.push(emit(
        &mut state.counters,
        current_tick,
        Event::TaskCompleted {
            ship_id: ship_id.clone(),
            task_kind: "DeepScan".to_string(),
            target: Some(asteroid_id.0.clone()),
        },
    ));
}

fn set_ship_idle(state: &mut GameState, ship_id: &ShipId, current_tick: u64) {
    if let Some(ship) = state.ships.get_mut(ship_id) {
        ship.task = Some(TaskState {
            kind: TaskKind::Idle,
            started_tick: current_tick,
            eta_tick: current_tick,
        });
    }
}

// --- Step 3: advance research -----------------------------------------------

fn advance_research(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    event_level: EventLevel,
    events: &mut Vec<EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    // Sort station IDs for deterministic RNG consumption order.
    let mut station_ids: Vec<StationId> = state.stations.keys().cloned().collect();
    station_ids.sort_by(|a, b| a.0.cmp(&b.0));

    for station_id in station_ids {
        let (compute_total, power_per_unit, efficiency) = {
            let facilities = &state.stations[&station_id].facilities;
            (
                facilities.compute_units_total,
                facilities.power_per_compute_unit_per_tick,
                facilities.efficiency,
            )
        };

        // Collect eligible techs: prereqs met, not yet unlocked. Sort for determinism.
        let mut eligible: Vec<(TechId, f32)> = content
            .techs
            .iter()
            .filter(|tech| {
                !state.research.unlocked.contains(&tech.id)
                    && tech
                        .prereqs
                        .iter()
                        .all(|prereq| state.research.unlocked.contains(prereq))
            })
            .map(|tech| (tech.id.clone(), tech.difficulty))
            .collect();
        eligible.sort_by(|(a, _), (b, _)| a.0.cmp(&b.0));

        if eligible.is_empty() {
            continue;
        }

        let per_tech_compute = compute_total as f32 / eligible.len() as f32;
        let total_power = compute_total as f32 * power_per_unit;

        events.push(emit(
            &mut state.counters,
            current_tick,
            Event::PowerConsumed {
                station_id: station_id.clone(),
                amount: total_power,
            },
        ));

        for (tech_id, difficulty) in eligible {
            let current_evidence = {
                let evidence = state
                    .research
                    .evidence
                    .entry(tech_id.clone())
                    .or_insert(0.0);
                *evidence += per_tech_compute * efficiency;
                *evidence
            };

            let p = 1.0 - (-current_evidence / difficulty).exp();
            let rolled: f32 = rng.gen();

            if event_level == EventLevel::Debug {
                events.push(emit(
                    &mut state.counters,
                    current_tick,
                    Event::ResearchRoll {
                        tech_id: tech_id.clone(),
                        evidence: current_evidence,
                        p,
                        rolled,
                    },
                ));
            }

            if rolled < p {
                state.research.unlocked.insert(tech_id.clone());
                events.push(emit(
                    &mut state.counters,
                    current_tick,
                    Event::TechUnlocked { tech_id },
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    // --- Test helpers -------------------------------------------------------

    fn test_content() -> GameContent {
        GameContent {
            content_version: "test".to_string(),
            techs: vec![TechDef {
                id: TechId("tech_deep_scan_v1".to_string()),
                name: "Deep Scan v1".to_string(),
                prereqs: vec![],
                accepted_data: vec![DataKind::ScanData],
                difficulty: 10.0,
                effects: vec![
                    TechEffect::EnableDeepScan,
                    // sigma=0: mapped composition matches true composition exactly
                    TechEffect::DeepScanCompositionNoise { sigma: 0.0 },
                ],
            }],
            solar_system: SolarSystemDef {
                nodes: vec![NodeDef {
                    id: NodeId("node_test".to_string()),
                    name: "Test Node".to_string(),
                }],
                edges: vec![],
            },
            asteroid_templates: vec![AsteroidTemplateDef {
                id: "tmpl_iron_rich".to_string(),
                anomaly_tags: vec![AnomalyTag::IronRich],
                composition_ranges: HashMap::from([
                    // Fixed ranges so true_composition is deterministic.
                    ("Fe".to_string(), (0.7, 0.7)),
                    ("Si".to_string(), (0.3, 0.3)),
                ]),
            }],
            constants: Constants {
                survey_scan_ticks: 1,
                deep_scan_ticks: 1,
                travel_ticks_per_hop: 1,
                survey_scan_data_amount: 5.0,
                survey_scan_data_quality: 1.0,
                deep_scan_data_amount: 15.0,
                deep_scan_data_quality: 1.2,
                // Always detect tags so tests are predictable.
                survey_tag_detection_probability: 1.0,
                asteroid_count_per_template: 1,
                station_compute_units_total: 10,
                station_power_per_compute_unit_per_tick: 1.0,
                station_efficiency: 1.0,
                station_power_available_per_tick: 100.0,
            },
        }
    }

    fn test_state(content: &GameContent) -> GameState {
        let node_id = NodeId("node_test".to_string());
        let ship_id = ShipId("ship_0001".to_string());
        let station_id = StationId("station_earth_orbit".to_string());
        let owner = PrincipalId("principal_autopilot".to_string());

        GameState {
            meta: MetaState {
                tick: 0,
                seed: 42,
                schema_version: 1,
                content_version: content.content_version.clone(),
            },
            scan_sites: vec![ScanSite {
                id: SiteId("site_0001".to_string()),
                node: node_id.clone(),
                template_id: "tmpl_iron_rich".to_string(),
            }],
            asteroids: HashMap::new(),
            ships: HashMap::from([(
                ship_id.clone(),
                ShipState {
                    id: ship_id,
                    location_node: node_id.clone(),
                    owner,
                    task: None,
                },
            )]),
            stations: HashMap::from([(
                station_id.clone(),
                StationState {
                    id: station_id,
                    location_node: node_id,
                    power_available_per_tick: 100.0,
                    facilities: FacilitiesState {
                        compute_units_total: 10,
                        power_per_compute_unit_per_tick: 1.0,
                        efficiency: 1.0,
                    },
                },
            )]),
            research: ResearchState {
                unlocked: HashSet::new(),
                data_pool: HashMap::new(),
                evidence: HashMap::new(),
            },
            counters: Counters {
                next_event_id: 0,
                next_command_id: 0,
                next_asteroid_id: 0,
            },
        }
    }

    fn make_rng() -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(42)
    }

    fn survey_command(state: &GameState) -> CommandEnvelope {
        let ship_id = ShipId("ship_0001".to_string());
        let owner = state.ships[&ship_id].owner.clone();
        CommandEnvelope {
            id: CommandId("cmd_000001".to_string()),
            issued_by: owner,
            issued_tick: state.meta.tick,
            execute_at_tick: state.meta.tick,
            command: Command::AssignShipTask {
                ship_id,
                task_kind: TaskKind::Survey {
                    site: SiteId("site_0001".to_string()),
                },
            },
        }
    }

    // --- Command application ------------------------------------------------

    #[test]
    fn test_assign_survey_sets_task() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

        let ship = &state.ships[&ShipId("ship_0001".to_string())];
        assert!(
            matches!(&ship.task, Some(task) if matches!(task.kind, TaskKind::Survey { .. })),
            "ship should have a Survey task after command"
        );
    }

    #[test]
    fn test_assign_command_emits_task_started() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let cmd = survey_command(&state);
        let events = tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);

        assert!(
            events
                .iter()
                .any(|e| matches!(e.event, Event::TaskStarted { .. })),
            "TaskStarted event should be emitted"
        );
    }

    #[test]
    fn test_wrong_owner_command_is_dropped() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let ship_id = ShipId("ship_0001".to_string());
        let bad_command = CommandEnvelope {
            id: CommandId("cmd_000001".to_string()),
            issued_by: PrincipalId("principal_intruder".to_string()),
            issued_tick: 0,
            execute_at_tick: 0,
            command: Command::AssignShipTask {
                ship_id: ship_id.clone(),
                task_kind: TaskKind::Survey {
                    site: SiteId("site_0001".to_string()),
                },
            },
        };

        tick(
            &mut state,
            &[bad_command],
            &content,
            &mut rng,
            EventLevel::Normal,
        );

        let ship = &state.ships[&ship_id];
        assert!(
            ship.task.is_none(),
            "command from wrong owner should be silently dropped"
        );
    }

    #[test]
    fn test_future_command_not_applied_early() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let ship_id = ShipId("ship_0001".to_string());
        let future_command = CommandEnvelope {
            id: CommandId("cmd_000001".to_string()),
            issued_by: state.ships[&ship_id].owner.clone(),
            issued_tick: 0,
            execute_at_tick: 5, // scheduled for tick 5, not now
            command: Command::AssignShipTask {
                ship_id: ship_id.clone(),
                task_kind: TaskKind::Survey {
                    site: SiteId("site_0001".to_string()),
                },
            },
        };

        tick(
            &mut state,
            &[future_command],
            &content,
            &mut rng,
            EventLevel::Normal,
        );

        let ship = &state.ships[&ship_id];
        assert!(
            ship.task.is_none(),
            "command scheduled for a future tick should not apply yet"
        );
    }

    // --- Survey scan --------------------------------------------------------

    #[test]
    fn test_survey_creates_asteroid_on_completion() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        // Tick 0: assign task (eta_tick = 0 + 1 = 1).
        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        assert!(
            state.asteroids.is_empty(),
            "asteroid should not exist before task completes"
        );

        // Tick 1: task resolves.
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
        assert_eq!(
            state.asteroids.len(),
            1,
            "asteroid should be created on survey completion"
        );
        assert!(state.scan_sites.is_empty(), "scan site should be consumed");
    }

    #[test]
    fn test_survey_emits_expected_events() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        let completion_events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let event_kinds: Vec<&str> = completion_events
            .iter()
            .map(|e| match &e.event {
                Event::AsteroidDiscovered { .. } => "AsteroidDiscovered",
                Event::ScanResult { .. } => "ScanResult",
                Event::DataGenerated { .. } => "DataGenerated",
                Event::TaskCompleted { .. } => "TaskCompleted",
                _ => "other",
            })
            .collect();

        assert!(event_kinds.contains(&"AsteroidDiscovered"));
        assert!(event_kinds.contains(&"ScanResult"));
        assert!(event_kinds.contains(&"DataGenerated"));
        assert!(event_kinds.contains(&"TaskCompleted"));
    }

    #[test]
    fn test_survey_detects_tags_with_prob_one() {
        // test_content sets detection probability to 1.0, so all tags must be detected.
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let tags = events
            .iter()
            .find_map(|e| match &e.event {
                Event::ScanResult { tags, .. } => Some(tags.clone()),
                _ => None,
            })
            .expect("ScanResult should be emitted");

        assert!(
            tags.iter().any(|(tag, _)| *tag == AnomalyTag::IronRich),
            "IronRich tag should be detected when probability is 1.0"
        );
    }

    #[test]
    fn test_survey_accumulates_scan_data() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let scan_data = state
            .research
            .data_pool
            .get(&DataKind::ScanData)
            .copied()
            .unwrap_or(0.0);
        assert!(
            scan_data > 0.0,
            "ScanData should accumulate in the data pool after a survey"
        );
    }

    #[test]
    fn test_ship_returns_to_idle_after_survey() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let ship = &state.ships[&ShipId("ship_0001".to_string())];
        assert!(
            matches!(&ship.task, Some(task) if matches!(task.kind, TaskKind::Idle)),
            "ship should return to Idle after survey completes"
        );
    }

    // --- Deep scan ----------------------------------------------------------

    #[test]
    fn test_deep_scan_blocked_without_tech() {
        // Use very high difficulty so research cannot unlock the tech in the setup ticks.
        let mut content = test_content();
        content.techs[0].difficulty = 1_000_000.0;
        let mut state = test_state(&content);
        let mut rng = make_rng();

        // Create an asteroid first.
        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let asteroid_id = state.asteroids.keys().next().unwrap().clone();
        let ship_id = ShipId("ship_0001".to_string());
        let owner = state.ships[&ship_id].owner.clone();

        let deep_cmd = CommandEnvelope {
            id: CommandId("cmd_000002".to_string()),
            issued_by: owner,
            issued_tick: state.meta.tick,
            execute_at_tick: state.meta.tick,
            command: Command::AssignShipTask {
                ship_id: ship_id.clone(),
                task_kind: TaskKind::DeepScan {
                    asteroid: asteroid_id,
                },
            },
        };

        tick(
            &mut state,
            &[deep_cmd],
            &content,
            &mut rng,
            EventLevel::Normal,
        );

        let ship = &state.ships[&ship_id];
        assert!(
            !matches!(&ship.task, Some(task) if matches!(task.kind, TaskKind::DeepScan { .. })),
            "DeepScan command should be dropped when tech is not unlocked"
        );
    }

    #[test]
    fn test_deep_scan_maps_composition() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        // Unlock the tech directly.
        state
            .research
            .unlocked
            .insert(TechId("tech_deep_scan_v1".to_string()));

        // Survey to create an asteroid.
        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let asteroid_id = state.asteroids.keys().next().unwrap().clone();
        assert!(
            state.asteroids[&asteroid_id]
                .knowledge
                .composition
                .is_none(),
            "composition should be unknown before deep scan"
        );

        let ship_id = ShipId("ship_0001".to_string());
        let owner = state.ships[&ship_id].owner.clone();
        let deep_cmd = CommandEnvelope {
            id: CommandId("cmd_000002".to_string()),
            issued_by: owner,
            issued_tick: state.meta.tick,
            execute_at_tick: state.meta.tick,
            command: Command::AssignShipTask {
                ship_id,
                task_kind: TaskKind::DeepScan {
                    asteroid: asteroid_id.clone(),
                },
            },
        };

        tick(
            &mut state,
            &[deep_cmd],
            &content,
            &mut rng,
            EventLevel::Normal,
        );
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let composition = state.asteroids[&asteroid_id].knowledge.composition.as_ref();
        assert!(
            composition.is_some(),
            "composition should be mapped after deep scan"
        );
    }

    #[test]
    fn test_deep_scan_composition_matches_truth_with_zero_sigma() {
        // test_content sets sigma=0.0, so mapped should exactly equal true.
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        state
            .research
            .unlocked
            .insert(TechId("tech_deep_scan_v1".to_string()));

        let cmd = survey_command(&state);
        tick(&mut state, &[cmd], &content, &mut rng, EventLevel::Normal);
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let asteroid_id = state.asteroids.keys().next().unwrap().clone();
        let ship_id = ShipId("ship_0001".to_string());
        let owner = state.ships[&ship_id].owner.clone();
        let deep_cmd = CommandEnvelope {
            id: CommandId("cmd_000002".to_string()),
            issued_by: owner,
            issued_tick: state.meta.tick,
            execute_at_tick: state.meta.tick,
            command: Command::AssignShipTask {
                ship_id,
                task_kind: TaskKind::DeepScan {
                    asteroid: asteroid_id.clone(),
                },
            },
        };
        tick(
            &mut state,
            &[deep_cmd],
            &content,
            &mut rng,
            EventLevel::Normal,
        );
        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let asteroid = &state.asteroids[&asteroid_id];
        let mapped = asteroid.knowledge.composition.as_ref().unwrap();
        for (element, &true_val) in &asteroid.true_composition {
            let mapped_val = mapped.get(element).copied().unwrap_or(0.0);
            assert!(
                (mapped_val - true_val).abs() < 1e-5,
                "mapped {element} ({mapped_val}) should equal true value ({true_val}) with sigma=0"
            );
        }
    }

    // --- Research -----------------------------------------------------------

    #[test]
    fn test_research_evidence_accumulates_each_tick() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = ChaCha8Rng::seed_from_u64(999); // seed unlikely to unlock tech immediately

        let tech_id = TechId("tech_deep_scan_v1".to_string());

        // Run enough ticks that evidence grows, but use a known-safe seed.
        // We just want to observe accumulation, not guarantee no unlock.
        // Instead, set difficulty very high so unlock is practically impossible.
        let mut high_difficulty_content = content.clone();
        high_difficulty_content.techs[0].difficulty = 1_000_000.0;

        tick(
            &mut state,
            &[],
            &high_difficulty_content,
            &mut rng,
            EventLevel::Normal,
        );
        let evidence_t1 = *state.research.evidence.get(&tech_id).unwrap_or(&0.0);

        tick(
            &mut state,
            &[],
            &high_difficulty_content,
            &mut rng,
            EventLevel::Normal,
        );
        let evidence_t2 = *state.research.evidence.get(&tech_id).unwrap_or(&0.0);

        assert!(
            evidence_t1 > 0.0,
            "evidence should be positive after first tick"
        );
        assert!(
            evidence_t2 > evidence_t1,
            "evidence should increase each tick"
        );
    }

    #[test]
    fn test_research_emits_power_consumed() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        assert!(
            events
                .iter()
                .any(|e| matches!(e.event, Event::PowerConsumed { .. })),
            "PowerConsumed should be emitted each tick that research runs"
        );
    }

    #[test]
    fn test_research_power_amount_correct() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let power = events
            .iter()
            .find_map(|e| match &e.event {
                Event::PowerConsumed { amount, .. } => Some(*amount),
                _ => None,
            })
            .expect("PowerConsumed event should be present");

        // compute_units_total=10, power_per_unit=1.0 → 10.0
        assert!(
            (power - 10.0).abs() < 1e-5,
            "power consumed should equal compute_units_total * power_per_unit"
        );
    }

    #[test]
    fn test_research_prereq_blocks_evidence() {
        let mut content = test_content();
        content.techs[0].prereqs = vec![TechId("tech_not_yet_unlocked".to_string())];

        let mut state = test_state(&content);
        let mut rng = make_rng();

        tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        let tech_id = TechId("tech_deep_scan_v1".to_string());
        let evidence = state
            .research
            .evidence
            .get(&tech_id)
            .copied()
            .unwrap_or(0.0);
        assert_eq!(
            evidence, 0.0,
            "evidence should not accumulate when prerequisites are unmet"
        );
    }

    #[test]
    fn test_research_no_power_consumed_when_no_eligible_techs() {
        let content = test_content();
        let mut state = test_state(&content);
        // Pre-unlock the only tech so nothing is eligible.
        state
            .research
            .unlocked
            .insert(TechId("tech_deep_scan_v1".to_string()));
        let mut rng = make_rng();

        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        assert!(
            !events
                .iter()
                .any(|e| matches!(e.event, Event::PowerConsumed { .. })),
            "no PowerConsumed when all techs are already unlocked"
        );
    }

    #[test]
    fn test_tech_unlocks_eventually() {
        let content = test_content(); // difficulty=10, compute=10
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let tech_id = TechId("tech_deep_scan_v1".to_string());
        let mut unlocked_at = None;

        for tick_num in 0..500 {
            tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
            if state.research.unlocked.contains(&tech_id) {
                unlocked_at = Some(tick_num);
                break;
            }
        }

        assert!(unlocked_at.is_some(), "tech should unlock within 500 ticks");
    }

    #[test]
    fn test_tech_unlock_tick_is_deterministic() {
        let content = test_content();

        let unlock_tick = |seed: u64| -> Option<u64> {
            let mut state = test_state(&content);
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let tech_id = TechId("tech_deep_scan_v1".to_string());
            for _ in 0..500 {
                tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
                if state.research.unlocked.contains(&tech_id) {
                    return Some(state.meta.tick);
                }
            }
            None
        };

        assert_eq!(
            unlock_tick(42),
            unlock_tick(42),
            "same seed must produce the same unlock tick"
        );
    }

    #[test]
    fn test_debug_level_emits_research_roll_events() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Debug);

        assert!(
            events
                .iter()
                .any(|e| matches!(e.event, Event::ResearchRoll { .. })),
            "ResearchRoll events should be emitted at EventLevel::Debug"
        );
    }

    #[test]
    fn test_normal_level_suppresses_research_roll_events() {
        let content = test_content();
        let mut state = test_state(&content);
        let mut rng = make_rng();

        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);

        assert!(
            !events
                .iter()
                .any(|e| matches!(e.event, Event::ResearchRoll { .. })),
            "ResearchRoll events should not be emitted at EventLevel::Normal"
        );
    }

    // --- Determinism --------------------------------------------------------

    #[test]
    fn test_identical_seeds_produce_identical_event_logs() {
        let content = test_content();

        let run = |seed: u64| -> Vec<(String, u64)> {
            let mut state = test_state(&content);
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let mut log = Vec::new();

            let cmd = survey_command(&state);
            for i in 0..20u64 {
                let commands = if i == 0 {
                    std::slice::from_ref(&cmd)
                } else {
                    &[]
                };
                let events = tick(&mut state, commands, &content, &mut rng, EventLevel::Debug);
                for event in events {
                    log.push((event.id.0.clone(), event.tick));
                }
            }
            log
        };

        assert_eq!(
            run(42),
            run(42),
            "identical seeds must produce identical event logs"
        );
    }

    #[test]
    fn test_different_seeds_produce_different_results() {
        let content = test_content();

        let unlock_tick = |seed: u64| -> Option<u64> {
            let mut state = test_state(&content);
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let tech_id = TechId("tech_deep_scan_v1".to_string());
            for _ in 0..500 {
                tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
                if state.research.unlocked.contains(&tech_id) {
                    return Some(state.meta.tick);
                }
            }
            None
        };

        // Seeds 42 and 1234 should unlock at different ticks (very likely with this model).
        // If they happen to collide the test is a false failure — acceptable in practice.
        let tick_42 = unlock_tick(42);
        let tick_1234 = unlock_tick(1234);
        assert_ne!(
            tick_42, tick_1234,
            "different seeds should generally produce different results"
        );
    }

    // --- Transit ------------------------------------------------------------

    #[test]
    fn test_shortest_hop_count_same_node() {
        let content = test_content();
        let node = NodeId("node_test".to_string());
        assert_eq!(
            shortest_hop_count(&node, &node, &content.solar_system),
            Some(0)
        );
    }

    #[test]
    fn test_shortest_hop_count_adjacent() {
        let solar_system = SolarSystemDef {
            nodes: vec![
                NodeDef { id: NodeId("a".to_string()), name: "A".to_string() },
                NodeDef { id: NodeId("b".to_string()), name: "B".to_string() },
            ],
            edges: vec![(NodeId("a".to_string()), NodeId("b".to_string()))],
        };
        assert_eq!(
            shortest_hop_count(
                &NodeId("a".to_string()),
                &NodeId("b".to_string()),
                &solar_system
            ),
            Some(1)
        );
        // Undirected: reverse also works.
        assert_eq!(
            shortest_hop_count(
                &NodeId("b".to_string()),
                &NodeId("a".to_string()),
                &solar_system
            ),
            Some(1)
        );
    }

    #[test]
    fn test_shortest_hop_count_two_hops() {
        let solar_system = SolarSystemDef {
            nodes: vec![
                NodeDef { id: NodeId("a".to_string()), name: "A".to_string() },
                NodeDef { id: NodeId("b".to_string()), name: "B".to_string() },
                NodeDef { id: NodeId("c".to_string()), name: "C".to_string() },
            ],
            edges: vec![
                (NodeId("a".to_string()), NodeId("b".to_string())),
                (NodeId("b".to_string()), NodeId("c".to_string())),
            ],
        };
        assert_eq!(
            shortest_hop_count(
                &NodeId("a".to_string()),
                &NodeId("c".to_string()),
                &solar_system
            ),
            Some(2)
        );
    }

    #[test]
    fn test_shortest_hop_count_no_path() {
        let solar_system = SolarSystemDef {
            nodes: vec![
                NodeDef { id: NodeId("a".to_string()), name: "A".to_string() },
                NodeDef { id: NodeId("b".to_string()), name: "B".to_string() },
            ],
            edges: vec![],
        };
        assert_eq!(
            shortest_hop_count(
                &NodeId("a".to_string()),
                &NodeId("b".to_string()),
                &solar_system
            ),
            None
        );
    }

    #[test]
    fn transit_moves_ship_and_starts_next_task() {
        // Two-node solar system; ship starts at node_a, site is at node_b.
        let mut content = test_content();
        let node_a = NodeId("node_a".to_string());
        let node_b = NodeId("node_b".to_string());
        content.solar_system = SolarSystemDef {
            nodes: vec![
                NodeDef { id: node_a.clone(), name: "A".to_string() },
                NodeDef { id: node_b.clone(), name: "B".to_string() },
            ],
            edges: vec![(node_a.clone(), node_b.clone())],
        };
        content.constants.travel_ticks_per_hop = 5;
        content.constants.survey_scan_ticks = 1;

        let ship_id = ShipId("ship_0001".to_string());
        let site_id = SiteId("site_0001".to_string());
        let owner = PrincipalId("principal_autopilot".to_string());
        let station_id = StationId("station_test".to_string());

        let mut state = GameState {
            meta: MetaState { tick: 0, seed: 0, schema_version: 1, content_version: "test".to_string() },
            scan_sites: vec![ScanSite { id: site_id.clone(), node: node_b.clone(), template_id: "tmpl_iron_rich".to_string() }],
            asteroids: HashMap::new(),
            ships: HashMap::from([(
                ship_id.clone(),
                ShipState { id: ship_id.clone(), location_node: node_a.clone(), owner: owner.clone(), task: None },
            )]),
            stations: HashMap::from([(
                station_id.clone(),
                StationState {
                    id: station_id,
                    location_node: node_a.clone(),
                    power_available_per_tick: 100.0,
                    facilities: FacilitiesState {
                        compute_units_total: 10,
                        power_per_compute_unit_per_tick: 1.0,
                        efficiency: 1.0,
                    },
                },
            )]),
            research: ResearchState {
                unlocked: std::collections::HashSet::new(),
                data_pool: HashMap::new(),
                evidence: HashMap::new(),
            },
            counters: Counters { next_event_id: 0, next_command_id: 0, next_asteroid_id: 0 },
        };

        let mut rng = ChaCha8Rng::seed_from_u64(0);

        // Assign a Transit task: 5 ticks to node_b, then Survey.
        let transit_cmd = CommandEnvelope {
            id: CommandId("cmd_000000".to_string()),
            issued_by: owner,
            issued_tick: 0,
            execute_at_tick: 0,
            command: Command::AssignShipTask {
                ship_id: ship_id.clone(),
                task_kind: TaskKind::Transit {
                    destination: node_b.clone(),
                    total_ticks: 5,
                    then: Box::new(TaskKind::Survey { site: site_id.clone() }),
                },
            },
        };

        // Tick 0: assign transit.
        tick(&mut state, &[transit_cmd], &content, &mut rng, EventLevel::Normal);
        assert_eq!(state.ships[&ship_id].location_node, node_a, "ship still at origin during transit");

        // Ticks 1–4: transit in progress, ship still at node_a.
        for _ in 1..5 {
            tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
        }
        assert_eq!(state.ships[&ship_id].location_node, node_a, "ship still in transit");

        // Tick 5: transit resolves → ship moves to node_b, survey starts.
        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
        assert_eq!(state.ships[&ship_id].location_node, node_b, "ship arrived at destination");
        assert!(
            events.iter().any(|e| matches!(&e.event, Event::ShipArrived { node, .. } if node == &node_b)),
            "ShipArrived event should be emitted"
        );
        let survey_started = events.iter().any(|e| matches!(&e.event,
            Event::TaskStarted { task_kind, .. } if task_kind == "Survey"
        ));
        assert!(survey_started, "Survey task should start immediately after arrival");

        // Tick 6: survey resolves → asteroid discovered.
        let events = tick(&mut state, &[], &content, &mut rng, EventLevel::Normal);
        assert!(
            events.iter().any(|e| matches!(e.event, Event::AsteroidDiscovered { .. })),
            "AsteroidDiscovered after survey completes"
        );
    }
}
