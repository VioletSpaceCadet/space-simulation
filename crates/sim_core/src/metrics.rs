//! Snapshot metrics computed from `GameState`.
//!
//! `compute_metrics(&GameState, &GameContent) -> MetricsSnapshot` samples the
//! current state for time-series analysis. Uses `MetricsAccumulator` internally
//! to accumulate per-station and per-ship metrics before finalizing averages.
//! No state mutation, no IO.

use crate::{
    tasks::inventory_volume_m3, GameContent, GameState, InventoryItem, ModuleBehaviorDef,
    ModuleKindState, TaskKind,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Write;

/// Current schema version — bump when fields are added/removed/reordered.
/// v11: Replace per-module-type fields with dynamic `per_module_metrics` `BTreeMap`.
pub const METRICS_VERSION: u32 = 13;

/// A typed metric value extracted from a [`MetricsSnapshot`] field.
#[derive(Clone, Copy, Debug)]
pub enum MetricValue {
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
}

impl MetricValue {
    /// Convert any metric value to f64 for generic aggregation.
    /// Note: U64 uses lossy `as f64` cast (only affects values > 2^53, e.g. tick at ~9 quadrillion).
    pub fn as_f64(self) -> f64 {
        match self {
            Self::U32(v) => f64::from(v),
            Self::U64(v) => v as f64,
            Self::F32(v) => f64::from(v),
            Self::F64(v) => v,
        }
    }

    /// Write the value to a CSV cell.
    pub fn write_csv(self, writer: &mut impl std::io::Write) -> std::io::Result<()> {
        match self {
            Self::U32(v) => write!(writer, "{v}"),
            Self::U64(v) => write!(writer, "{v}"),
            Self::F32(v) => write!(writer, "{v}"),
            Self::F64(v) => write!(writer, "{v}"),
        }
    }

    /// Return the type descriptor for this value.
    pub fn metric_type(self) -> MetricType {
        match self {
            Self::U32(_) => MetricType::U32,
            Self::U64(_) => MetricType::U64,
            Self::F32(_) => MetricType::F32,
            Self::F64(_) => MetricType::F64,
        }
    }
}

/// Field type descriptor for schema generation (Arrow, Parquet, etc.).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricType {
    U32,
    U64,
    F32,
    F64,
}

/// Per-element ore composition statistics (avg/min/max fraction across all ore lots).
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct OreElementStats {
    pub avg_fraction: f32,
    pub min_fraction: f32,
    pub max_fraction: f32,
}

/// Per-module-type status counters (active, stalled, starved).
/// Keyed by behavior type name (e.g., `"processor"`, `"assembler"`) in
/// [`MetricsSnapshot::per_module_metrics`].
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct ModuleStatusMetrics {
    pub active: u32,
    pub stalled: u32,
    pub starved: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub tick: u64,
    pub metrics_version: u32,

    // Inventory totals (all stations + ships combined)
    pub total_ore_kg: f32,
    pub total_material_kg: f32,
    pub total_slag_kg: f32,

    /// Per-element refined material kg (e.g. "Fe" → 500.0, "H2O" → 100.0).
    pub per_element_material_kg: BTreeMap<String, f32>,

    // Storage pressure
    pub station_storage_used_pct: f32,
    pub ship_cargo_used_pct: f32,

    // Ore quality (per-element)
    /// Per-element ore composition stats (avg/min/max fraction across ore lots).
    pub per_element_ore_stats: BTreeMap<String, OreElementStats>,
    pub ore_lot_count: u32,

    // Material quality
    pub avg_material_quality: f32,

    /// Per-module-type status metrics, keyed by behavior type name
    /// (e.g., `"processor"`, `"assembler"`). See [`ModuleStatusMetrics`].
    pub per_module_metrics: BTreeMap<String, ModuleStatusMetrics>,

    // Fleet
    pub fleet_total: u32,
    pub fleet_idle: u32,
    pub fleet_mining: u32,
    pub fleet_transiting: u32,
    pub fleet_surveying: u32,
    pub fleet_depositing: u32,
    pub fleet_refueling: u32,

    // Propulsion
    pub fleet_propellant_kg: f32,
    pub fleet_propellant_pct: f32,
    pub propellant_consumed_total: f32,

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
    pub export_revenue_total: f64,
    pub export_count: u32,

    // Power
    pub power_generated_kw: f32,
    pub power_consumed_kw: f32,
    pub power_deficit_kw: f32,
    pub battery_charge_pct: f32,

    // (Propellant totals are in per_element_material_kg: H2O, LH2, LOX)

    // Thermal
    pub station_max_temp_mk: u32,
    pub station_avg_temp_mk: u32,
    pub overheat_warning_count: u32,
    pub overheat_critical_count: u32,
    pub heat_wear_multiplier_avg: f32,
}

impl MetricsSnapshot {
    /// Returns all fixed scalar field (name, value) pairs in column order.
    /// Excludes dynamic per-element maps (`per_element_material_kg`, `per_element_ore_stats`).
    pub fn fixed_field_values(&self) -> Vec<(&'static str, MetricValue)> {
        use MetricValue::{F32, U32};
        let mut fields = self.inventory_field_values();
        fields.extend(self.module_field_values());
        fields.extend(self.fleet_field_values());
        fields.extend(self.exploration_field_values());
        fields.extend(self.economy_field_values());
        fields.extend(self.power_field_values());
        fields.extend([
            ("station_max_temp_mk", U32(self.station_max_temp_mk)),
            ("station_avg_temp_mk", U32(self.station_avg_temp_mk)),
            ("overheat_warning_count", U32(self.overheat_warning_count)),
            ("overheat_critical_count", U32(self.overheat_critical_count)),
            (
                "heat_wear_multiplier_avg",
                F32(self.heat_wear_multiplier_avg),
            ),
        ]);
        fields
    }

    fn inventory_field_values(&self) -> Vec<(&'static str, MetricValue)> {
        use MetricValue::{F32, U32, U64};
        vec![
            ("tick", U64(self.tick)),
            ("metrics_version", U32(self.metrics_version)),
            ("total_ore_kg", F32(self.total_ore_kg)),
            ("total_material_kg", F32(self.total_material_kg)),
            ("total_slag_kg", F32(self.total_slag_kg)),
            (
                "station_storage_used_pct",
                F32(self.station_storage_used_pct),
            ),
            ("ship_cargo_used_pct", F32(self.ship_cargo_used_pct)),
            ("ore_lot_count", U32(self.ore_lot_count)),
            ("avg_material_quality", F32(self.avg_material_quality)),
        ]
    }

    fn module_field_values(&self) -> Vec<(&'static str, MetricValue)> {
        use MetricValue::{F32, U32};
        vec![
            ("avg_module_wear", F32(self.avg_module_wear)),
            ("max_module_wear", F32(self.max_module_wear)),
            ("repair_kits_remaining", U32(self.repair_kits_remaining)),
        ]
    }

    fn fleet_field_values(&self) -> Vec<(&'static str, MetricValue)> {
        use MetricValue::{F32, U32};
        vec![
            ("fleet_total", U32(self.fleet_total)),
            ("fleet_idle", U32(self.fleet_idle)),
            ("fleet_mining", U32(self.fleet_mining)),
            ("fleet_transiting", U32(self.fleet_transiting)),
            ("fleet_surveying", U32(self.fleet_surveying)),
            ("fleet_depositing", U32(self.fleet_depositing)),
            ("fleet_refueling", U32(self.fleet_refueling)),
            ("fleet_propellant_kg", F32(self.fleet_propellant_kg)),
            ("fleet_propellant_pct", F32(self.fleet_propellant_pct)),
            (
                "propellant_consumed_total",
                F32(self.propellant_consumed_total),
            ),
        ]
    }

    fn exploration_field_values(&self) -> Vec<(&'static str, MetricValue)> {
        use MetricValue::{F32, U32};
        vec![
            ("scan_sites_remaining", U32(self.scan_sites_remaining)),
            ("asteroids_discovered", U32(self.asteroids_discovered)),
            ("asteroids_depleted", U32(self.asteroids_depleted)),
            ("techs_unlocked", U32(self.techs_unlocked)),
            ("total_scan_data", F32(self.total_scan_data)),
            ("max_tech_evidence", F32(self.max_tech_evidence)),
        ]
    }

    fn economy_field_values(&self) -> Vec<(&'static str, MetricValue)> {
        use MetricValue::{F64, U32};
        vec![
            ("balance", F64(self.balance)),
            ("thruster_count", U32(self.thruster_count)),
            ("export_revenue_total", F64(self.export_revenue_total)),
            ("export_count", U32(self.export_count)),
        ]
    }

    fn power_field_values(&self) -> Vec<(&'static str, MetricValue)> {
        use MetricValue::F32;
        vec![
            ("power_generated_kw", F32(self.power_generated_kw)),
            ("power_consumed_kw", F32(self.power_consumed_kw)),
            ("power_deficit_kw", F32(self.power_deficit_kw)),
            ("battery_charge_pct", F32(self.battery_charge_pct)),
        ]
    }

    /// Returns fixed scalar field descriptors (name, type) in column order.
    /// Same order as [`fixed_field_values`](Self::fixed_field_values):
    /// inventory → modules → fleet → exploration → economy → power → thermal.
    pub fn fixed_field_descriptors() -> Vec<(&'static str, MetricType)> {
        use MetricType::{F32, F64, U32, U64};
        vec![
            // Inventory
            ("tick", U64),
            ("metrics_version", U32),
            ("total_ore_kg", F32),
            ("total_material_kg", F32),
            ("total_slag_kg", F32),
            ("station_storage_used_pct", F32),
            ("ship_cargo_used_pct", F32),
            ("ore_lot_count", U32),
            ("avg_material_quality", F32),
            // Modules (per-type counts are in per_module_metrics map)
            ("avg_module_wear", F32),
            ("max_module_wear", F32),
            ("repair_kits_remaining", U32),
            // Fleet
            ("fleet_total", U32),
            ("fleet_idle", U32),
            ("fleet_mining", U32),
            ("fleet_transiting", U32),
            ("fleet_surveying", U32),
            ("fleet_depositing", U32),
            ("fleet_refueling", U32),
            ("fleet_propellant_kg", F32),
            ("fleet_propellant_pct", F32),
            ("propellant_consumed_total", F32),
            // Exploration & Research
            ("scan_sites_remaining", U32),
            ("asteroids_discovered", U32),
            ("asteroids_depleted", U32),
            ("techs_unlocked", U32),
            ("total_scan_data", F32),
            ("max_tech_evidence", F32),
            // Economy
            ("balance", F64),
            ("thruster_count", U32),
            ("export_revenue_total", F64),
            ("export_count", U32),
            // Power
            ("power_generated_kw", F32),
            ("power_consumed_kw", F32),
            ("power_deficit_kw", F32),
            ("battery_charge_pct", F32),
            // Thermal
            ("station_max_temp_mk", U32),
            ("station_avg_temp_mk", U32),
            ("overheat_warning_count", U32),
            ("overheat_critical_count", U32),
            ("heat_wear_multiplier_avg", F32),
        ]
    }

    /// Look up a fixed scalar field by name and return its value as f64.
    /// Returns `None` for unknown field names.
    pub fn get_field_f64(&self, name: &str) -> Option<f64> {
        // Check fixed scalar fields first.
        if let Some(val) = self
            .fixed_field_values()
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_f64())
        {
            return Some(val);
        }
        // Check per-module metrics: e.g., "processor_starved" → per_module_metrics["processor"].starved
        if let Some(suffix_start) = name.rfind('_') {
            let type_name = &name[..suffix_start];
            let metric_name = &name[suffix_start + 1..];
            if let Some(metrics) = self.per_module_metrics.get(type_name) {
                return match metric_name {
                    "active" => Some(f64::from(metrics.active)),
                    "stalled" => Some(f64::from(metrics.stalled)),
                    "starved" => Some(f64::from(metrics.starved)),
                    _ => None,
                };
            }
        }
        None
    }
}

#[derive(Default)]
struct InventoryAccumulator {
    total_ore_kg: f32,
    total_material_kg: f32,
    total_slag_kg: f32,
    per_element_material_kg: BTreeMap<String, f32>,
    ore_lot_count: u32,
    ore_total_weight: f32,
    /// Per-element weighted sum of ore fractions (for computing avg).
    ore_element_weighted_sums: BTreeMap<String, f32>,
    ore_element_min: BTreeMap<String, f32>,
    ore_element_max: BTreeMap<String, f32>,
    material_quality_weighted_sum: f32,
    material_total_weight: f32,
}

impl InventoryAccumulator {
    fn accumulate(&mut self, inventory: &[InventoryItem]) {
        for item in inventory {
            match item {
                InventoryItem::Ore {
                    kg, composition, ..
                } => {
                    self.total_ore_kg += kg;
                    self.ore_lot_count += 1;
                    self.ore_total_weight += kg;
                    for (element, &fraction) in composition {
                        *self
                            .ore_element_weighted_sums
                            .entry(element.clone())
                            .or_default() += fraction * kg;
                        let min = self
                            .ore_element_min
                            .entry(element.clone())
                            .or_insert(f32::MAX);
                        if fraction < *min {
                            *min = fraction;
                        }
                        let max = self
                            .ore_element_max
                            .entry(element.clone())
                            .or_insert(f32::MIN);
                        if fraction > *max {
                            *max = fraction;
                        }
                    }
                }
                InventoryItem::Material {
                    element,
                    kg,
                    quality,
                    ..
                } => {
                    self.total_material_kg += kg;
                    *self
                        .per_element_material_kg
                        .entry(element.clone())
                        .or_default() += kg;
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

/// Extract sorted element IDs from content for dynamic CSV columns.
/// Returns all element IDs in definition order.
pub fn content_element_ids(content: &GameContent) -> Vec<String> {
    content.elements.iter().map(|e| e.id.clone()).collect()
}

/// Extract unique behavior type names from content for dynamic CSV/Parquet columns.
/// Returns sorted, deduplicated lowercase type names (e.g., `["assembler", "processor"]`).
pub fn content_behavior_types(content: &GameContent) -> Vec<String> {
    let types: std::collections::BTreeSet<String> = content
        .module_defs
        .values()
        .map(|def| def.behavior.type_name().to_string())
        .collect();
    types.into_iter().collect()
}

pub fn compute_metrics(state: &GameState, content: &GameContent) -> MetricsSnapshot {
    let mut acc = MetricsAccumulator::new();
    for station in state.stations.values() {
        acc.accumulate_station(station, content);
    }
    for ship in state.ships.values() {
        acc.accumulate_ship(ship, content);
    }
    acc.finalize(state)
}

// ---------------------------------------------------------------------------
// Accumulator
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MetricsAccumulator {
    inv: InventoryAccumulator,

    station_storage_sum: f32,
    station_count: u32,

    per_module_metrics: BTreeMap<String, ModuleStatusMetrics>,

    wear_sum: f32,
    wear_count: u32,
    max_wear: f32,
    total_repair_kits: u32,
    total_thruster_count: u32,

    power_generated_kw: f32,
    power_consumed_kw: f32,
    power_deficit_kw: f32,
    battery_stored_kwh: f32,
    battery_capacity_kwh: f32,

    thermal_max_temp_mk: u32,
    thermal_temp_sum: u64,
    thermal_module_count: u32,
    overheat_warning_count: u32,
    overheat_critical_count: u32,
    heat_wear_multiplier_sum: f32,

    fleet_total: u32,
    fleet_idle: u32,
    fleet_mining: u32,
    fleet_transiting: u32,
    fleet_surveying: u32,
    fleet_depositing: u32,
    fleet_refueling: u32,
    fleet_propellant_sum: f32,
    fleet_propellant_capacity_sum: f32,
    ship_cargo_sum: f32,
    ship_count: u32,
}

impl MetricsAccumulator {
    fn new() -> Self {
        Self {
            inv: InventoryAccumulator::default(),
            ..Default::default()
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn accumulate_station(&mut self, station: &crate::StationState, content: &GameContent) {
        self.inv.accumulate(&station.inventory);

        let volume_used = inventory_volume_m3(&station.inventory, content);
        if station.cargo_capacity_m3 > 0.0 {
            self.station_storage_sum += volume_used / station.cargo_capacity_m3;
        }
        self.station_count += 1;

        let total_ore_at_station: f32 = station
            .inventory
            .iter()
            .filter(|item| item.is_ore())
            .map(InventoryItem::mass_kg)
            .sum();

        for module in &station.modules {
            self.accumulate_module(module, content, total_ore_at_station);
        }

        self.power_generated_kw += station.power.generated_kw;
        self.power_consumed_kw += station.power.consumed_kw;
        self.power_deficit_kw += station.power.deficit_kw;
        self.battery_stored_kwh += station.power.battery_stored_kwh;

        for module in &station.modules {
            if let Some(def) = content.module_defs.get(&module.def_id) {
                if let ModuleBehaviorDef::Battery(battery_def) = &def.behavior {
                    self.battery_capacity_kwh += battery_def.capacity_kwh;
                }
            }
        }

        for item in &station.inventory {
            if let InventoryItem::Component {
                component_id,
                count,
                ..
            } = item
            {
                if component_id.0 == crate::COMPONENT_REPAIR_KIT {
                    self.total_repair_kits += *count;
                }
                if component_id.0 == crate::COMPONENT_THRUSTER {
                    self.total_thruster_count += *count;
                }
            }
        }
    }

    fn accumulate_module(
        &mut self,
        module: &crate::ModuleState,
        content: &GameContent,
        total_ore_at_station: f32,
    ) {
        let Some(def) = content.module_defs.get(&module.def_id) else {
            return;
        };

        if matches!(
            def.behavior,
            ModuleBehaviorDef::Processor(_) | ModuleBehaviorDef::Assembler(_)
        ) {
            self.wear_sum += module.wear.wear;
            self.wear_count += 1;
            if module.wear.wear > self.max_wear {
                self.max_wear = module.wear.wear;
            }
        }

        if let Some(thermal) = &module.thermal {
            self.thermal_module_count += 1;
            self.thermal_temp_sum += u64::from(thermal.temp_mk);
            if thermal.temp_mk > self.thermal_max_temp_mk {
                self.thermal_max_temp_mk = thermal.temp_mk;
            }
            match thermal.overheat_zone {
                crate::OverheatZone::Warning => self.overheat_warning_count += 1,
                crate::OverheatZone::Critical | crate::OverheatZone::Damage => {
                    self.overheat_critical_count += 1;
                }
                crate::OverheatZone::Nominal => {}
            }
            self.heat_wear_multiplier_sum +=
                crate::thermal::heat_wear_multiplier(thermal.overheat_zone, &content.constants);
        }

        if !module.enabled {
            return;
        }

        let entry = self
            .per_module_metrics
            .entry(def.behavior.type_name().to_string())
            .or_default();
        entry.active += 1;
        if module.kind_state.is_stalled() {
            entry.stalled += 1;
        }
        // Starved is Processor-specific: ore supply below threshold.
        if let ModuleKindState::Processor(ps) = &module.kind_state {
            if total_ore_at_station < ps.threshold_kg {
                entry.starved += 1;
            }
        }
    }

    fn accumulate_ship(&mut self, ship: &crate::ShipState, content: &GameContent) {
        self.fleet_total += 1;
        self.inv.accumulate(&ship.inventory);

        let volume_used = inventory_volume_m3(&ship.inventory, content);
        if ship.cargo_capacity_m3 > 0.0 {
            self.ship_cargo_sum += volume_used / ship.cargo_capacity_m3;
        }
        self.ship_count += 1;

        self.fleet_propellant_sum += ship.propellant_kg;
        self.fleet_propellant_capacity_sum += ship.propellant_capacity_kg;

        match ship.task.as_ref().map(|t| &t.kind) {
            None | Some(TaskKind::Idle) => self.fleet_idle += 1,
            Some(TaskKind::Mine { .. }) => self.fleet_mining += 1,
            Some(TaskKind::Transit { .. }) => self.fleet_transiting += 1,
            Some(TaskKind::Survey { .. } | TaskKind::DeepScan { .. }) => {
                self.fleet_surveying += 1;
            }
            Some(TaskKind::Deposit { .. }) => self.fleet_depositing += 1,
            Some(TaskKind::Refuel { .. }) => self.fleet_refueling += 1,
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn compute_averages(&self) -> Averages {
        let per_element_ore_stats: BTreeMap<String, OreElementStats> = self
            .inv
            .ore_element_weighted_sums
            .keys()
            .map(|element| {
                let avg = safe_div(
                    self.inv.ore_element_weighted_sums[element],
                    self.inv.ore_total_weight,
                );
                let min = if self.inv.ore_lot_count > 0 {
                    self.inv
                        .ore_element_min
                        .get(element)
                        .copied()
                        .unwrap_or(0.0)
                } else {
                    0.0
                };
                let max = if self.inv.ore_lot_count > 0 {
                    self.inv
                        .ore_element_max
                        .get(element)
                        .copied()
                        .unwrap_or(0.0)
                } else {
                    0.0
                };
                (
                    element.clone(),
                    OreElementStats {
                        avg_fraction: avg,
                        min_fraction: min,
                        max_fraction: max,
                    },
                )
            })
            .collect();

        Averages {
            per_element_ore_stats,
            avg_material_quality: safe_div(
                self.inv.material_quality_weighted_sum,
                self.inv.material_total_weight,
            ),
            station_storage_used_pct: safe_div(self.station_storage_sum, self.station_count as f32),
            ship_cargo_used_pct: safe_div(self.ship_cargo_sum, self.ship_count as f32),
            avg_module_wear: safe_div(self.wear_sum, self.wear_count as f32),
            battery_charge_pct: safe_div(self.battery_stored_kwh, self.battery_capacity_kwh),
            station_avg_temp_mk: if self.thermal_module_count > 0 {
                (self.thermal_temp_sum / u64::from(self.thermal_module_count)) as u32
            } else {
                0
            },
            heat_wear_multiplier_avg: safe_div(
                self.heat_wear_multiplier_sum,
                self.thermal_module_count as f32,
            ),
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn finalize(self, state: &GameState) -> MetricsSnapshot {
        let avgs = self.compute_averages();

        let asteroids_depleted = state
            .asteroids
            .values()
            .filter(|a| a.mass_kg <= 0.0)
            .count() as u32;
        let total_scan_data = state
            .research
            .data_pool
            .get(&crate::DataKind::SurveyData)
            .copied()
            .unwrap_or(0.0);
        let max_tech_evidence = state
            .research
            .evidence
            .values()
            .flat_map(|dp| dp.points.values())
            .copied()
            .fold(0.0_f32, f32::max);

        MetricsSnapshot {
            tick: state.meta.tick,
            metrics_version: METRICS_VERSION,
            total_ore_kg: self.inv.total_ore_kg,
            total_material_kg: self.inv.total_material_kg,
            total_slag_kg: self.inv.total_slag_kg,
            per_element_material_kg: self.inv.per_element_material_kg,
            station_storage_used_pct: avgs.station_storage_used_pct,
            ship_cargo_used_pct: avgs.ship_cargo_used_pct,
            per_element_ore_stats: avgs.per_element_ore_stats,
            ore_lot_count: self.inv.ore_lot_count,
            avg_material_quality: avgs.avg_material_quality,
            per_module_metrics: self.per_module_metrics,
            fleet_total: self.fleet_total,
            fleet_idle: self.fleet_idle,
            fleet_mining: self.fleet_mining,
            fleet_transiting: self.fleet_transiting,
            fleet_surveying: self.fleet_surveying,
            fleet_depositing: self.fleet_depositing,
            fleet_refueling: self.fleet_refueling,
            fleet_propellant_kg: self.fleet_propellant_sum,
            fleet_propellant_pct: if self.fleet_propellant_capacity_sum > 0.0 {
                self.fleet_propellant_sum / self.fleet_propellant_capacity_sum
            } else {
                0.0
            },
            #[allow(clippy::cast_possible_truncation)]
            propellant_consumed_total: state.propellant_consumed_total as f32,
            scan_sites_remaining: state.scan_sites.len() as u32,
            asteroids_discovered: state.asteroids.len() as u32,
            asteroids_depleted,
            techs_unlocked: state.research.unlocked.len() as u32,
            total_scan_data,
            max_tech_evidence,
            avg_module_wear: avgs.avg_module_wear,
            max_module_wear: self.max_wear,
            repair_kits_remaining: self.total_repair_kits,
            balance: state.balance,
            thruster_count: self.total_thruster_count,
            export_revenue_total: state.export_revenue_total,
            export_count: state.export_count,
            power_generated_kw: self.power_generated_kw,
            power_consumed_kw: self.power_consumed_kw,
            power_deficit_kw: self.power_deficit_kw,
            battery_charge_pct: avgs.battery_charge_pct,
            station_max_temp_mk: self.thermal_max_temp_mk,
            station_avg_temp_mk: avgs.station_avg_temp_mk,
            overheat_warning_count: self.overheat_warning_count,
            overheat_critical_count: self.overheat_critical_count,
            heat_wear_multiplier_avg: avgs.heat_wear_multiplier_avg,
        }
    }
}

/// Divide numerator by denominator, returning 0.0 when denominator is zero.
fn safe_div(numerator: f32, denominator: f32) -> f32 {
    if denominator > 0.0 {
        numerator / denominator
    } else {
        0.0
    }
}

struct Averages {
    per_element_ore_stats: BTreeMap<String, OreElementStats>,
    avg_material_quality: f32,
    station_storage_used_pct: f32,
    ship_cargo_used_pct: f32,
    avg_module_wear: f32,
    battery_charge_pct: f32,
    station_avg_temp_mk: u32,
    heat_wear_multiplier_avg: f32,
}

/// Write the CSV header row for metrics. `element_ids` defines the dynamic
/// per-element columns (`material_kg_X`, `ore_avg_X`, `ore_min_X`, `ore_max_X`).
///
/// Column order (v11): fixed scalar fields, then per-element columns,
/// then per-module-type columns (`{type}_active`, `{type}_stalled`, `{type}_starved`).
pub fn write_metrics_header(
    writer: &mut impl std::io::Write,
    element_ids: &[String],
    behavior_types: &[String],
) -> std::io::Result<()> {
    let descriptors = MetricsSnapshot::fixed_field_descriptors();
    for (index, (name, _)) in descriptors.iter().enumerate() {
        if index > 0 {
            write!(writer, ",")?;
        }
        write!(writer, "{name}")?;
    }
    for eid in element_ids {
        write!(writer, ",material_kg_{eid}")?;
    }
    for eid in element_ids {
        write!(writer, ",ore_avg_{eid},ore_min_{eid},ore_max_{eid}")?;
    }
    for bt in behavior_types {
        write!(writer, ",{bt}_active,{bt}_stalled,{bt}_starved")?;
    }
    writeln!(writer)
}

/// Append a single metrics snapshot as a CSV row.
///
/// Uses [`MetricsSnapshot::fixed_field_values`] to iterate scalar fields,
/// then appends dynamic per-element and per-module-type columns.
pub fn append_metrics_row(
    writer: &mut impl std::io::Write,
    snapshot: &MetricsSnapshot,
    element_ids: &[String],
    behavior_types: &[String],
) -> std::io::Result<()> {
    let values = snapshot.fixed_field_values();
    for (index, (_, value)) in values.iter().enumerate() {
        if index > 0 {
            write!(writer, ",")?;
        }
        value.write_csv(writer)?;
    }
    for eid in element_ids {
        let val = snapshot
            .per_element_material_kg
            .get(eid)
            .copied()
            .unwrap_or(0.0);
        write!(writer, ",{val}")?;
    }
    for eid in element_ids {
        let stats = snapshot
            .per_element_ore_stats
            .get(eid)
            .cloned()
            .unwrap_or_default();
        write!(
            writer,
            ",{},{},{}",
            stats.avg_fraction, stats.min_fraction, stats.max_fraction
        )?;
    }
    for bt in behavior_types {
        let metrics = snapshot.per_module_metrics.get(bt);
        let active = metrics.map_or(0, |m| m.active);
        let stalled = metrics.map_or(0, |m| m.stalled);
        let starved = metrics.map_or(0, |m| m.starved);
        write!(writer, ",{active},{stalled},{starved}")?;
    }
    writeln!(writer)
}

/// Write a collection of snapshots to a CSV file.
pub fn write_metrics_csv(
    path: &str,
    snapshots: &[MetricsSnapshot],
    element_ids: &[String],
    behavior_types: &[String],
) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    write_metrics_header(&mut file, element_ids, behavior_types)?;
    for snapshot in snapshots {
        append_metrics_row(&mut file, snapshot, element_ids, behavior_types)?;
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
    element_ids: Vec<String>,
    behavior_types: Vec<String>,
}

impl MetricsFileWriter {
    /// Create a new writer, opening the first CSV file with a header row.
    pub fn new(
        run_dir: std::path::PathBuf,
        element_ids: Vec<String>,
        behavior_types: Vec<String>,
    ) -> std::io::Result<Self> {
        let (writer, _) = open_csv_file(&run_dir, 0, &element_ids, &behavior_types)?;
        Ok(Self {
            run_dir,
            file_index: 0,
            rows_in_current_file: 0,
            writer,
            element_ids,
            behavior_types,
        })
    }

    /// Append one snapshot row, rotating to a new file if the current one is full.
    pub fn write_row(&mut self, snapshot: &MetricsSnapshot) -> std::io::Result<()> {
        if self.rows_in_current_file >= MAX_ROWS_PER_FILE {
            self.writer.flush()?;
            self.file_index += 1;
            let (new_writer, _) = open_csv_file(
                &self.run_dir,
                self.file_index,
                &self.element_ids,
                &self.behavior_types,
            )?;
            self.writer = new_writer;
            self.rows_in_current_file = 0;
        }
        append_metrics_row(
            &mut self.writer,
            snapshot,
            &self.element_ids,
            &self.behavior_types,
        )?;
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
    element_ids: &[String],
    behavior_types: &[String],
) -> std::io::Result<(std::io::BufWriter<std::fs::File>, std::path::PathBuf)> {
    let name = format!("metrics_{index:03}.csv");
    let path = run_dir.join(&name);
    let file = std::fs::File::create(&path)?;
    let mut writer = std::io::BufWriter::new(file);
    write_metrics_header(&mut writer, element_ids, behavior_types)?;
    Ok((writer, path))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        test_fixtures::{base_content, test_position, ModuleDefBuilder},
        AHashMap, AsteroidId, AsteroidKnowledge, AsteroidState, Counters, DataKind, DomainProgress,
        GameState, HullId, LotId, MetaState, ModuleInstanceId, ModuleState, PrincipalId,
        ProcessorState, ResearchDomain, ResearchState, ShipId, ShipState, StationId, StationState,
        TaskState, TechId,
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
            asteroids: std::collections::BTreeMap::new(),
            ships: std::collections::BTreeMap::new(),
            stations: std::collections::BTreeMap::new(),
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
            propellant_consumed_total: 0.0,
            body_cache: AHashMap::default(),
        }
    }

    fn make_station(inventory: Vec<InventoryItem>, modules: Vec<ModuleState>) -> StationState {
        StationState {
            id: StationId("station_0001".to_string()),
            position: test_position(),
            inventory,
            cargo_capacity_m3: 10_000.0,
            power_available_per_tick: 100.0,
            modules,
            modifiers: crate::modifiers::ModifierSet::default(),
            crew: Default::default(),
            leaders: Vec::new(),
            thermal_links: Vec::new(),
            power: crate::PowerState::default(),
            cached_inventory_volume_m3: None,
            module_type_index: crate::ModuleTypeIndex::default(),
            module_id_index: HashMap::new(),
            power_budget_cache: crate::PowerBudgetCache::default(),
        }
    }

    fn make_ship(task: Option<TaskState>) -> ShipState {
        ShipState {
            id: ShipId("ship_0001".to_string()),
            position: test_position(),
            owner: PrincipalId("principal_autopilot".to_string()),
            inventory: vec![],
            cargo_capacity_m3: 20.0,
            task,
            speed_ticks_per_au: None,
            modifiers: crate::modifiers::ModifierSet::default(),
            hull_id: HullId("hull_general_purpose".to_string()),
            fitted_modules: vec![],
            propellant_kg: 0.0,
            propellant_capacity_kg: 0.0,
            crew: Default::default(),
            leaders: Vec::new(),
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
        assert_eq!(
            snapshot
                .per_element_material_kg
                .get("Fe")
                .copied()
                .unwrap_or(0.0),
            0.0
        );
        assert_eq!(snapshot.station_storage_used_pct, 0.0);
        assert_eq!(snapshot.ship_cargo_used_pct, 0.0);
        assert!(snapshot.per_element_ore_stats.is_empty());
        assert_eq!(snapshot.ore_lot_count, 0);
        assert_eq!(snapshot.avg_material_quality, 0.0);
        assert!(snapshot.per_module_metrics.get("processor").is_none());
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
        assert_eq!(snapshot.export_revenue_total, 0.0);
        assert_eq!(snapshot.export_count, 0);
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
        let fe_stats = &snapshot.per_element_ore_stats["Fe"];
        assert!((fe_stats.avg_fraction - 0.7).abs() < 1e-5);
        assert!((fe_stats.min_fraction - 0.7).abs() < 1e-5);
        assert!((fe_stats.max_fraction - 0.7).abs() < 1e-5);
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
        assert!((snapshot.per_element_material_kg["Fe"] - 500.0).abs() < 1e-3);
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
                    destination: test_position(),
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
        content.module_defs = [(
            "module_basic_iron_refinery".to_string(),
            ModuleDefBuilder::new("module_basic_iron_refinery")
                .name("Basic Iron Refinery")
                .mass(5000.0)
                .volume(10.0)
                .power(10.0)
                .wear(0.01)
                .behavior(ModuleBehaviorDef::Processor(crate::ProcessorDef {
                    processing_interval_minutes: 60,
                    processing_interval_ticks: 60,
                    recipes: vec![],
                }))
                .build(),
        )]
        .into_iter()
        .collect();

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
                    selected_recipe: None,
                }),
                wear: crate::WearState::default(),
                power_stalled: false,
                module_priority: 0,
                assigned_crew: Default::default(),
                crew_satisfied: true,
                thermal: None,
            }],
        );
        state.stations.insert(station.id.clone(), station);

        let snapshot = compute_metrics(&state, &content);

        let proc = snapshot.per_module_metrics.get("processor").unwrap();
        assert_eq!(proc.active, 1);
        assert_eq!(proc.starved, 1);
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
                position: test_position(),
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
            snapshot_a.per_element_ore_stats,
            snapshot_b.per_element_ore_stats
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
        state.research.data_pool.insert(DataKind::SurveyData, 42.5);
        state.research.evidence.insert(
            TechId("tech_c".to_string()),
            DomainProgress {
                points: HashMap::from([(ResearchDomain::Survey, 15.0)]),
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
        content.module_defs = [(
            "module_basic_iron_refinery".to_string(),
            ModuleDefBuilder::new("module_basic_iron_refinery")
                .name("Basic Iron Refinery")
                .mass(5000.0)
                .volume(10.0)
                .power(10.0)
                .wear(0.01)
                .behavior(ModuleBehaviorDef::Processor(crate::ProcessorDef {
                    processing_interval_minutes: 60,
                    processing_interval_ticks: 60,
                    recipes: vec![],
                }))
                .build(),
        )]
        .into_iter()
        .collect();

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
                    selected_recipe: None,
                }),
                wear: crate::WearState::default(),
                power_stalled: false,
                module_priority: 0,
                assigned_crew: Default::default(),
                crew_satisfied: true,
                thermal: None,
            }],
        );
        state.stations.insert(station.id.clone(), station);

        let snapshot = compute_metrics(&state, &content);

        let proc = snapshot.per_module_metrics.get("processor").unwrap();
        assert_eq!(proc.active, 1);
        assert_eq!(proc.stalled, 1);
        // Not starved (1000kg ore > 500kg threshold)
        assert_eq!(proc.starved, 0);
    }

    #[test]
    fn test_exploration_metrics() {
        let content = empty_content();
        let mut state = empty_state();

        state.scan_sites.push(crate::ScanSite {
            id: crate::SiteId("site_0001".to_string()),
            position: test_position(),
            template_id: "tmpl_iron_rich".to_string(),
        });
        state.scan_sites.push(crate::ScanSite {
            id: crate::SiteId("site_0002".to_string()),
            position: test_position(),
            template_id: "tmpl_iron_rich".to_string(),
        });

        state.asteroids.insert(
            AsteroidId("ast_0001".to_string()),
            AsteroidState {
                id: AsteroidId("ast_0001".to_string()),
                position: test_position(),
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
                position: test_position(),
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
        content.module_defs = [(
            "module_basic_iron_refinery".to_string(),
            ModuleDefBuilder::new("module_basic_iron_refinery")
                .name("Basic Iron Refinery")
                .mass(5000.0)
                .volume(10.0)
                .power(10.0)
                .wear(0.01)
                .behavior(ModuleBehaviorDef::Processor(crate::ProcessorDef {
                    processing_interval_minutes: 60,
                    processing_interval_ticks: 60,
                    recipes: vec![],
                }))
                .build(),
        )]
        .into_iter()
        .collect();

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
                        selected_recipe: None,
                    }),
                    wear: crate::WearState { wear: 0.3 },
                    power_stalled: false,
                    module_priority: 0,
                    assigned_crew: Default::default(),
                    crew_satisfied: true,
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
                        selected_recipe: None,
                    }),
                    wear: crate::WearState { wear: 0.7 },
                    power_stalled: false,
                    module_priority: 0,
                    assigned_crew: Default::default(),
                    crew_satisfied: true,
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
            ModuleDefBuilder::new("module_basic_battery")
                .name("Basic Battery")
                .mass(2000.0)
                .volume(4.0)
                .behavior(ModuleBehaviorDef::Battery(crate::BatteryDef {
                    capacity_kwh: 100.0,
                    charge_rate_kw: 20.0,
                    discharge_rate_kw: 30.0,
                }))
                .build(),
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
            module_priority: 0,
            assigned_crew: Default::default(),
            crew_satisfied: true,
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
            ModuleDefBuilder::new("module_smelter")
                .name("Smelter")
                .mass(5000.0)
                .volume(10.0)
                .power(10.0)
                .wear(0.01)
                .behavior(ModuleBehaviorDef::Processor(crate::ProcessorDef {
                    processing_interval_minutes: 60,
                    processing_interval_ticks: 60,
                    recipes: vec![],
                }))
                .thermal(crate::ThermalDef {
                    heat_capacity_j_per_k: 500.0,
                    passive_cooling_coefficient: 0.0,
                    max_temp_mk: 2_000_000,
                    operating_min_mk: None,
                    operating_max_mk: None,
                    thermal_group: None,
                    idle_heat_generation_w: None,
                })
                .build(),
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
                        selected_recipe: None,
                    }),
                    wear: crate::WearState::default(),
                    power_stalled: false,
                    module_priority: 0,
                    assigned_crew: Default::default(),
                    crew_satisfied: true,
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
                        selected_recipe: None,
                    }),
                    wear: crate::WearState::default(),
                    power_stalled: false,
                    module_priority: 0,
                    assigned_crew: Default::default(),
                    crew_satisfied: true,
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
                        selected_recipe: None,
                    }),
                    wear: crate::WearState::default(),
                    power_stalled: false,
                    module_priority: 0,
                    assigned_crew: Default::default(),
                    crew_satisfied: true,
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
            ModuleDefBuilder::new("module_basic_iron_refinery")
                .name("Basic Iron Refinery")
                .mass(5000.0)
                .volume(10.0)
                .power(10.0)
                .wear(0.01)
                .behavior(ModuleBehaviorDef::Processor(crate::ProcessorDef {
                    processing_interval_minutes: 60,
                    processing_interval_ticks: 60,
                    recipes: vec![],
                }))
                .build(),
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
                    selected_recipe: None,
                }),
                wear: crate::WearState::default(),
                power_stalled: false,
                module_priority: 0,
                assigned_crew: Default::default(),
                crew_satisfied: true,
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

    #[test]
    fn fixed_field_descriptors_covers_all_scalar_fields() {
        // Serialize a snapshot to JSON to count fields via serde.
        let snapshot = compute_metrics(&empty_state(), &empty_content());
        let value = serde_json::to_value(&snapshot).unwrap();
        let obj = value.as_object().unwrap();

        // Count scalar fields (exclude dynamic map fields).
        let scalar_count = obj
            .keys()
            .filter(|k| !k.starts_with("per_element") && *k != "per_module_metrics")
            .count();
        let descriptor_count = MetricsSnapshot::fixed_field_descriptors().len();

        assert_eq!(
            descriptor_count, scalar_count,
            "fixed_field_descriptors() has {descriptor_count} entries but MetricsSnapshot \
             has {scalar_count} scalar fields. Did you add a field to the struct but forget \
             to add it to fixed_field_values()?"
        );
    }

    #[test]
    fn fixed_field_values_names_match_descriptors() {
        let snapshot = compute_metrics(&empty_state(), &empty_content());
        let values = snapshot.fixed_field_values();
        let descriptors = MetricsSnapshot::fixed_field_descriptors();

        assert_eq!(values.len(), descriptors.len());
        for ((vname, val), (dname, dtype)) in values.iter().zip(descriptors.iter()) {
            assert_eq!(vname, dname, "name mismatch at field");
            assert_eq!(val.metric_type(), *dtype, "type mismatch at field {vname}");
        }
    }

    #[test]
    fn get_field_f64_known_fields() {
        let snapshot = compute_metrics(&empty_state(), &empty_content());
        assert!(snapshot.get_field_f64("tick").is_some());
        assert!(snapshot.get_field_f64("balance").is_some());
        assert!(snapshot.get_field_f64("heat_wear_multiplier_avg").is_some());
        assert!(snapshot.get_field_f64("nonexistent_field").is_none());
    }

    #[test]
    fn get_field_f64_resolves_dynamic_module_metrics() {
        let mut snapshot = compute_metrics(&empty_state(), &empty_content());
        snapshot.per_module_metrics.insert(
            "processor".to_string(),
            ModuleStatusMetrics {
                active: 3,
                stalled: 1,
                starved: 2,
            },
        );
        // Normal resolution
        assert_eq!(snapshot.get_field_f64("processor_active"), Some(3.0));
        assert_eq!(snapshot.get_field_f64("processor_stalled"), Some(1.0));
        assert_eq!(snapshot.get_field_f64("processor_starved"), Some(2.0));

        // Underscore-containing type name (e.g., sensor_array)
        snapshot.per_module_metrics.insert(
            "sensor_array".to_string(),
            ModuleStatusMetrics {
                active: 5,
                stalled: 0,
                starved: 0,
            },
        );
        assert_eq!(snapshot.get_field_f64("sensor_array_active"), Some(5.0));

        // Unknown suffix
        assert!(snapshot.get_field_f64("processor_unknown").is_none());
        // Nonexistent type
        assert!(snapshot.get_field_f64("nonexistent_active").is_none());
    }
}
