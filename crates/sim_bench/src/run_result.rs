use serde::Serialize;
use sim_core::MetricsSnapshot;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct RunResult {
    pub run_schema_version: u32,
    pub run_status: String,
    pub run_id: String,
    pub git_sha: String,
    pub git_dirty: bool,
    pub seed: u64,
    pub scenario_name: String,
    pub scenario_params: serde_json::Value,
    pub tick_start: u64,
    pub tick_end: u64,
    pub total_ticks: u64,
    pub wall_time_ms: u64,
    pub sim_ticks_per_second: f64,
    pub summary_metrics: Option<SummaryMetrics>,
    pub alert_counts_by_type: HashMap<String, u64>,
    pub alert_first_tick_by_type: HashMap<String, u64>,
    pub alert_last_tick_by_type: HashMap<String, u64>,
    pub collapse_occurred: bool,
    pub collapse_tick: Option<u64>,
    pub collapse_reason: Option<String>,
    pub metrics_path: String,
    pub alerts_path: Option<String>,
    pub events_path: Option<String>,
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing_stats: Option<TimingStats>,
}

#[derive(Debug, Serialize)]
pub struct SummaryMetrics {
    pub total_ore_kg: f64,
    pub total_material_kg: f64,
    pub total_slag_kg: f64,
    pub station_storage_used_pct: f64,
    pub fleet_total: u32,
    pub fleet_idle: u32,
    pub per_module_metrics: std::collections::BTreeMap<String, sim_core::ModuleStatusMetrics>,
    pub avg_module_wear: f64,
    pub max_module_wear: f64,
    pub repair_kits_remaining: u32,
    pub techs_unlocked: u32,
    pub asteroids_discovered: u32,
    pub asteroids_depleted: u32,
    pub scan_sites_remaining: u32,
    pub balance: f64,
    pub thruster_count: u32,
    pub export_revenue_total: f64,
    pub export_count: u32,
    pub power_generated_kw: f64,
    pub power_consumed_kw: f64,
    pub power_deficit_kw: f64,
    pub battery_charge_pct: f64,
    pub station_max_temp_mk: u32,
    pub station_avg_temp_mk: u32,
    pub overheat_warning_count: u32,
    pub overheat_critical_count: u32,
    pub heat_wear_multiplier_avg: f64,
}

impl SummaryMetrics {
    pub fn from_snapshot(snapshot: &MetricsSnapshot) -> Self {
        Self {
            total_ore_kg: f64::from(snapshot.total_ore_kg),
            total_material_kg: f64::from(snapshot.total_material_kg),
            total_slag_kg: f64::from(snapshot.total_slag_kg),
            station_storage_used_pct: f64::from(snapshot.station_storage_used_pct),
            fleet_total: snapshot.fleet_total,
            fleet_idle: snapshot.fleet_idle,
            per_module_metrics: snapshot.per_module_metrics.clone(),
            avg_module_wear: f64::from(snapshot.avg_module_wear),
            max_module_wear: f64::from(snapshot.max_module_wear),
            repair_kits_remaining: snapshot.repair_kits_remaining,
            techs_unlocked: snapshot.techs_unlocked,
            asteroids_discovered: snapshot.asteroids_discovered,
            asteroids_depleted: snapshot.asteroids_depleted,
            scan_sites_remaining: snapshot.scan_sites_remaining,
            balance: snapshot.balance,
            thruster_count: snapshot.thruster_count,
            export_revenue_total: snapshot.export_revenue_total,
            export_count: snapshot.export_count,
            power_generated_kw: f64::from(snapshot.power_generated_kw),
            power_consumed_kw: f64::from(snapshot.power_consumed_kw),
            power_deficit_kw: f64::from(snapshot.power_deficit_kw),
            battery_charge_pct: f64::from(snapshot.battery_charge_pct),
            station_max_temp_mk: snapshot.station_max_temp_mk,
            station_avg_temp_mk: snapshot.station_avg_temp_mk,
            overheat_warning_count: snapshot.overheat_warning_count,
            overheat_critical_count: snapshot.overheat_critical_count,
            heat_wear_multiplier_avg: f64::from(snapshot.heat_wear_multiplier_avg),
        }
    }
}

/// Per-step timing summary stats across all ticks in a run.
#[derive(Debug, Clone, Serialize)]
pub struct TimingStats {
    pub steps: Vec<StepTimingEntry>,
}

/// Timing stats for a single tick step.
#[derive(Debug, Clone, Serialize)]
pub struct StepTimingEntry {
    pub name: String,
    pub mean_us: f64,
    pub p50_us: f64,
    pub p95_us: f64,
    pub max_us: f64,
}

impl RunResult {
    /// Write JSON atomically: write to `.tmp` then rename.
    pub fn write_atomic(&self, path: &Path) -> anyhow::Result<()> {
        let tmp_path = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(self)?;
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    }
}

/// Detect collapse: processor starved > 0 AND `fleet_idle` == `fleet_total`.
pub fn detect_collapse(snapshot: &MetricsSnapshot) -> (bool, Option<String>) {
    let processor_starved = snapshot
        .per_module_metrics
        .get("processor")
        .map_or(0, |m| m.starved);
    let collapsed = processor_starved > 0 && snapshot.fleet_idle == snapshot.fleet_total;
    if collapsed {
        (true, Some("refinery_starved + fleet_idle".to_string()))
    } else {
        (false, None)
    }
}

pub fn git_sha() -> String {
    env!("GIT_SHA").to_string()
}

pub fn git_dirty() -> bool {
    env!("GIT_DIRTY") == "true"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_snapshot() -> MetricsSnapshot {
        MetricsSnapshot {
            tick: 1000,
            metrics_version: 3,
            total_ore_kg: 500.0,
            total_material_kg: 200.0,
            total_slag_kg: 50.0,
            per_element_material_kg: std::collections::BTreeMap::new(),
            station_storage_used_pct: 0.35,
            ship_cargo_used_pct: 0.1,
            per_element_ore_stats: std::collections::BTreeMap::new(),
            ore_lot_count: 5,
            avg_material_quality: 0.75,
            per_module_metrics: {
                let mut m = std::collections::BTreeMap::new();
                m.insert(
                    "processor".to_string(),
                    sim_core::ModuleStatusMetrics {
                        active: 2,
                        stalled: 0,
                        starved: 0,
                    },
                );
                m.insert(
                    "assembler".to_string(),
                    sim_core::ModuleStatusMetrics {
                        active: 1,
                        stalled: 0,
                        starved: 0,
                    },
                );
                m
            },
            fleet_total: 3,
            fleet_idle: 1,
            fleet_mining: 1,
            fleet_transiting: 1,
            fleet_surveying: 0,
            fleet_depositing: 0,
            fleet_refueling: 0,
            scan_sites_remaining: 4,
            asteroids_discovered: 6,
            asteroids_depleted: 2,
            techs_unlocked: 3,
            total_scan_data: 100.0,
            max_tech_evidence: 25.0,
            avg_module_wear: 0.3,
            max_module_wear: 0.6,
            repair_kits_remaining: 5,
            balance: 0.0,
            thruster_count: 0,
            export_revenue_total: 0.0,
            export_count: 0,
            power_generated_kw: 0.0,
            power_consumed_kw: 0.0,
            power_deficit_kw: 0.0,
            battery_charge_pct: 0.0,
            station_max_temp_mk: 0,
            station_avg_temp_mk: 0,
            overheat_warning_count: 0,
            overheat_critical_count: 0,
            heat_wear_multiplier_avg: 0.0,
        }
    }

    #[test]
    fn test_summary_metrics_from_snapshot() {
        let snapshot = sample_snapshot();
        let metrics = SummaryMetrics::from_snapshot(&snapshot);
        assert!((metrics.total_ore_kg - 500.0).abs() < 1e-3);
        assert_eq!(metrics.fleet_total, 3);
        assert_eq!(metrics.techs_unlocked, 3);
        assert_eq!(metrics.scan_sites_remaining, 4);
    }

    #[test]
    fn test_run_result_round_trip_serialization() {
        let snapshot = sample_snapshot();
        let result = RunResult {
            run_schema_version: 1,
            run_status: "completed".to_string(),
            run_id: "test-uuid".to_string(),
            git_sha: "abc123".to_string(),
            git_dirty: false,
            seed: 42,
            scenario_name: "test_scenario".to_string(),
            scenario_params: serde_json::json!({"ticks": 1000}),
            tick_start: 0,
            tick_end: 1000,
            total_ticks: 1000,
            wall_time_ms: 500,
            sim_ticks_per_second: 2000.0,
            summary_metrics: Some(SummaryMetrics::from_snapshot(&snapshot)),
            alert_counts_by_type: HashMap::new(),
            alert_first_tick_by_type: HashMap::new(),
            alert_last_tick_by_type: HashMap::new(),
            collapse_occurred: false,
            collapse_tick: None,
            collapse_reason: None,
            metrics_path: "metrics_000.csv".to_string(),
            alerts_path: None,
            events_path: None,
            error_message: None,
            timing_stats: None,
        };

        let json = serde_json::to_string_pretty(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["run_schema_version"], 1);
        assert_eq!(parsed["run_status"], "completed");
        assert_eq!(parsed["seed"], 42);
        assert!(parsed["summary_metrics"]["total_ore_kg"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn test_atomic_write() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("run_result.json");
        let result = RunResult {
            run_schema_version: 1,
            run_status: "completed".to_string(),
            run_id: "test-uuid".to_string(),
            git_sha: "abc123".to_string(),
            git_dirty: false,
            seed: 42,
            scenario_name: "test".to_string(),
            scenario_params: serde_json::json!({}),
            tick_start: 0,
            tick_end: 100,
            total_ticks: 100,
            wall_time_ms: 50,
            sim_ticks_per_second: 2000.0,
            summary_metrics: None,
            alert_counts_by_type: HashMap::new(),
            alert_first_tick_by_type: HashMap::new(),
            alert_last_tick_by_type: HashMap::new(),
            collapse_occurred: false,
            collapse_tick: None,
            collapse_reason: None,
            metrics_path: "metrics_000.csv".to_string(),
            alerts_path: None,
            events_path: None,
            error_message: None,
            timing_stats: None,
        };

        result.write_atomic(&path).unwrap();
        assert!(path.exists());
        // Tmp file should not remain
        assert!(!path.with_extension("json.tmp").exists());

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["run_schema_version"], 1);
    }

    #[test]
    fn test_collapse_detection_healthy() {
        let snapshot = sample_snapshot();
        let (collapsed, reason) = detect_collapse(&snapshot);
        assert!(!collapsed);
        assert!(reason.is_none());
    }

    #[test]
    fn test_collapse_detection_collapsed() {
        let mut snapshot = sample_snapshot();
        snapshot
            .per_module_metrics
            .entry("processor".to_string())
            .or_default()
            .starved = 2;
        snapshot.fleet_idle = 3;
        snapshot.fleet_total = 3;
        let (collapsed, reason) = detect_collapse(&snapshot);
        assert!(collapsed);
        assert!(reason.is_some());
    }

    #[test]
    fn test_git_sha_not_empty() {
        // Build-time env vars should be set
        let sha = git_sha();
        assert!(!sha.is_empty());
    }
}
