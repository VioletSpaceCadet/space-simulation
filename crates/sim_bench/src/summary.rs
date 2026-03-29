use serde::Serialize;
use sim_core::MetricsSnapshot;

#[derive(Debug, Serialize)]
pub struct SummaryStats {
    pub seed_count: usize,
    pub collapsed_count: usize,
    pub metrics: Vec<MetricSummary>,
}

#[derive(Debug, Serialize)]
pub struct MetricSummary {
    pub name: String,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
    pub stddev: f64,
}

pub fn compute_summary(snapshots: &[(u64, &MetricsSnapshot)]) -> SummaryStats {
    let seed_count = snapshots.len();

    // A seed is "collapsed" if processor starved > 0 AND fleet_idle == fleet_total
    let collapsed_count = snapshots
        .iter()
        .filter(|(_, s)| {
            let starved = s
                .per_module_metrics
                .get("processor")
                .map_or(0, |m| m.starved);
            starved > 0 && s.fleet_idle == s.fleet_total
        })
        .count();

    // Curated summary fields — a subset of MetricsSnapshot meaningful for seed comparison.
    // Uses get_field_f64() for direct field lookups instead of per-field closures.
    let direct_fields: &[&str] = &[
        "station_storage_used_pct",
        "processor_starved",
        "techs_unlocked",
        "avg_module_wear",
        "repair_kits_remaining",
        "balance",
        "thruster_count",
        "export_revenue_total",
        "export_count",
        "power_generated_kw",
        "power_consumed_kw",
        "power_deficit_kw",
        "battery_charge_pct",
        "station_max_temp_mk",
        "station_avg_temp_mk",
        "overheat_warning_count",
        "overheat_critical_count",
        "heat_wear_multiplier_avg",
    ];

    let mut metrics: Vec<MetricSummary> = Vec::new();

    // Renamed alias: storage_saturation_pct → station_storage_used_pct
    metrics.push({
        let values: Vec<f64> = snapshots
            .iter()
            .map(|(_, s)| s.get_field_f64("station_storage_used_pct").unwrap_or(0.0))
            .collect();
        compute_metric_summary("storage_saturation_pct", &values)
    });

    // Composite: fleet_idle_pct = fleet_idle / fleet_total
    metrics.push({
        let values: Vec<f64> = snapshots
            .iter()
            .map(|(_, s)| {
                if s.fleet_total == 0 {
                    0.0
                } else {
                    f64::from(s.fleet_idle) / f64::from(s.fleet_total)
                }
            })
            .collect();
        compute_metric_summary("fleet_idle_pct", &values)
    });

    // Direct field extractions (skip storage_saturation_pct — already added above as alias)
    for field_name in &direct_fields[1..] {
        let values: Vec<f64> = snapshots
            .iter()
            .map(|(_, s)| s.get_field_f64(field_name).unwrap_or(0.0))
            .collect();
        metrics.push(compute_metric_summary(field_name, &values));
    }

    SummaryStats {
        seed_count,
        collapsed_count,
        metrics,
    }
}

fn compute_metric_summary(name: &str, values: &[f64]) -> MetricSummary {
    let count = values.len() as f64;
    let mean = values.iter().sum::<f64>() / count;
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count;
    let stddev = variance.sqrt();

    MetricSummary {
        name: name.to_string(),
        mean,
        min,
        max,
        stddev,
    }
}

/// Build aggregated metrics in the contract format:
/// `{ "key": { "mean": ..., "min": ..., "max": ..., "stddev": ... }, ... }`
///
/// Auto-generates entries for all fixed scalar fields (except `tick` and `metrics_version`)
/// using [`MetricsSnapshot::fixed_field_values`]. New fields added to `MetricsSnapshot`
/// automatically appear in the aggregated output.
pub fn build_aggregated_metrics(snapshots: &[&MetricsSnapshot]) -> serde_json::Value {
    let descriptors = MetricsSnapshot::fixed_field_descriptors();
    // Pre-extract field values once per snapshot to avoid O(fields * snapshots) allocations.
    let all_values: Vec<Vec<(&str, sim_core::MetricValue)>> =
        snapshots.iter().map(|s| s.fixed_field_values()).collect();

    let mut map = serde_json::Map::new();
    for (index, (name, _)) in descriptors.iter().enumerate() {
        if matches!(*name, "tick" | "metrics_version") {
            continue;
        }
        let values: Vec<f64> = all_values.iter().map(|fv| fv[index].1.as_f64()).collect();
        let summary = compute_metric_summary(name, &values);
        map.insert(
            name.to_string(),
            serde_json::json!({
                "mean": summary.mean,
                "min": summary.min,
                "max": summary.max,
                "stddev": summary.stddev,
            }),
        );
    }
    serde_json::Value::Object(map)
}

pub fn print_summary(scenario_name: &str, ticks: u64, stats: &SummaryStats) {
    let tick_display = if ticks >= 1000 {
        format!("{}k", ticks / 1000)
    } else {
        ticks.to_string()
    };
    println!(
        "\n=== {} ({} seeds, {} ticks each) ===\n",
        scenario_name, stats.seed_count, tick_display
    );
    println!(
        "{:<30} {:>8} {:>8} {:>8} {:>8}",
        "Metric", "Mean", "Min", "Max", "StdDev"
    );
    println!("{}", "-".repeat(70));
    for metric in &stats.metrics {
        println!(
            "{:<30} {:>8.2} {:>8.2} {:>8.2} {:>8.2}",
            metric.name, metric.mean, metric.min, metric.max, metric.stddev
        );
    }
    println!(
        "{:<30} {}/{}",
        "collapse_rate", stats.collapsed_count, stats.seed_count
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    fn make_snapshot(
        tick: u64,
        storage_pct: f32,
        fleet_total: u32,
        fleet_idle: u32,
        refinery_starved: u32,
        techs: u32,
        avg_wear: f32,
        repair_kits: u32,
    ) -> MetricsSnapshot {
        MetricsSnapshot {
            tick,
            metrics_version: 3,
            total_ore_kg: 0.0,
            total_material_kg: 0.0,
            total_slag_kg: 0.0,
            per_element_material_kg: std::collections::BTreeMap::new(),
            station_storage_used_pct: storage_pct,
            ship_cargo_used_pct: 0.0,
            per_element_ore_stats: std::collections::BTreeMap::new(),
            ore_lot_count: 0,
            avg_material_quality: 0.0,
            per_module_metrics: {
                let mut m = std::collections::BTreeMap::new();
                if refinery_starved > 0 {
                    m.insert(
                        "processor".to_string(),
                        sim_core::ModuleStatusMetrics {
                            active: 0,
                            stalled: 0,
                            starved: refinery_starved,
                        },
                    );
                }
                m
            },
            fleet_total,
            fleet_idle,
            fleet_mining: 0,
            fleet_transiting: 0,
            fleet_surveying: 0,
            fleet_depositing: 0,
            fleet_refueling: 0,
            fleet_propellant_kg: 0.0,
            fleet_propellant_pct: 0.0,
            propellant_consumed_total: 0.0,
            scan_sites_remaining: 0,
            asteroids_discovered: 0,
            asteroids_depleted: 0,
            techs_unlocked: techs,
            total_scan_data: 0.0,
            max_tech_evidence: 0.0,
            avg_module_wear: avg_wear,
            max_module_wear: 0.0,
            repair_kits_remaining: repair_kits,
            balance: 0.0,
            crew_salary_per_hour: 0.0,
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
    fn test_summary_basic_stats() {
        let s1 = make_snapshot(100, 0.5, 2, 0, 0, 3, 0.2, 5);
        let s2 = make_snapshot(100, 0.7, 2, 0, 0, 5, 0.4, 3);
        let snapshots: Vec<(u64, &MetricsSnapshot)> = vec![(1, &s1), (2, &s2)];
        let stats = compute_summary(&snapshots);

        assert_eq!(stats.seed_count, 2);
        assert_eq!(stats.collapsed_count, 0);

        let storage = &stats.metrics[0];
        assert_eq!(storage.name, "storage_saturation_pct");
        assert!((storage.mean - 0.6).abs() < 1e-5);
        assert!((storage.min - 0.5).abs() < 1e-5);
        assert!((storage.max - 0.7).abs() < 1e-5);
    }

    #[test]
    fn test_collapse_detection() {
        // Collapsed: refinery_starved > 0 AND fleet_idle == fleet_total
        let collapsed = make_snapshot(100, 0.5, 2, 2, 1, 3, 0.2, 5);
        let healthy = make_snapshot(100, 0.5, 2, 0, 0, 3, 0.2, 5);
        let snapshots: Vec<(u64, &MetricsSnapshot)> = vec![(1, &collapsed), (2, &healthy)];
        let stats = compute_summary(&snapshots);

        assert_eq!(stats.collapsed_count, 1);
    }

    #[test]
    fn test_stddev_zero_for_identical() {
        let s1 = make_snapshot(100, 0.5, 2, 1, 0, 3, 0.2, 5);
        let s2 = make_snapshot(100, 0.5, 2, 1, 0, 3, 0.2, 5);
        let snapshots: Vec<(u64, &MetricsSnapshot)> = vec![(1, &s1), (2, &s2)];
        let stats = compute_summary(&snapshots);

        for metric in &stats.metrics {
            assert!(
                metric.stddev.abs() < 1e-10,
                "stddev for {} should be 0, got {}",
                metric.name,
                metric.stddev
            );
        }
    }

    #[test]
    fn test_build_aggregated_metrics_has_all_keys() {
        let s1 = make_snapshot(100, 0.5, 2, 0, 0, 3, 0.2, 5);
        let s2 = make_snapshot(100, 0.7, 2, 1, 1, 5, 0.4, 3);
        let snapshots: Vec<&MetricsSnapshot> = vec![&s1, &s2];
        let agg = build_aggregated_metrics(&snapshots);

        let obj = agg.as_object().unwrap();
        // Dynamically derive expected keys from fixed_field_descriptors, minus tick/metrics_version
        let expected_keys: Vec<&str> = MetricsSnapshot::fixed_field_descriptors()
            .iter()
            .map(|(name, _)| *name)
            .filter(|name| !matches!(*name, "tick" | "metrics_version"))
            .collect();
        assert_eq!(obj.len(), expected_keys.len());
        for key in &expected_keys {
            let entry = obj
                .get(*key)
                .unwrap_or_else(|| panic!("missing key: {key}"));
            assert!(entry.get("mean").is_some(), "missing mean for {key}");
            assert!(entry.get("min").is_some(), "missing min for {key}");
            assert!(entry.get("max").is_some(), "missing max for {key}");
            assert!(entry.get("stddev").is_some(), "missing stddev for {key}");
        }
    }

    #[test]
    fn test_build_aggregated_metrics_values() {
        let s1 = make_snapshot(100, 0.5, 4, 1, 0, 3, 0.2, 5);
        let s2 = make_snapshot(100, 0.7, 6, 3, 0, 5, 0.4, 3);
        let snapshots: Vec<&MetricsSnapshot> = vec![&s1, &s2];
        let agg = build_aggregated_metrics(&snapshots);

        let fleet_total = &agg["fleet_total"];
        assert!((fleet_total["mean"].as_f64().unwrap() - 5.0).abs() < 1e-5);
        assert!((fleet_total["min"].as_f64().unwrap() - 4.0).abs() < 1e-5);
        assert!((fleet_total["max"].as_f64().unwrap() - 6.0).abs() < 1e-5);
    }
}
