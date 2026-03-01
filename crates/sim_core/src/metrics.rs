//! Snapshot metrics computed from `GameState`.
//!
//! A single `compute_metrics(&GameState, &GameContent) -> MetricsSnapshot` function
//! samples the current state for time-series analysis. No state mutation, no IO.

use crate::{
    tasks::inventory_volume_m3, GameContent, GameState, InventoryItem, ModuleBehaviorDef,
    ModuleKindState, TaskKind,
};
use serde::Serialize;
use std::io::Write;

/// Current schema version — bump when fields are added/removed/reordered.
const METRICS_VERSION: u32 = 6;

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
    pub refinery_stalled_count: u32,

    // Assembler
    pub assembler_active_count: u32,
    pub assembler_stalled_count: u32,

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

    // Wear & Maintenance
    pub avg_module_wear: f32,
    pub max_module_wear: f32,
    pub repair_kits_remaining: u32,

    // Economy
    pub balance: f64,
    pub thruster_count: u32,

    // Power
    pub power_generated_kw: f32,
    pub power_consumed_kw: f32,
    pub power_deficit_kw: f32,
    pub battery_charge_pct: f32,

    // Thermal
    pub station_max_temp_mk: u32,
    pub station_avg_temp_mk: u32,
    pub overheat_warning_count: u32,
    pub overheat_critical_count: u32,
    pub heat_wear_multiplier_avg: f32,
}

#[derive(Default)]
struct InventoryAccumulator {
    total_ore_kg: f32,
    total_material_kg: f32,
    total_slag_kg: f32,
    total_iron_material_kg: f32,
    ore_lot_count: u32,
    ore_fe_weighted_sum: f32,
    ore_total_weight: f32,
    min_ore_fe: f32,
    max_ore_fe: f32,
    material_quality_weighted_sum: f32,
    material_total_weight: f32,
}

impl InventoryAccumulator {
    fn new() -> Self {
        Self {
            min_ore_fe: f32::MAX,
            max_ore_fe: f32::MIN,
            ..Default::default()
        }
    }

    fn accumulate(&mut self, inventory: &[InventoryItem]) {
        for item in inventory {
            match item {
                InventoryItem::Ore {
                    kg, composition, ..
                } => {
                    self.total_ore_kg += kg;
                    self.ore_lot_count += 1;
                    let fe_frac = composition.get(crate::ELEMENT_FE).copied().unwrap_or(0.0);
                    self.ore_fe_weighted_sum += fe_frac * kg;
                    self.ore_total_weight += kg;
                    if fe_frac < self.min_ore_fe {
                        self.min_ore_fe = fe_frac;
                    }
                    if fe_frac > self.max_ore_fe {
                        self.max_ore_fe = fe_frac;
                    }
                }
                InventoryItem::Material {
                    element,
                    kg,
                    quality,
                    ..
                } => {
                    self.total_material_kg += kg;
                    if element == crate::ELEMENT_FE {
                        self.total_iron_material_kg += kg;
                    }
                    self.material_quality_weighted_sum += quality * kg;
                    self.material_total_weight += kg;
                }
                InventoryItem::Slag { kg, .. } => {
                    self.total_slag_kg += kg;
                }
                InventoryItem::Component { .. } | InventoryItem::Module { .. } => {}
            }
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]
pub fn compute_metrics(state: &GameState, content: &GameContent) -> MetricsSnapshot {
    let mut acc = InventoryAccumulator::new();

    let mut station_storage_sum = 0.0_f32;
    let mut station_count = 0_u32;

    let mut refinery_active_count = 0_u32;
    let mut refinery_starved_count = 0_u32;
    let mut refinery_stalled_count = 0_u32;

    let mut assembler_active_count = 0_u32;
    let mut assembler_stalled_count = 0_u32;

    let mut wear_sum = 0.0_f32;
    let mut wear_count = 0_u32;
    let mut max_wear = 0.0_f32;
    let mut total_repair_kits = 0_u32;
    let mut total_thruster_count = 0_u32;

    let mut power_generated_kw = 0.0_f32;
    let mut power_consumed_kw = 0.0_f32;
    let mut power_deficit_kw = 0.0_f32;
    let mut battery_stored_kwh = 0.0_f32;
    let mut battery_capacity_kwh = 0.0_f32;

    let mut thermal_max_temp_mk = 0_u32;
    let mut thermal_temp_sum = 0_u64;
    let mut thermal_module_count = 0_u32;
    let mut overheat_warning_count = 0_u32;
    let mut overheat_critical_count = 0_u32;
    let mut heat_wear_multiplier_sum = 0.0_f32;

    // --- Stations ---
    for station in state.stations.values() {
        acc.accumulate(&station.inventory);

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
            // Track wear for all processor modules (enabled or not)
            let Some(def) = content.module_defs.get(&module.def_id) else {
                continue;
            };
            if matches!(
                def.behavior,
                ModuleBehaviorDef::Processor(_) | ModuleBehaviorDef::Assembler(_)
            ) {
                wear_sum += module.wear.wear;
                wear_count += 1;
                if module.wear.wear > max_wear {
                    max_wear = module.wear.wear;
                }
            }

            // Accumulate thermal metrics for any module with thermal state.
            if let Some(thermal) = &module.thermal {
                thermal_module_count += 1;
                thermal_temp_sum += u64::from(thermal.temp_mk);
                if thermal.temp_mk > thermal_max_temp_mk {
                    thermal_max_temp_mk = thermal.temp_mk;
                }
                match thermal.overheat_zone {
                    crate::OverheatZone::Warning => overheat_warning_count += 1,
                    crate::OverheatZone::Critical => overheat_critical_count += 1,
                    crate::OverheatZone::Nominal => {}
                }
                heat_wear_multiplier_sum +=
                    crate::thermal::heat_wear_multiplier(thermal.overheat_zone, &content.constants);
            }

            if !module.enabled {
                continue;
            }

            match &def.behavior {
                ModuleBehaviorDef::Processor(_) => {
                    refinery_active_count += 1;
                    if let ModuleKindState::Processor(ps) = &module.kind_state {
                        if ps.stalled {
                            refinery_stalled_count += 1;
                        }
                        if total_ore_at_station < ps.threshold_kg {
                            refinery_starved_count += 1;
                        }
                    }
                }
                ModuleBehaviorDef::Assembler(_) => {
                    assembler_active_count += 1;
                    if let ModuleKindState::Assembler(asmb) = &module.kind_state {
                        if asmb.stalled {
                            assembler_stalled_count += 1;
                        }
                    }
                }
                _ => {}
            }
        }

        // Accumulate power metrics from station power state
        power_generated_kw += station.power.generated_kw;
        power_consumed_kw += station.power.consumed_kw;
        power_deficit_kw += station.power.deficit_kw;
        battery_stored_kwh += station.power.battery_stored_kwh;

        // Sum total battery capacity from module defs
        for module in &station.modules {
            if let Some(def) = content.module_defs.get(&module.def_id) {
                if let ModuleBehaviorDef::Battery(battery_def) = &def.behavior {
                    battery_capacity_kwh += battery_def.capacity_kwh;
                }
            }
        }

        // Count repair kits and thrusters
        for item in &station.inventory {
            if let InventoryItem::Component {
                component_id,
                count,
                ..
            } = item
            {
                if component_id.0 == "repair_kit" {
                    total_repair_kits += *count;
                }
                if component_id.0 == "thruster" {
                    total_thruster_count += *count;
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

        acc.accumulate(&ship.inventory);

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
        .flat_map(|dp| dp.points.values())
        .copied()
        .fold(0.0_f32, f32::max);

    // --- Finalize averages ---
    let avg_ore_fe_fraction = if acc.ore_total_weight > 0.0 {
        acc.ore_fe_weighted_sum / acc.ore_total_weight
    } else {
        0.0
    };

    let avg_material_quality = if acc.material_total_weight > 0.0 {
        acc.material_quality_weighted_sum / acc.material_total_weight
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

    let avg_module_wear = if wear_count > 0 {
        wear_sum / wear_count as f32
    } else {
        0.0
    };

    // Clamp min/max to 0.0 when no ore lots exist.
    let min_ore_fe_fraction = if acc.ore_lot_count > 0 {
        acc.min_ore_fe
    } else {
        0.0
    };
    let max_ore_fe_fraction = if acc.ore_lot_count > 0 {
        acc.max_ore_fe
    } else {
        0.0
    };

    let battery_charge_pct = if battery_capacity_kwh > 0.0 {
        battery_stored_kwh / battery_capacity_kwh
    } else {
        0.0
    };

    let station_avg_temp_mk = if thermal_module_count > 0 {
        (thermal_temp_sum / u64::from(thermal_module_count)) as u32
    } else {
        0
    };

    let heat_wear_multiplier_avg = if thermal_module_count > 0 {
        heat_wear_multiplier_sum / thermal_module_count as f32
    } else {
        0.0
    };

    MetricsSnapshot {
        tick: state.meta.tick,
        metrics_version: METRICS_VERSION,
        total_ore_kg: acc.total_ore_kg,
        total_material_kg: acc.total_material_kg,
        total_slag_kg: acc.total_slag_kg,
        total_iron_material_kg: acc.total_iron_material_kg,
        station_storage_used_pct,
        ship_cargo_used_pct,
        avg_ore_fe_fraction,
        ore_lot_count: acc.ore_lot_count,
        min_ore_fe_fraction,
        max_ore_fe_fraction,
        avg_material_quality,
        refinery_active_count,
        refinery_starved_count,
        refinery_stalled_count,
        assembler_active_count,
        assembler_stalled_count,
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
        avg_module_wear,
        max_module_wear: max_wear,
        repair_kits_remaining: total_repair_kits,
        balance: state.balance,
        thruster_count: total_thruster_count,
        power_generated_kw,
        power_consumed_kw,
        power_deficit_kw,
        battery_charge_pct,
        station_max_temp_mk: thermal_max_temp_mk,
        station_avg_temp_mk,
        overheat_warning_count,
        overheat_critical_count,
        heat_wear_multiplier_avg,
    }
}

/// Write the CSV header row for metrics.
pub fn write_metrics_header(writer: &mut impl std::io::Write) -> std::io::Result<()> {
    writeln!(
        writer,
        "tick,metrics_version,\
         total_ore_kg,total_material_kg,total_slag_kg,total_iron_material_kg,\
         station_storage_used_pct,ship_cargo_used_pct,\
         avg_ore_fe_fraction,ore_lot_count,min_ore_fe_fraction,max_ore_fe_fraction,\
         avg_material_quality,\
         refinery_active_count,refinery_starved_count,refinery_stalled_count,\
         assembler_active_count,assembler_stalled_count,\
         fleet_total,fleet_idle,fleet_mining,fleet_transiting,fleet_surveying,fleet_depositing,\
         scan_sites_remaining,asteroids_discovered,asteroids_depleted,\
         techs_unlocked,total_scan_data,max_tech_evidence,\
         avg_module_wear,max_module_wear,repair_kits_remaining,\
         balance,thruster_count,\
         power_generated_kw,power_consumed_kw,power_deficit_kw,battery_charge_pct,\
         station_max_temp_mk,station_avg_temp_mk,overheat_warning_count,overheat_critical_count,\
         heat_wear_multiplier_avg"
    )
}

/// Append a single metrics snapshot as a CSV row.
pub fn append_metrics_row(
    writer: &mut impl std::io::Write,
    snapshot: &MetricsSnapshot,
) -> std::io::Result<()> {
    writeln!(
        writer,
        "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
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
        snapshot.refinery_stalled_count,
        snapshot.assembler_active_count,
        snapshot.assembler_stalled_count,
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
        snapshot.avg_module_wear,
        snapshot.max_module_wear,
        snapshot.repair_kits_remaining,
        snapshot.balance,
        snapshot.thruster_count,
        snapshot.power_generated_kw,
        snapshot.power_consumed_kw,
        snapshot.power_deficit_kw,
        snapshot.battery_charge_pct,
        snapshot.station_max_temp_mk,
        snapshot.station_avg_temp_mk,
        snapshot.overheat_warning_count,
        snapshot.overheat_critical_count,
        snapshot.heat_wear_multiplier_avg,
    )
}

/// Write a collection of snapshots to a CSV file.
pub fn write_metrics_csv(path: &str, snapshots: &[MetricsSnapshot]) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    write_metrics_header(&mut file)?;
    for snapshot in snapshots {
        append_metrics_row(&mut file, snapshot)?;
    }
    Ok(())
}

/// Maximum data rows per CSV file before rotating to a new file.
const MAX_ROWS_PER_FILE: usize = 50_000;

/// Rotating metrics CSV writer. Automatically splits into numbered files
/// (`metrics_000.csv`, `metrics_001.csv`, ...) after [`MAX_ROWS_PER_FILE`] rows each.
pub struct MetricsFileWriter {
    run_dir: std::path::PathBuf,
    file_index: u32,
    rows_in_current_file: usize,
    writer: std::io::BufWriter<std::fs::File>,
}

impl MetricsFileWriter {
    /// Create a new writer, opening the first CSV file with a header row.
    pub fn new(run_dir: std::path::PathBuf) -> std::io::Result<Self> {
        let (writer, _) = open_csv_file(&run_dir, 0)?;
        Ok(Self {
            run_dir,
            file_index: 0,
            rows_in_current_file: 0,
            writer,
        })
    }

    /// Append one snapshot row, rotating to a new file if the current one is full.
    pub fn write_row(&mut self, snapshot: &MetricsSnapshot) -> std::io::Result<()> {
        if self.rows_in_current_file >= MAX_ROWS_PER_FILE {
            self.writer.flush()?;
            self.file_index += 1;
            let (new_writer, _) = open_csv_file(&self.run_dir, self.file_index)?;
            self.writer = new_writer;
            self.rows_in_current_file = 0;
        }
        append_metrics_row(&mut self.writer, snapshot)?;
        self.writer.flush()?;
        self.rows_in_current_file += 1;
        Ok(())
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

fn open_csv_file(
    run_dir: &std::path::Path,
    index: u32,
) -> std::io::Result<(std::io::BufWriter<std::fs::File>, std::path::PathBuf)> {
    let name = format!("metrics_{index:03}.csv");
    let path = run_dir.join(&name);
    let file = std::fs::File::create(&path)?;
    let mut writer = std::io::BufWriter::new(file);
    write_metrics_header(&mut writer)?;
    Ok((writer, path))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        test_fixtures::base_content, AsteroidId, AsteroidKnowledge, AsteroidState, Counters,
        DataKind, DomainProgress, GameState, LotId, MetaState, ModuleInstanceId, ModuleState,
        NodeId, PrincipalId, ProcessorState, ResearchDomain, ResearchState, ShipId, ShipState,
        StationId, StationState, TaskState, TechId,
    };
    use std::collections::{HashMap, HashSet};

    fn empty_content() -> crate::GameContent {
        base_content()
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

    fn make_station(inventory: Vec<InventoryItem>, modules: Vec<ModuleState>) -> StationState {
        StationState {
            id: StationId("station_0001".to_string()),
            location_node: NodeId("node_test".to_string()),
            inventory,
            cargo_capacity_m3: 10_000.0,
            power_available_per_tick: 100.0,
            modules,
            power: crate::PowerState::default(),
            cached_inventory_volume_m3: None,
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
    #[allow(clippy::float_cmp)]
    #[allow(clippy::cognitive_complexity)] // exhaustive field-by-field assertions
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
        assert_eq!(snapshot.refinery_stalled_count, 0);
        assert_eq!(snapshot.fleet_total, 0);
        assert_eq!(snapshot.fleet_idle, 0);
        assert_eq!(snapshot.scan_sites_remaining, 0);
        assert_eq!(snapshot.asteroids_discovered, 0);
        assert_eq!(snapshot.asteroids_depleted, 0);
        assert_eq!(snapshot.techs_unlocked, 0);
        assert_eq!(snapshot.total_scan_data, 0.0);
        assert_eq!(snapshot.max_tech_evidence, 0.0);
        assert_eq!(snapshot.avg_module_wear, 0.0);
        assert_eq!(snapshot.max_module_wear, 0.0);
        assert_eq!(snapshot.repair_kits_remaining, 0);
        assert_eq!(snapshot.balance, 0.0);
        assert_eq!(snapshot.thruster_count, 0);
        assert_eq!(snapshot.power_generated_kw, 0.0);
        assert_eq!(snapshot.power_consumed_kw, 0.0);
        assert_eq!(snapshot.power_deficit_kw, 0.0);
        assert_eq!(snapshot.battery_charge_pct, 0.0);
        assert_eq!(snapshot.station_max_temp_mk, 0);
        assert_eq!(snapshot.station_avg_temp_mk, 0);
        assert_eq!(snapshot.overheat_warning_count, 0);
        assert_eq!(snapshot.overheat_critical_count, 0);
        assert!((snapshot.heat_wear_multiplier_avg - 0.0).abs() < f32::EPSILON);
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
                    thermal: None,
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
                    blocked: false,
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
        content.module_defs = HashMap::from([(
            "module_basic_iron_refinery".to_string(),
            crate::ModuleDef {
                id: "module_basic_iron_refinery".to_string(),
                name: "Basic Iron Refinery".to_string(),
                mass_kg: 5000.0,
                volume_m3: 10.0,
                power_consumption_per_run: 10.0,
                wear_per_run: 0.01,
                behavior: ModuleBehaviorDef::Processor(crate::ProcessorDef {
                    processing_interval_minutes: 60,
                    processing_interval_ticks: 60,
                    recipes: vec![],
                }),
                thermal: None,
            },
        )]);

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
                    stalled: false,
                }),
                wear: crate::WearState::default(),
                power_stalled: false,
                thermal: None,
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
    #[allow(clippy::float_cmp)]
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
                    thermal: None,
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
        state.research.evidence.insert(
            TechId("tech_c".to_string()),
            DomainProgress {
                points: HashMap::from([(ResearchDomain::Exploration, 15.0)]),
            },
        );
        state.research.evidence.insert(
            TechId("tech_d".to_string()),
            DomainProgress {
                points: HashMap::from([(ResearchDomain::Materials, 30.0)]),
            },
        );

        let snapshot = compute_metrics(&state, &content);

        assert_eq!(snapshot.techs_unlocked, 2);
        assert!((snapshot.total_scan_data - 42.5).abs() < 1e-5);
        assert!((snapshot.max_tech_evidence - 30.0).abs() < 1e-5);
    }

    #[test]
    fn test_refinery_stalled_metric() {
        let mut content = empty_content();
        content.module_defs = HashMap::from([(
            "module_basic_iron_refinery".to_string(),
            crate::ModuleDef {
                id: "module_basic_iron_refinery".to_string(),
                name: "Basic Iron Refinery".to_string(),
                mass_kg: 5000.0,
                volume_m3: 10.0,
                power_consumption_per_run: 10.0,
                wear_per_run: 0.01,
                behavior: ModuleBehaviorDef::Processor(crate::ProcessorDef {
                    processing_interval_minutes: 60,
                    processing_interval_ticks: 60,
                    recipes: vec![],
                }),
                thermal: None,
            },
        )]);

        let mut state = empty_state();
        let station = make_station(
            vec![InventoryItem::Ore {
                lot_id: LotId("lot_0001".to_string()),
                asteroid_id: AsteroidId("ast_0001".to_string()),
                kg: 1000.0,
                composition: HashMap::from([("Fe".to_string(), 0.7)]),
            }],
            vec![ModuleState {
                id: ModuleInstanceId("mod_0001".to_string()),
                def_id: "module_basic_iron_refinery".to_string(),
                enabled: true,
                kind_state: ModuleKindState::Processor(ProcessorState {
                    threshold_kg: 500.0,
                    ticks_since_last_run: 0,
                    stalled: true,
                }),
                wear: crate::WearState::default(),
                power_stalled: false,
                thermal: None,
            }],
        );
        state.stations.insert(station.id.clone(), station);

        let snapshot = compute_metrics(&state, &content);

        assert_eq!(snapshot.refinery_active_count, 1);
        assert_eq!(snapshot.refinery_stalled_count, 1);
        // Not starved (1000kg ore > 500kg threshold)
        assert_eq!(snapshot.refinery_starved_count, 0);
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

    #[test]
    fn test_wear_metrics() {
        let mut content = empty_content();
        content.module_defs = HashMap::from([(
            "module_basic_iron_refinery".to_string(),
            crate::ModuleDef {
                id: "module_basic_iron_refinery".to_string(),
                name: "Basic Iron Refinery".to_string(),
                mass_kg: 5000.0,
                volume_m3: 10.0,
                power_consumption_per_run: 10.0,
                wear_per_run: 0.01,
                behavior: ModuleBehaviorDef::Processor(crate::ProcessorDef {
                    processing_interval_minutes: 60,
                    processing_interval_ticks: 60,
                    recipes: vec![],
                }),
                thermal: None,
            },
        )]);

        let mut state = empty_state();
        let station = make_station(
            vec![InventoryItem::Component {
                component_id: crate::ComponentId("repair_kit".to_string()),
                count: 3,
                quality: 1.0,
            }],
            vec![
                ModuleState {
                    id: ModuleInstanceId("mod_0001".to_string()),
                    def_id: "module_basic_iron_refinery".to_string(),
                    enabled: true,
                    kind_state: ModuleKindState::Processor(ProcessorState {
                        threshold_kg: 500.0,
                        ticks_since_last_run: 0,
                        stalled: false,
                    }),
                    wear: crate::WearState { wear: 0.3 },
                    power_stalled: false,
                    thermal: None,
                },
                ModuleState {
                    id: ModuleInstanceId("mod_0002".to_string()),
                    def_id: "module_basic_iron_refinery".to_string(),
                    enabled: true,
                    kind_state: ModuleKindState::Processor(ProcessorState {
                        threshold_kg: 500.0,
                        ticks_since_last_run: 0,
                        stalled: false,
                    }),
                    wear: crate::WearState { wear: 0.7 },
                    power_stalled: false,
                    thermal: None,
                },
            ],
        );
        state.stations.insert(station.id.clone(), station);

        let snapshot = compute_metrics(&state, &content);
        assert!(
            (snapshot.avg_module_wear - 0.5).abs() < 1e-5,
            "avg wear should be 0.5"
        );
        assert!(
            (snapshot.max_module_wear - 0.7).abs() < 1e-5,
            "max wear should be 0.7"
        );
        assert_eq!(snapshot.repair_kits_remaining, 3);
    }

    #[test]
    fn test_power_metrics() {
        let mut content = empty_content();
        content.module_defs.insert(
            "module_basic_battery".to_string(),
            crate::ModuleDef {
                id: "module_basic_battery".to_string(),
                name: "Basic Battery".to_string(),
                mass_kg: 2000.0,
                volume_m3: 4.0,
                power_consumption_per_run: 0.0,
                wear_per_run: 0.0,
                behavior: ModuleBehaviorDef::Battery(crate::BatteryDef {
                    capacity_kwh: 100.0,
                    charge_rate_kw: 20.0,
                    discharge_rate_kw: 30.0,
                }),
                thermal: None,
            },
        );
        let mut state = empty_state();

        let mut station = make_station(vec![], vec![]);
        station.power = crate::PowerState {
            generated_kw: 100.0,
            consumed_kw: 80.0,
            deficit_kw: 0.0,
            battery_discharge_kw: 0.0,
            battery_charge_kw: 20.0,
            battery_stored_kwh: 50.0,
        };
        // Add a battery module so we can compute capacity for charge_pct
        station.modules.push(ModuleState {
            id: ModuleInstanceId("mod_bat".to_string()),
            def_id: "module_basic_battery".to_string(),
            enabled: true,
            kind_state: ModuleKindState::Battery(crate::BatteryState { charge_kwh: 50.0 }),
            wear: crate::WearState::default(),
            power_stalled: false,
            thermal: None,
        });
        state.stations.insert(station.id.clone(), station);

        let snapshot = compute_metrics(&state, &content);

        assert!((snapshot.power_generated_kw - 100.0).abs() < 1e-3);
        assert!((snapshot.power_consumed_kw - 80.0).abs() < 1e-3);
        assert!((snapshot.power_deficit_kw - 0.0).abs() < 1e-3);
        // 50 kWh stored / 100 kWh capacity = 0.5
        assert!(
            (snapshot.battery_charge_pct - 0.5).abs() < 1e-3,
            "expected ~0.5, got {}",
            snapshot.battery_charge_pct
        );
    }

    #[test]
    fn test_thermal_metrics_with_modules() {
        let mut content = empty_content();
        content.module_defs.insert(
            "module_smelter".to_string(),
            crate::ModuleDef {
                id: "module_smelter".to_string(),
                name: "Smelter".to_string(),
                mass_kg: 5000.0,
                volume_m3: 10.0,
                power_consumption_per_run: 10.0,
                wear_per_run: 0.01,
                behavior: ModuleBehaviorDef::Processor(crate::ProcessorDef {
                    processing_interval_minutes: 60,
                    processing_interval_ticks: 60,
                    recipes: vec![],
                }),
                thermal: Some(crate::ThermalDef {
                    heat_capacity_j_per_k: 500.0,
                    passive_cooling_coefficient: 0.0,
                    max_temp_mk: 2_000_000,
                    operating_min_mk: None,
                    operating_max_mk: None,
                    thermal_group: None,
                }),
            },
        );

        let mut state = empty_state();
        let station = make_station(
            vec![],
            vec![
                // Module at 1_800_000 mK (nominal)
                ModuleState {
                    id: ModuleInstanceId("smelter_a".to_string()),
                    def_id: "module_smelter".to_string(),
                    enabled: true,
                    kind_state: ModuleKindState::Processor(ProcessorState {
                        threshold_kg: 100.0,
                        ticks_since_last_run: 0,
                        stalled: false,
                    }),
                    wear: crate::WearState::default(),
                    power_stalled: false,
                    thermal: Some(crate::ThermalState {
                        temp_mk: 1_800_000,
                        thermal_group: None,
                        overheat_zone: crate::OverheatZone::Nominal,
                        overheat_disabled: false,
                    }),
                },
                // Module at 2_400_000 mK (warning)
                ModuleState {
                    id: ModuleInstanceId("smelter_b".to_string()),
                    def_id: "module_smelter".to_string(),
                    enabled: true,
                    kind_state: ModuleKindState::Processor(ProcessorState {
                        threshold_kg: 100.0,
                        ticks_since_last_run: 0,
                        stalled: false,
                    }),
                    wear: crate::WearState::default(),
                    power_stalled: false,
                    thermal: Some(crate::ThermalState {
                        temp_mk: 2_400_000,
                        thermal_group: None,
                        overheat_zone: crate::OverheatZone::Warning,
                        overheat_disabled: false,
                    }),
                },
                // Module at 2_800_000 mK (critical)
                ModuleState {
                    id: ModuleInstanceId("smelter_c".to_string()),
                    def_id: "module_smelter".to_string(),
                    enabled: false,
                    kind_state: ModuleKindState::Processor(ProcessorState {
                        threshold_kg: 100.0,
                        ticks_since_last_run: 0,
                        stalled: false,
                    }),
                    wear: crate::WearState::default(),
                    power_stalled: false,
                    thermal: Some(crate::ThermalState {
                        temp_mk: 2_800_000,
                        thermal_group: None,
                        overheat_zone: crate::OverheatZone::Critical,
                        overheat_disabled: true,
                    }),
                },
            ],
        );
        // Need a second station to verify we aggregate across stations
        state.stations.insert(station.id.clone(), station);

        let snapshot = compute_metrics(&state, &content);

        assert_eq!(snapshot.station_max_temp_mk, 2_800_000);
        // avg = (1_800_000 + 2_400_000 + 2_800_000) / 3 = 2_333_333
        assert_eq!(snapshot.station_avg_temp_mk, 2_333_333);
        assert_eq!(snapshot.overheat_warning_count, 1);
        assert_eq!(snapshot.overheat_critical_count, 1);
        // Multipliers: nominal=1.0, warning=2.0, critical=4.0 → avg=(1+2+4)/3=2.333...
        assert!(
            (snapshot.heat_wear_multiplier_avg - 7.0 / 3.0).abs() < 1e-5,
            "expected ~2.333, got {}",
            snapshot.heat_wear_multiplier_avg,
        );
    }

    #[test]
    fn test_thermal_metrics_no_thermal_modules() {
        let mut content = empty_content();
        content.module_defs.insert(
            "module_basic_iron_refinery".to_string(),
            crate::ModuleDef {
                id: "module_basic_iron_refinery".to_string(),
                name: "Basic Iron Refinery".to_string(),
                mass_kg: 5000.0,
                volume_m3: 10.0,
                power_consumption_per_run: 10.0,
                wear_per_run: 0.01,
                behavior: ModuleBehaviorDef::Processor(crate::ProcessorDef {
                    processing_interval_minutes: 60,
                    processing_interval_ticks: 60,
                    recipes: vec![],
                }),
                thermal: None,
            },
        );

        let mut state = empty_state();
        let station = make_station(
            vec![],
            vec![ModuleState {
                id: ModuleInstanceId("refinery_a".to_string()),
                def_id: "module_basic_iron_refinery".to_string(),
                enabled: true,
                kind_state: ModuleKindState::Processor(ProcessorState {
                    threshold_kg: 100.0,
                    ticks_since_last_run: 0,
                    stalled: false,
                }),
                wear: crate::WearState::default(),
                power_stalled: false,
                thermal: None,
            }],
        );
        state.stations.insert(station.id.clone(), station);

        let snapshot = compute_metrics(&state, &content);

        assert_eq!(snapshot.station_max_temp_mk, 0);
        assert_eq!(snapshot.station_avg_temp_mk, 0);
        assert_eq!(snapshot.overheat_warning_count, 0);
        assert_eq!(snapshot.overheat_critical_count, 0);
        assert!((snapshot.heat_wear_multiplier_avg - 0.0).abs() < f32::EPSILON);
    }
}
