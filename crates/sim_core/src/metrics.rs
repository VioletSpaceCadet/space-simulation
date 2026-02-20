//! Snapshot metrics computed from `GameState`.
//!
//! A single `compute_metrics(&GameState, &GameContent) -> MetricsSnapshot` function
//! samples the current state for time-series analysis. No state mutation, no IO.

use crate::{
    tasks::inventory_volume_m3, GameContent, GameState, InventoryItem, ModuleBehaviorDef,
    ModuleKindState, TaskKind,
};
use serde::Serialize;

/// Current schema version — bump when fields are added/removed/reordered.
const METRICS_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub tick: u64,
    pub metrics_version: u32,

    // Inventory totals (all stations + ships combined)
    pub total_ore_kg: f32,
    pub total_material_kg: f32,
    pub total_slag_kg: f32,
    pub total_iron_material_kg: f32,

    // Storage pressure
    pub station_storage_used_pct: f32,
    pub ship_cargo_used_pct: f32,

    // Ore quality
    pub avg_ore_fe_fraction: f32,
    pub ore_lot_count: u32,
    pub min_ore_fe_fraction: f32,
    pub max_ore_fe_fraction: f32,

    // Material quality
    pub avg_material_quality: f32,

    // Refinery
    pub refinery_active_count: u32,
    pub refinery_starved_count: u32,

    // Fleet
    pub fleet_total: u32,
    pub fleet_idle: u32,
    pub fleet_mining: u32,
    pub fleet_transiting: u32,
    pub fleet_surveying: u32,
    pub fleet_depositing: u32,

    // Exploration
    pub scan_sites_remaining: u32,
    pub asteroids_discovered: u32,
    pub asteroids_depleted: u32,

    // Research
    pub techs_unlocked: u32,
    pub total_scan_data: f32,
    pub max_tech_evidence: f32,
}

#[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
pub fn compute_metrics(state: &GameState, content: &GameContent) -> MetricsSnapshot {
    let mut total_ore_kg = 0.0_f32;
    let mut total_material_kg = 0.0_f32;
    let mut total_slag_kg = 0.0_f32;
    let mut total_iron_material_kg = 0.0_f32;

    let mut ore_lot_count = 0_u32;
    let mut ore_fe_weighted_sum = 0.0_f32;
    let mut ore_total_weight = 0.0_f32;
    let mut min_ore_fe = f32::MAX;
    let mut max_ore_fe = f32::MIN;

    let mut material_quality_weighted_sum = 0.0_f32;
    let mut material_total_weight = 0.0_f32;

    let mut station_storage_sum = 0.0_f32;
    let mut station_count = 0_u32;

    let mut refinery_active_count = 0_u32;
    let mut refinery_starved_count = 0_u32;

    // --- Stations ---
    for station in state.stations.values() {
        accumulate_inventory(
            &station.inventory,
            &mut total_ore_kg,
            &mut total_material_kg,
            &mut total_slag_kg,
            &mut total_iron_material_kg,
            &mut ore_lot_count,
            &mut ore_fe_weighted_sum,
            &mut ore_total_weight,
            &mut min_ore_fe,
            &mut max_ore_fe,
            &mut material_quality_weighted_sum,
            &mut material_total_weight,
        );

        let volume_used = inventory_volume_m3(&station.inventory, content);
        if station.cargo_capacity_m3 > 0.0 {
            station_storage_sum += volume_used / station.cargo_capacity_m3;
        }
        station_count += 1;

        // Check refinery modules
        let total_ore_at_station: f32 = station
            .inventory
            .iter()
            .filter_map(|item| {
                if let InventoryItem::Ore { kg, .. } = item {
                    Some(*kg)
                } else {
                    None
                }
            })
            .sum();

        for module in &station.modules {
            if !module.enabled {
                continue;
            }
            let Some(def) = content.module_defs.iter().find(|d| d.id == module.def_id) else {
                continue;
            };
            if !matches!(def.behavior, ModuleBehaviorDef::Processor(_)) {
                continue;
            }
            refinery_active_count += 1;

            if let ModuleKindState::Processor(ps) = &module.kind_state {
                if total_ore_at_station < ps.threshold_kg {
                    refinery_starved_count += 1;
                }
            }
        }
    }

    // --- Ships ---
    let mut fleet_total = 0_u32;
    let mut fleet_idle = 0_u32;
    let mut fleet_mining = 0_u32;
    let mut fleet_transiting = 0_u32;
    let mut fleet_surveying = 0_u32;
    let mut fleet_depositing = 0_u32;
    let mut ship_cargo_sum = 0.0_f32;
    let mut ship_count = 0_u32;

    for ship in state.ships.values() {
        fleet_total += 1;

        accumulate_inventory(
            &ship.inventory,
            &mut total_ore_kg,
            &mut total_material_kg,
            &mut total_slag_kg,
            &mut total_iron_material_kg,
            &mut ore_lot_count,
            &mut ore_fe_weighted_sum,
            &mut ore_total_weight,
            &mut min_ore_fe,
            &mut max_ore_fe,
            &mut material_quality_weighted_sum,
            &mut material_total_weight,
        );

        let volume_used = inventory_volume_m3(&ship.inventory, content);
        if ship.cargo_capacity_m3 > 0.0 {
            ship_cargo_sum += volume_used / ship.cargo_capacity_m3;
        }
        ship_count += 1;

        match ship.task.as_ref().map(|t| &t.kind) {
            None | Some(TaskKind::Idle) => fleet_idle += 1,
            Some(TaskKind::Mine { .. }) => fleet_mining += 1,
            Some(TaskKind::Transit { .. }) => fleet_transiting += 1,
            Some(TaskKind::Survey { .. } | TaskKind::DeepScan { .. }) => {
                fleet_surveying += 1;
            }
            Some(TaskKind::Deposit { .. }) => fleet_depositing += 1,
        }
    }

    // --- Exploration ---
    let asteroids_depleted = state
        .asteroids
        .values()
        .filter(|a| a.mass_kg <= 0.0)
        .count() as u32;

    // --- Research ---
    let total_scan_data = state
        .research
        .data_pool
        .get(&crate::DataKind::ScanData)
        .copied()
        .unwrap_or(0.0);

    let max_tech_evidence = state
        .research
        .evidence
        .values()
        .copied()
        .fold(0.0_f32, f32::max);

    // --- Finalize averages ---
    let avg_ore_fe_fraction = if ore_total_weight > 0.0 {
        ore_fe_weighted_sum / ore_total_weight
    } else {
        0.0
    };

    let avg_material_quality = if material_total_weight > 0.0 {
        material_quality_weighted_sum / material_total_weight
    } else {
        0.0
    };

    let station_storage_used_pct = if station_count > 0 {
        station_storage_sum / station_count as f32
    } else {
        0.0
    };

    let ship_cargo_used_pct = if ship_count > 0 {
        ship_cargo_sum / ship_count as f32
    } else {
        0.0
    };

    // Clamp min/max to 0.0 when no ore lots exist.
    let min_ore_fe_fraction = if ore_lot_count > 0 { min_ore_fe } else { 0.0 };
    let max_ore_fe_fraction = if ore_lot_count > 0 { max_ore_fe } else { 0.0 };

    MetricsSnapshot {
        tick: state.meta.tick,
        metrics_version: METRICS_VERSION,
        total_ore_kg,
        total_material_kg,
        total_slag_kg,
        total_iron_material_kg,
        station_storage_used_pct,
        ship_cargo_used_pct,
        avg_ore_fe_fraction,
        ore_lot_count,
        min_ore_fe_fraction,
        max_ore_fe_fraction,
        avg_material_quality,
        refinery_active_count,
        refinery_starved_count,
        fleet_total,
        fleet_idle,
        fleet_mining,
        fleet_transiting,
        fleet_surveying,
        fleet_depositing,
        scan_sites_remaining: state.scan_sites.len() as u32,
        asteroids_discovered: state.asteroids.len() as u32,
        asteroids_depleted,
        techs_unlocked: state.research.unlocked.len() as u32,
        total_scan_data,
        max_tech_evidence,
    }
}

#[allow(clippy::too_many_arguments)]
fn accumulate_inventory(
    inventory: &[InventoryItem],
    total_ore_kg: &mut f32,
    total_material_kg: &mut f32,
    total_slag_kg: &mut f32,
    total_iron_material_kg: &mut f32,
    ore_lot_count: &mut u32,
    ore_fe_weighted_sum: &mut f32,
    ore_total_weight: &mut f32,
    min_ore_fe: &mut f32,
    max_ore_fe: &mut f32,
    material_quality_weighted_sum: &mut f32,
    material_total_weight: &mut f32,
) {
    for item in inventory {
        match item {
            InventoryItem::Ore {
                kg, composition, ..
            } => {
                *total_ore_kg += kg;
                *ore_lot_count += 1;
                let fe_frac = composition.get("Fe").copied().unwrap_or(0.0);
                *ore_fe_weighted_sum += fe_frac * kg;
                *ore_total_weight += kg;
                if fe_frac < *min_ore_fe {
                    *min_ore_fe = fe_frac;
                }
                if fe_frac > *max_ore_fe {
                    *max_ore_fe = fe_frac;
                }
            }
            InventoryItem::Material {
                element,
                kg,
                quality,
            } => {
                *total_material_kg += kg;
                if element == "Fe" {
                    *total_iron_material_kg += kg;
                }
                *material_quality_weighted_sum += quality * kg;
                *material_total_weight += kg;
            }
            InventoryItem::Slag { kg, .. } => {
                *total_slag_kg += kg;
            }
            InventoryItem::Component { .. } | InventoryItem::Module { .. } => {}
        }
    }
}

/// Write a collection of snapshots to a CSV file.
pub fn write_metrics_csv(path: &str, snapshots: &[MetricsSnapshot]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;

    // Header
    writeln!(
        file,
        "tick,metrics_version,\
         total_ore_kg,total_material_kg,total_slag_kg,total_iron_material_kg,\
         station_storage_used_pct,ship_cargo_used_pct,\
         avg_ore_fe_fraction,ore_lot_count,min_ore_fe_fraction,max_ore_fe_fraction,\
         avg_material_quality,\
         refinery_active_count,refinery_starved_count,\
         fleet_total,fleet_idle,fleet_mining,fleet_transiting,fleet_surveying,fleet_depositing,\
         scan_sites_remaining,asteroids_discovered,asteroids_depleted,\
         techs_unlocked,total_scan_data,max_tech_evidence"
    )?;

    for snapshot in snapshots {
        writeln!(
            file,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            snapshot.tick,
            snapshot.metrics_version,
            snapshot.total_ore_kg,
            snapshot.total_material_kg,
            snapshot.total_slag_kg,
            snapshot.total_iron_material_kg,
            snapshot.station_storage_used_pct,
            snapshot.ship_cargo_used_pct,
            snapshot.avg_ore_fe_fraction,
            snapshot.ore_lot_count,
            snapshot.min_ore_fe_fraction,
            snapshot.max_ore_fe_fraction,
            snapshot.avg_material_quality,
            snapshot.refinery_active_count,
            snapshot.refinery_starved_count,
            snapshot.fleet_total,
            snapshot.fleet_idle,
            snapshot.fleet_mining,
            snapshot.fleet_transiting,
            snapshot.fleet_surveying,
            snapshot.fleet_depositing,
            snapshot.scan_sites_remaining,
            snapshot.asteroids_discovered,
            snapshot.asteroids_depleted,
            snapshot.techs_unlocked,
            snapshot.total_scan_data,
            snapshot.max_tech_evidence,
        )?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AsteroidId, AsteroidKnowledge, AsteroidState, Counters, DataKind, FacilitiesState,
        GameContent, GameState, LotId, MetaState, ModuleInstanceId, ModuleState, NodeDef, NodeId,
        PrincipalId, ProcessorState, ResearchState, ShipId, ShipState, StationId, StationState,
        TaskState, TechId,
    };
    use std::collections::{HashMap, HashSet};

    fn empty_content() -> GameContent {
        GameContent {
            content_version: "test".to_string(),
            techs: vec![],
            solar_system: crate::SolarSystemDef {
                nodes: vec![NodeDef {
                    id: NodeId("node_test".to_string()),
                    name: "Test".to_string(),
                }],
                edges: vec![],
            },
            asteroid_templates: vec![],
            elements: vec![
                crate::ElementDef {
                    id: "ore".to_string(),
                    density_kg_per_m3: 3000.0,
                    display_name: "Raw Ore".to_string(),
                    refined_name: None,
                },
                crate::ElementDef {
                    id: "Fe".to_string(),
                    density_kg_per_m3: 7874.0,
                    display_name: "Iron".to_string(),
                    refined_name: Some("Iron Ingot".to_string()),
                },
                crate::ElementDef {
                    id: "slag".to_string(),
                    density_kg_per_m3: 2500.0,
                    display_name: "Slag".to_string(),
                    refined_name: None,
                },
            ],
            module_defs: vec![],
            constants: crate::Constants {
                survey_scan_ticks: 1,
                deep_scan_ticks: 1,
                travel_ticks_per_hop: 1,
                survey_scan_data_amount: 5.0,
                survey_scan_data_quality: 1.0,
                deep_scan_data_amount: 15.0,
                deep_scan_data_quality: 1.2,
                survey_tag_detection_probability: 1.0,
                asteroid_count_per_template: 1,
                asteroid_mass_min_kg: 500.0,
                asteroid_mass_max_kg: 500.0,
                ship_cargo_capacity_m3: 20.0,
                station_cargo_capacity_m3: 10_000.0,
                station_compute_units_total: 10,
                station_power_per_compute_unit_per_tick: 1.0,
                station_efficiency: 1.0,
                station_power_available_per_tick: 100.0,
                mining_rate_kg_per_tick: 50.0,
                deposit_ticks: 1,
                autopilot_iron_rich_confidence_threshold: 0.7,
                autopilot_refinery_threshold_kg: 500.0,
            },
        }
    }

    fn empty_state() -> GameState {
        GameState {
            meta: MetaState {
                tick: 0,
                seed: 42,
                schema_version: 1,
                content_version: "test".to_string(),
            },
            scan_sites: vec![],
            asteroids: HashMap::new(),
            ships: HashMap::new(),
            stations: HashMap::new(),
            research: ResearchState {
                unlocked: HashSet::new(),
                data_pool: HashMap::new(),
                evidence: HashMap::new(),
            },
            counters: Counters {
                next_event_id: 0,
                next_command_id: 0,
                next_asteroid_id: 0,
                next_lot_id: 0,
                next_module_instance_id: 0,
            },
        }
    }

    fn make_station(inventory: Vec<InventoryItem>, modules: Vec<ModuleState>) -> StationState {
        StationState {
            id: StationId("station_0001".to_string()),
            location_node: NodeId("node_test".to_string()),
            inventory,
            cargo_capacity_m3: 10_000.0,
            power_available_per_tick: 100.0,
            facilities: FacilitiesState {
                compute_units_total: 10,
                power_per_compute_unit_per_tick: 1.0,
                efficiency: 1.0,
            },
            modules,
        }
    }

    fn make_ship(task: Option<TaskState>) -> ShipState {
        ShipState {
            id: ShipId("ship_0001".to_string()),
            location_node: NodeId("node_test".to_string()),
            owner: PrincipalId("principal_autopilot".to_string()),
            inventory: vec![],
            cargo_capacity_m3: 20.0,
            task,
        }
    }

    #[test]
    fn test_empty_state_all_zeros() {
        let content = empty_content();
        let state = empty_state();
        let snapshot = compute_metrics(&state, &content);

        assert_eq!(snapshot.tick, 0);
        assert_eq!(snapshot.metrics_version, METRICS_VERSION);
        assert_eq!(snapshot.total_ore_kg, 0.0);
        assert_eq!(snapshot.total_material_kg, 0.0);
        assert_eq!(snapshot.total_slag_kg, 0.0);
        assert_eq!(snapshot.total_iron_material_kg, 0.0);
        assert_eq!(snapshot.station_storage_used_pct, 0.0);
        assert_eq!(snapshot.ship_cargo_used_pct, 0.0);
        assert_eq!(snapshot.avg_ore_fe_fraction, 0.0);
        assert_eq!(snapshot.ore_lot_count, 0);
        assert_eq!(snapshot.min_ore_fe_fraction, 0.0);
        assert_eq!(snapshot.max_ore_fe_fraction, 0.0);
        assert_eq!(snapshot.avg_material_quality, 0.0);
        assert_eq!(snapshot.refinery_active_count, 0);
        assert_eq!(snapshot.refinery_starved_count, 0);
        assert_eq!(snapshot.fleet_total, 0);
        assert_eq!(snapshot.fleet_idle, 0);
        assert_eq!(snapshot.scan_sites_remaining, 0);
        assert_eq!(snapshot.asteroids_discovered, 0);
        assert_eq!(snapshot.asteroids_depleted, 0);
        assert_eq!(snapshot.techs_unlocked, 0);
        assert_eq!(snapshot.total_scan_data, 0.0);
        assert_eq!(snapshot.max_tech_evidence, 0.0);
    }

    #[test]
    fn test_station_with_ore() {
        let content = empty_content();
        let mut state = empty_state();

        let station = make_station(
            vec![InventoryItem::Ore {
                lot_id: LotId("lot_0001".to_string()),
                asteroid_id: AsteroidId("ast_0001".to_string()),
                kg: 1000.0,
                composition: HashMap::from([("Fe".to_string(), 0.7), ("Si".to_string(), 0.3)]),
            }],
            vec![],
        );
        state.stations.insert(station.id.clone(), station);

        let snapshot = compute_metrics(&state, &content);

        assert!((snapshot.total_ore_kg - 1000.0).abs() < 1e-3);
        assert_eq!(snapshot.ore_lot_count, 1);
        assert!((snapshot.avg_ore_fe_fraction - 0.7).abs() < 1e-5);
        assert!((snapshot.min_ore_fe_fraction - 0.7).abs() < 1e-5);
        assert!((snapshot.max_ore_fe_fraction - 0.7).abs() < 1e-5);
    }

    #[test]
    fn test_station_with_material_and_slag() {
        let content = empty_content();
        let mut state = empty_state();

        let station = make_station(
            vec![
                InventoryItem::Material {
                    element: "Fe".to_string(),
                    kg: 500.0,
                    quality: 0.8,
                },
                InventoryItem::Slag {
                    kg: 200.0,
                    composition: HashMap::from([("Si".to_string(), 1.0)]),
                },
            ],
            vec![],
        );
        state.stations.insert(station.id.clone(), station);

        let snapshot = compute_metrics(&state, &content);

        assert!((snapshot.total_material_kg - 500.0).abs() < 1e-3);
        assert!((snapshot.total_iron_material_kg - 500.0).abs() < 1e-3);
        assert!((snapshot.total_slag_kg - 200.0).abs() < 1e-3);
        assert!((snapshot.avg_material_quality - 0.8).abs() < 1e-5);
    }

    #[test]
    fn test_fleet_classification() {
        let content = empty_content();
        let mut state = empty_state();

        // Idle ship (no task)
        let idle_ship = ShipState {
            id: ShipId("ship_idle".to_string()),
            ..make_ship(None)
        };

        // Mining ship
        let mining_ship = ShipState {
            id: ShipId("ship_mining".to_string()),
            ..make_ship(Some(TaskState {
                kind: TaskKind::Mine {
                    asteroid: AsteroidId("ast_0001".to_string()),
                    duration_ticks: 10,
                },
                started_tick: 0,
                eta_tick: 10,
            }))
        };

        // Transiting ship
        let transit_ship = ShipState {
            id: ShipId("ship_transit".to_string()),
            ..make_ship(Some(TaskState {
                kind: TaskKind::Transit {
                    destination: NodeId("node_b".to_string()),
                    total_ticks: 5,
                    then: Box::new(TaskKind::Idle),
                },
                started_tick: 0,
                eta_tick: 5,
            }))
        };

        // Surveying ship
        let survey_ship = ShipState {
            id: ShipId("ship_survey".to_string()),
            ..make_ship(Some(TaskState {
                kind: TaskKind::Survey {
                    site: crate::SiteId("site_0001".to_string()),
                },
                started_tick: 0,
                eta_tick: 1,
            }))
        };

        // Depositing ship
        let deposit_ship = ShipState {
            id: ShipId("ship_deposit".to_string()),
            ..make_ship(Some(TaskState {
                kind: TaskKind::Deposit {
                    station: StationId("station_0001".to_string()),
                },
                started_tick: 0,
                eta_tick: 1,
            }))
        };

        state.ships.insert(idle_ship.id.clone(), idle_ship);
        state.ships.insert(mining_ship.id.clone(), mining_ship);
        state.ships.insert(transit_ship.id.clone(), transit_ship);
        state.ships.insert(survey_ship.id.clone(), survey_ship);
        state.ships.insert(deposit_ship.id.clone(), deposit_ship);

        let snapshot = compute_metrics(&state, &content);

        assert_eq!(snapshot.fleet_total, 5);
        assert_eq!(snapshot.fleet_idle, 1);
        assert_eq!(snapshot.fleet_mining, 1);
        assert_eq!(snapshot.fleet_transiting, 1);
        assert_eq!(snapshot.fleet_surveying, 1);
        assert_eq!(snapshot.fleet_depositing, 1);
    }

    #[test]
    fn test_refinery_starved_detection() {
        let mut content = empty_content();
        content.module_defs = vec![crate::ModuleDef {
            id: "module_basic_iron_refinery".to_string(),
            name: "Basic Iron Refinery".to_string(),
            mass_kg: 5000.0,
            volume_m3: 10.0,
            power_consumption_per_run: 10.0,
            behavior: ModuleBehaviorDef::Processor(crate::ProcessorDef {
                processing_interval_ticks: 60,
                recipes: vec![],
            }),
        }];

        let mut state = empty_state();
        // Station with 100kg ore but threshold is 500kg → starved
        let station = make_station(
            vec![InventoryItem::Ore {
                lot_id: LotId("lot_0001".to_string()),
                asteroid_id: AsteroidId("ast_0001".to_string()),
                kg: 100.0,
                composition: HashMap::from([("Fe".to_string(), 0.7)]),
            }],
            vec![ModuleState {
                id: ModuleInstanceId("mod_0001".to_string()),
                def_id: "module_basic_iron_refinery".to_string(),
                enabled: true,
                kind_state: ModuleKindState::Processor(ProcessorState {
                    threshold_kg: 500.0,
                    ticks_since_last_run: 0,
                }),
            }],
        );
        state.stations.insert(station.id.clone(), station);

        let snapshot = compute_metrics(&state, &content);

        assert_eq!(snapshot.refinery_active_count, 1);
        assert_eq!(snapshot.refinery_starved_count, 1);
    }

    #[test]
    fn test_storage_utilization() {
        let content = empty_content();
        let mut state = empty_state();

        // Station capacity 10,000 m3; 3000kg ore at density 3000 kg/m3 = 1.0 m3 → 0.01%
        let station = make_station(
            vec![InventoryItem::Ore {
                lot_id: LotId("lot_0001".to_string()),
                asteroid_id: AsteroidId("ast_0001".to_string()),
                kg: 3000.0,
                composition: HashMap::from([("Fe".to_string(), 0.7)]),
            }],
            vec![],
        );
        state.stations.insert(station.id.clone(), station);

        let snapshot = compute_metrics(&state, &content);

        // 3000 kg / 3000 density = 1.0 m3; 1.0 / 10000.0 = 0.0001
        assert!(
            (snapshot.station_storage_used_pct - 0.0001).abs() < 1e-5,
            "expected ~0.0001, got {}",
            snapshot.station_storage_used_pct
        );
    }

    #[test]
    fn test_determinism_same_state_same_snapshot() {
        let content = empty_content();
        let mut state = empty_state();

        let station = make_station(
            vec![
                InventoryItem::Ore {
                    lot_id: LotId("lot_0001".to_string()),
                    asteroid_id: AsteroidId("ast_0001".to_string()),
                    kg: 1000.0,
                    composition: HashMap::from([("Fe".to_string(), 0.7)]),
                },
                InventoryItem::Material {
                    element: "Fe".to_string(),
                    kg: 300.0,
                    quality: 0.9,
                },
            ],
            vec![],
        );
        state.stations.insert(station.id.clone(), station);

        state.asteroids.insert(
            AsteroidId("ast_0001".to_string()),
            AsteroidState {
                id: AsteroidId("ast_0001".to_string()),
                location_node: NodeId("node_test".to_string()),
                true_composition: HashMap::from([("Fe".to_string(), 0.7)]),
                anomaly_tags: vec![],
                mass_kg: 500.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![],
                    composition: None,
                },
            },
        );

        let snapshot_a = compute_metrics(&state, &content);
        let snapshot_b = compute_metrics(&state, &content);

        assert_eq!(snapshot_a.tick, snapshot_b.tick);
        assert_eq!(snapshot_a.total_ore_kg, snapshot_b.total_ore_kg);
        assert_eq!(snapshot_a.total_material_kg, snapshot_b.total_material_kg);
        assert_eq!(
            snapshot_a.avg_ore_fe_fraction,
            snapshot_b.avg_ore_fe_fraction
        );
        assert_eq!(
            snapshot_a.avg_material_quality,
            snapshot_b.avg_material_quality
        );
        assert_eq!(
            snapshot_a.asteroids_discovered,
            snapshot_b.asteroids_discovered
        );
    }

    #[test]
    fn test_research_metrics() {
        let content = empty_content();
        let mut state = empty_state();

        state.research.unlocked.insert(TechId("tech_a".to_string()));
        state.research.unlocked.insert(TechId("tech_b".to_string()));
        state.research.data_pool.insert(DataKind::ScanData, 42.5);
        state
            .research
            .evidence
            .insert(TechId("tech_c".to_string()), 15.0);
        state
            .research
            .evidence
            .insert(TechId("tech_d".to_string()), 30.0);

        let snapshot = compute_metrics(&state, &content);

        assert_eq!(snapshot.techs_unlocked, 2);
        assert!((snapshot.total_scan_data - 42.5).abs() < 1e-5);
        assert!((snapshot.max_tech_evidence - 30.0).abs() < 1e-5);
    }

    #[test]
    fn test_exploration_metrics() {
        let content = empty_content();
        let mut state = empty_state();

        state.scan_sites.push(crate::ScanSite {
            id: crate::SiteId("site_0001".to_string()),
            node: NodeId("node_test".to_string()),
            template_id: "tmpl_iron_rich".to_string(),
        });
        state.scan_sites.push(crate::ScanSite {
            id: crate::SiteId("site_0002".to_string()),
            node: NodeId("node_test".to_string()),
            template_id: "tmpl_iron_rich".to_string(),
        });

        state.asteroids.insert(
            AsteroidId("ast_0001".to_string()),
            AsteroidState {
                id: AsteroidId("ast_0001".to_string()),
                location_node: NodeId("node_test".to_string()),
                true_composition: HashMap::new(),
                anomaly_tags: vec![],
                mass_kg: 500.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![],
                    composition: None,
                },
            },
        );
        // Depleted asteroid
        state.asteroids.insert(
            AsteroidId("ast_0002".to_string()),
            AsteroidState {
                id: AsteroidId("ast_0002".to_string()),
                location_node: NodeId("node_test".to_string()),
                true_composition: HashMap::new(),
                anomaly_tags: vec![],
                mass_kg: 0.0,
                knowledge: AsteroidKnowledge {
                    tag_beliefs: vec![],
                    composition: None,
                },
            },
        );

        let snapshot = compute_metrics(&state, &content);

        assert_eq!(snapshot.scan_sites_remaining, 2);
        assert_eq!(snapshot.asteroids_discovered, 2);
        assert_eq!(snapshot.asteroids_depleted, 1);
    }
}
