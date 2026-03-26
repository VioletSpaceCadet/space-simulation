use sim_core::{AlertRuleDef, AlertRuleType, MetricsSnapshot};
use std::collections::{HashSet, VecDeque};

/// Alert detail returned by the advisor digest endpoint.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AlertDetail {
    pub id: String,
    pub severity: String,
    pub message: String,
    pub suggested_action: String,
}

// Metric field accessors use `MetricsSnapshot::get_field_f64()` directly —
// no per-field match arm needed here.

fn check_condition(value: f64, condition: &str, threshold: f64) -> bool {
    match condition {
        "gt" => value > threshold,
        "lt" => value < threshold,
        "gte" => value >= threshold,
        "lte" => value <= threshold,
        "eq" => (value - threshold).abs() < 1e-6,
        other => {
            tracing::warn!("unknown alert condition operator: {other}");
            false
        }
    }
}

// --- Helpers for querying recent snapshots ---

fn latest(h: &VecDeque<MetricsSnapshot>) -> Option<&MetricsSnapshot> {
    h.back()
}

fn tail(h: &VecDeque<MetricsSnapshot>, n: usize) -> Vec<&MetricsSnapshot> {
    h.iter().rev().take(n).collect()
}

fn max_f(snapshots: &[&MetricsSnapshot], f: fn(&MetricsSnapshot) -> f32) -> f32 {
    snapshots
        .iter()
        .map(|s| f(s))
        .fold(f32::NEG_INFINITY, f32::max)
}

fn min_f(snapshots: &[&MetricsSnapshot], f: fn(&MetricsSnapshot) -> f32) -> f32 {
    snapshots.iter().map(|s| f(s)).fold(f32::INFINITY, f32::min)
}

fn max_u(snapshots: &[&MetricsSnapshot], f: fn(&MetricsSnapshot) -> u32) -> u32 {
    snapshots.iter().map(|s| f(s)).max().unwrap_or(0)
}

fn min_u(snapshots: &[&MetricsSnapshot], f: fn(&MetricsSnapshot) -> u32) -> u32 {
    snapshots.iter().map(|s| f(s)).min().unwrap_or(0)
}

// --- Builtin rule evaluators ---

fn builtin_slag_backpressure(h: &VecDeque<MetricsSnapshot>) -> bool {
    let recent = tail(h, 5);
    if recent.len() < 2 {
        return false;
    }
    let slag_delta = max_f(&recent, |s| s.total_slag_kg) - min_f(&recent, |s| s.total_slag_kg);
    let mat_delta =
        max_f(&recent, |s| s.total_material_kg) - min_f(&recent, |s| s.total_material_kg);
    slag_delta > 10.0 && mat_delta < 1.0
}

fn builtin_ship_idle_with_work(h: &VecDeque<MetricsSnapshot>) -> bool {
    latest(h).is_some_and(|s| s.fleet_idle > 0)
}

fn builtin_throughput_drop(h: &VecDeque<MetricsSnapshot>) -> bool {
    let recent = tail(h, 10);
    let longer = tail(h, 50);
    if recent.len() < 2 || longer.len() < 2 {
        return false;
    }
    let recent_delta =
        max_f(&recent, |s| s.total_material_kg) - min_f(&recent, |s| s.total_material_kg);
    let longer_delta =
        max_f(&longer, |s| s.total_material_kg) - min_f(&longer, |s| s.total_material_kg);
    longer_delta > 0.0 && recent_delta < longer_delta * 0.5
}

fn builtin_exploration_stall(h: &VecDeque<MetricsSnapshot>) -> bool {
    let recent = tail(h, 10);
    if recent.len() < 2 {
        return false;
    }
    let discovered_unchanged =
        max_u(&recent, |s| s.asteroids_discovered) == min_u(&recent, |s| s.asteroids_discovered);
    let has_sites = max_u(&recent, |s| s.scan_sites_remaining) > 0;
    let has_idle = max_u(&recent, |s| s.fleet_idle) > 0;
    discovered_unchanged && has_sites && has_idle
}

fn builtin_research_stalled(h: &VecDeque<MetricsSnapshot>, total_techs: usize) -> bool {
    let recent = tail(h, 20);
    if recent.len() < 2 {
        return false;
    }
    let evidence_unchanged =
        (max_f(&recent, |s| s.max_tech_evidence) - min_f(&recent, |s| s.max_tech_evidence)).abs()
            < f32::EPSILON;
    #[allow(clippy::cast_possible_truncation)]
    let all_unlocked = max_u(&recent, |s| s.techs_unlocked) >= total_techs as u32;
    evidence_unchanged && !all_unlocked
}

fn builtin_overheat_warning(h: &VecDeque<MetricsSnapshot>) -> bool {
    tail(h, 5)
        .iter()
        .all(|s| s.overheat_warning_count > 0 || s.overheat_critical_count > 0)
        && h.len() >= 5
}

// --- AlertEngine ---

pub struct AlertEngine {
    rules: Vec<AlertRuleDef>,
    active: HashSet<String>,
    total_techs: usize,
}

impl AlertEngine {
    pub fn new(rules: &[AlertRuleDef], total_techs: usize) -> Self {
        Self {
            rules: rules.to_vec(),
            active: HashSet::new(),
            total_techs,
        }
    }

    /// Returns current active alert IDs (for the /api/v1/alerts endpoint).
    pub fn active_alert_ids(&self) -> Vec<String> {
        self.active.iter().cloned().collect()
    }

    /// Returns full details for all currently active alerts.
    pub fn active_alert_details(&self) -> Vec<AlertDetail> {
        self.rules
            .iter()
            .filter(|rule| self.active.contains(&rule.id))
            .map(|rule| AlertDetail {
                id: rule.id.clone(),
                severity: format!("{:?}", rule.severity),
                message: rule.message.clone(),
                suggested_action: rule.suggested_action.clone(),
            })
            .collect()
    }

    /// Evaluate a single rule against the metrics history.
    fn evaluate_rule(
        rule: &AlertRuleDef,
        history: &VecDeque<MetricsSnapshot>,
        total_techs: usize,
    ) -> bool {
        match &rule.rule {
            AlertRuleType::ThresholdLatest {
                metric,
                condition,
                threshold,
            } => latest(history).is_some_and(|snapshot| {
                let value = snapshot.get_field_f64(metric);
                if value.is_none() {
                    tracing::warn!("unknown metric field in alert rule: {metric}");
                }
                value.is_some_and(|v| check_condition(v, condition, *threshold))
            }),

            AlertRuleType::ThresholdLatestElement {
                element,
                condition,
                threshold,
                min_value,
            } => latest(history).is_some_and(|snapshot| {
                let value = f64::from(
                    snapshot
                        .per_element_material_kg
                        .get(element.as_str())
                        .copied()
                        .unwrap_or(0.0),
                );
                // If min_value is set, the value must exceed it (for "between" checks)
                if let Some(min_val) = min_value {
                    if value <= *min_val {
                        return false;
                    }
                }
                check_condition(value, condition, *threshold)
            }),

            AlertRuleType::Consecutive {
                metric,
                min_samples,
            } => {
                let n = *min_samples as usize;
                let recent = tail(history, n);
                recent.len() >= n
                    && recent
                        .iter()
                        .all(|snapshot| snapshot.get_field_f64(metric).is_some_and(|v| v > 0.0))
            }

            AlertRuleType::Builtin { name } => match name.as_str() {
                "slag_backpressure" => builtin_slag_backpressure(history),
                "ship_idle_with_work" => builtin_ship_idle_with_work(history),
                "throughput_drop" => builtin_throughput_drop(history),
                "exploration_stall" => builtin_exploration_stall(history),
                "research_stalled" => builtin_research_stalled(history, total_techs),
                "overheat_warning" => builtin_overheat_warning(history),
                other => {
                    tracing::warn!("unknown builtin alert rule: {other}");
                    false
                }
            },
        }
    }

    /// Evaluate all rules against recent metrics history. Returns events for state changes.
    pub fn evaluate(
        &mut self,
        history: &VecDeque<MetricsSnapshot>,
        tick: u64,
        counters: &mut sim_core::Counters,
    ) -> Vec<sim_core::EventEnvelope> {
        let mut events = Vec::new();

        for rule in &self.rules {
            let fired = Self::evaluate_rule(rule, history, self.total_techs);
            let was_active = self.active.contains(&rule.id);

            if fired && !was_active {
                // SHIP_IDLE_WITH_WORK only fires if another alert is already active
                if rule.id == "SHIP_IDLE_WITH_WORK" && self.active.is_empty() {
                    continue;
                }
                self.active.insert(rule.id.clone());
                events.push(make_envelope(
                    counters,
                    tick,
                    sim_core::Event::AlertRaised {
                        alert_id: rule.id.clone(),
                        severity: rule.severity.clone(),
                        message: rule.message.clone(),
                        suggested_action: rule.suggested_action.clone(),
                    },
                ));
            } else if !fired && was_active {
                self.active.remove(&rule.id);
                events.push(make_envelope(
                    counters,
                    tick,
                    sim_core::Event::AlertCleared {
                        alert_id: rule.id.clone(),
                    },
                ));
            }
        }

        events
    }
}

fn make_envelope(
    counters: &mut sim_core::Counters,
    tick: u64,
    event: sim_core::Event,
) -> sim_core::EventEnvelope {
    let id = sim_core::EventId(counters.next_event_id);
    counters.next_event_id += 1;
    sim_core::EventEnvelope { id, tick, event }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_snapshot(tick: u64) -> MetricsSnapshot {
        MetricsSnapshot {
            tick,
            metrics_version: 1,
            total_ore_kg: 0.0,
            total_material_kg: 0.0,
            total_slag_kg: 0.0,
            per_element_material_kg: std::collections::BTreeMap::new(),
            station_storage_used_pct: 0.0,
            ship_cargo_used_pct: 0.0,
            per_element_ore_stats: std::collections::BTreeMap::new(),
            ore_lot_count: 0,
            avg_material_quality: 0.0,
            per_module_metrics: std::collections::BTreeMap::new(),
            fleet_total: 0,
            fleet_idle: 0,
            fleet_mining: 0,
            fleet_transiting: 0,
            fleet_surveying: 0,
            fleet_depositing: 0,
            fleet_refueling: 0,
            scan_sites_remaining: 0,
            asteroids_discovered: 0,
            asteroids_depleted: 0,
            techs_unlocked: 0,
            total_scan_data: 0.0,
            max_tech_evidence: 0.0,
            avg_module_wear: 0.0,
            max_module_wear: 0.0,
            repair_kits_remaining: 0,
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

    fn test_counters() -> sim_core::Counters {
        sim_core::Counters {
            next_event_id: 0,
            next_command_id: 0,
            next_asteroid_id: 0,
            next_lot_id: 0,
            next_module_instance_id: 0,
        }
    }

    /// Load alert rules from content/alerts.json for tests.
    fn test_rules() -> Vec<AlertRuleDef> {
        let text = std::fs::read_to_string("../../content/alerts.json")
            .expect("content/alerts.json should exist");
        serde_json::from_str(&text).expect("alerts.json should parse")
    }

    #[test]
    fn new_engine_has_no_active_alerts() {
        let engine = AlertEngine::new(&test_rules(), 5);
        assert!(engine.active_alert_ids().is_empty());
    }

    #[test]
    fn evaluate_with_empty_history_fires_nothing() {
        let history = VecDeque::new();
        let mut counters = test_counters();
        let mut engine = AlertEngine::new(&test_rules(), 5);
        let events = engine.evaluate(&history, 1, &mut counters);
        assert!(events.is_empty());
    }

    #[test]
    fn evaluate_raises_and_clears_storage_saturation() {
        let mut history = VecDeque::new();
        let mut counters = test_counters();
        let mut engine = AlertEngine::new(&test_rules(), 5);

        // Insert a snapshot with storage > 95%
        let mut snap = empty_snapshot(1);
        snap.station_storage_used_pct = 0.97;
        history.push_back(snap);

        // First evaluation should raise the alert
        let events = engine.evaluate(&history, 1, &mut counters);
        let raised = events.iter().any(|e| {
            matches!(&e.event, sim_core::Event::AlertRaised { alert_id, .. } if alert_id == "STORAGE_SATURATION")
        });
        assert!(raised, "expected STORAGE_SATURATION to be raised");
        assert!(engine
            .active_alert_ids()
            .contains(&"STORAGE_SATURATION".to_string()));

        // Second evaluation with same data should produce no new events
        let events = engine.evaluate(&history, 2, &mut counters);
        let storage_events: Vec<_> = events
            .iter()
            .filter(|e| {
                matches!(&e.event, sim_core::Event::AlertRaised { alert_id, .. } | sim_core::Event::AlertCleared { alert_id } if alert_id == "STORAGE_SATURATION")
            })
            .collect();
        assert!(
            storage_events.is_empty(),
            "no state change should mean no events"
        );

        // Update to below threshold and re-evaluate — should clear
        history.clear();
        let mut snap = empty_snapshot(3);
        snap.station_storage_used_pct = 0.50;
        history.push_back(snap);

        let events = engine.evaluate(&history, 3, &mut counters);
        let cleared = events.iter().any(|e| {
            matches!(&e.event, sim_core::Event::AlertCleared { alert_id } if alert_id == "STORAGE_SATURATION")
        });
        assert!(cleared, "expected STORAGE_SATURATION to be cleared");
        assert!(!engine
            .active_alert_ids()
            .contains(&"STORAGE_SATURATION".to_string()));
    }

    #[test]
    fn ship_idle_requires_other_active_alert() {
        let mut history = VecDeque::new();
        let mut counters = test_counters();
        let mut engine = AlertEngine::new(&test_rules(), 5);

        let mut snap = empty_snapshot(1);
        snap.fleet_idle = 2;
        history.push_back(snap);

        let events = engine.evaluate(&history, 1, &mut counters);
        let idle_raised = events.iter().any(|e| {
            matches!(&e.event, sim_core::Event::AlertRaised { alert_id, .. } if alert_id == "SHIP_IDLE_WITH_WORK")
        });
        assert!(
            !idle_raised,
            "SHIP_IDLE_WITH_WORK should not fire without other active alerts"
        );
    }

    #[test]
    fn module_wear_high_fires_above_threshold() {
        let mut history = VecDeque::new();
        let mut counters = test_counters();
        let mut engine = AlertEngine::new(&test_rules(), 5);

        let mut snap = empty_snapshot(1);
        snap.max_module_wear = 0.85;
        history.push_back(snap);

        let events = engine.evaluate(&history, 1, &mut counters);
        let raised = events.iter().any(|e| {
            matches!(&e.event, sim_core::Event::AlertRaised { alert_id, .. } if alert_id == "MODULE_WEAR_HIGH")
        });
        assert!(raised, "expected MODULE_WEAR_HIGH to be raised");
    }

    #[test]
    fn refinery_stalled_needs_consecutive_samples() {
        let mut history = VecDeque::new();
        let mut counters = test_counters();
        let mut engine = AlertEngine::new(&test_rules(), 5);

        // Only one stalled sample — should not fire
        let mut snap = empty_snapshot(1);
        snap.per_module_metrics
            .entry("processor".to_string())
            .or_default()
            .stalled = 1;
        history.push_back(snap);

        let events = engine.evaluate(&history, 1, &mut counters);
        let raised = events.iter().any(|e| {
            matches!(&e.event, sim_core::Event::AlertRaised { alert_id, .. } if alert_id == "REFINERY_STALLED")
        });
        assert!(!raised, "one sample should not fire REFINERY_STALLED");

        // Second consecutive stalled sample — should fire
        let mut snap2 = empty_snapshot(2);
        snap2
            .per_module_metrics
            .entry("processor".to_string())
            .or_default()
            .stalled = 1;
        history.push_back(snap2);

        let events = engine.evaluate(&history, 2, &mut counters);
        let raised = events.iter().any(|e| {
            matches!(&e.event, sim_core::Event::AlertRaised { alert_id, .. } if alert_id == "REFINERY_STALLED")
        });
        assert!(
            raised,
            "two consecutive stalled samples should fire REFINERY_STALLED"
        );
    }

    #[test]
    fn active_alert_details_returns_full_info() {
        let mut history = VecDeque::new();
        let mut counters = test_counters();
        let mut engine = AlertEngine::new(&test_rules(), 5);

        // Trigger STORAGE_SATURATION
        let mut snap = empty_snapshot(1);
        snap.station_storage_used_pct = 0.97;
        history.push_back(snap);
        engine.evaluate(&history, 1, &mut counters);

        let details = engine.active_alert_details();
        assert_eq!(details.len(), 1);
        assert_eq!(details[0].id, "STORAGE_SATURATION");
        assert!(!details[0].message.is_empty());
        assert!(!details[0].suggested_action.is_empty());
        assert_eq!(details[0].severity, "Warning");
    }

    #[test]
    fn overheat_warning_fires_after_5_consecutive_samples() {
        let mut history = VecDeque::new();
        let mut counters = test_counters();
        let mut engine = AlertEngine::new(&test_rules(), 5);

        // 4 samples with warning — should not fire
        for tick in 1..=4 {
            let mut snap = empty_snapshot(tick);
            snap.overheat_warning_count = 1;
            history.push_back(snap);
        }
        let events = engine.evaluate(&history, 4, &mut counters);
        let raised = events.iter().any(|e| {
            matches!(&e.event, sim_core::Event::AlertRaised { alert_id, .. } if alert_id == "OVERHEAT_WARNING")
        });
        assert!(!raised, "4 samples should not fire OVERHEAT_WARNING");

        // 5th sample — should fire
        let mut snap5 = empty_snapshot(5);
        snap5.overheat_warning_count = 1;
        history.push_back(snap5);
        let events = engine.evaluate(&history, 5, &mut counters);
        let raised = events.iter().any(|e| {
            matches!(&e.event, sim_core::Event::AlertRaised { alert_id, .. } if alert_id == "OVERHEAT_WARNING")
        });
        assert!(raised, "5 consecutive samples should fire OVERHEAT_WARNING");
    }

    #[test]
    fn overheat_warning_clears_when_resolved() {
        let mut history = VecDeque::new();
        let mut counters = test_counters();
        let mut engine = AlertEngine::new(&test_rules(), 5);

        // Trigger alert with 5 warning samples
        for tick in 1..=5 {
            let mut snap = empty_snapshot(tick);
            snap.overheat_warning_count = 1;
            history.push_back(snap);
        }
        engine.evaluate(&history, 5, &mut counters);
        assert!(engine
            .active_alert_ids()
            .contains(&"OVERHEAT_WARNING".to_string()));

        // Clear: push a sample with no overheating
        history.push_back(empty_snapshot(6));
        let events = engine.evaluate(&history, 6, &mut counters);
        let cleared = events.iter().any(|e| {
            matches!(&e.event, sim_core::Event::AlertCleared { alert_id } if alert_id == "OVERHEAT_WARNING")
        });
        assert!(cleared, "OVERHEAT_WARNING should clear");
    }

    #[test]
    fn overheat_critical_fires_after_3_consecutive_samples() {
        let mut history = VecDeque::new();
        let mut counters = test_counters();
        let mut engine = AlertEngine::new(&test_rules(), 5);

        // 2 critical samples — should not fire
        for tick in 1..=2 {
            let mut snap = empty_snapshot(tick);
            snap.overheat_critical_count = 1;
            history.push_back(snap);
        }
        let events = engine.evaluate(&history, 2, &mut counters);
        let raised = events.iter().any(|e| {
            matches!(&e.event, sim_core::Event::AlertRaised { alert_id, .. } if alert_id == "OVERHEAT_CRITICAL")
        });
        assert!(!raised, "2 samples should not fire OVERHEAT_CRITICAL");

        // 3rd sample — should fire
        let mut snap3 = empty_snapshot(3);
        snap3.overheat_critical_count = 1;
        history.push_back(snap3);
        let events = engine.evaluate(&history, 3, &mut counters);
        let raised = events.iter().any(|e| {
            matches!(&e.event, sim_core::Event::AlertRaised { alert_id, .. } if alert_id == "OVERHEAT_CRITICAL")
        });
        assert!(
            raised,
            "3 consecutive critical samples should fire OVERHEAT_CRITICAL"
        );
    }

    #[test]
    fn overheat_critical_clears_when_resolved() {
        let mut history = VecDeque::new();
        let mut counters = test_counters();
        let mut engine = AlertEngine::new(&test_rules(), 5);

        // Trigger critical alert
        for tick in 1..=3 {
            let mut snap = empty_snapshot(tick);
            snap.overheat_critical_count = 2;
            history.push_back(snap);
        }
        engine.evaluate(&history, 3, &mut counters);
        assert!(engine
            .active_alert_ids()
            .contains(&"OVERHEAT_CRITICAL".to_string()));

        // Resolve: no critical modules
        history.push_back(empty_snapshot(4));
        let events = engine.evaluate(&history, 4, &mut counters);
        let cleared = events.iter().any(|e| {
            matches!(&e.event, sim_core::Event::AlertCleared { alert_id } if alert_id == "OVERHEAT_CRITICAL")
        });
        assert!(cleared, "OVERHEAT_CRITICAL should clear");
    }

    #[test]
    fn overheat_warning_also_fires_for_critical_modules() {
        let mut history = VecDeque::new();
        let mut counters = test_counters();
        let mut engine = AlertEngine::new(&test_rules(), 5);

        // 5 samples with only critical (no warning count) — should still fire OVERHEAT_WARNING
        for tick in 1..=5 {
            let mut snap = empty_snapshot(tick);
            snap.overheat_critical_count = 1;
            history.push_back(snap);
        }
        let events = engine.evaluate(&history, 5, &mut counters);
        let warning_raised = events.iter().any(|e| {
            matches!(&e.event, sim_core::Event::AlertRaised { alert_id, .. } if alert_id == "OVERHEAT_WARNING")
        });
        assert!(
            warning_raised,
            "OVERHEAT_WARNING should fire for critical modules too"
        );
    }

    #[test]
    fn propellant_low_fires_when_lh2_below_threshold() {
        let mut history = VecDeque::new();
        let mut counters = test_counters();
        let mut engine = AlertEngine::new(&test_rules(), 5);

        let mut snap = empty_snapshot(1);
        snap.per_element_material_kg
            .insert("LH2".to_string(), 100.0);
        history.push_back(snap);

        let events = engine.evaluate(&history, 1, &mut counters);
        let raised = events.iter().any(|e| {
            matches!(&e.event, sim_core::Event::AlertRaised { alert_id, .. } if alert_id == "PROPELLANT_LOW")
        });
        assert!(raised, "expected PROPELLANT_LOW to fire for LH2=100");
    }

    #[test]
    fn propellant_low_does_not_fire_at_zero() {
        let mut history = VecDeque::new();
        let mut counters = test_counters();
        let mut engine = AlertEngine::new(&test_rules(), 5);

        // LH2 = 0.0 — should NOT fire (min_value check)
        let snap = empty_snapshot(1);
        history.push_back(snap);

        let events = engine.evaluate(&history, 1, &mut counters);
        let raised = events.iter().any(|e| {
            matches!(&e.event, sim_core::Event::AlertRaised { alert_id, .. } if alert_id == "PROPELLANT_LOW")
        });
        assert!(
            !raised,
            "PROPELLANT_LOW should not fire when LH2 is absent/zero"
        );
    }

    #[test]
    fn alerts_json_parses_all_rules() {
        let rules = test_rules();
        // We expect exactly 12 rules matching the original hardcoded set
        // (minus one that was removed or consolidated, if applicable)
        assert!(
            rules.len() >= 12,
            "expected at least 12 alert rules, got {}",
            rules.len()
        );
        // Verify all expected rule IDs are present
        let ids: Vec<&str> = rules.iter().map(|r| r.id.as_str()).collect();
        for expected_id in &[
            "ORE_STARVATION",
            "STORAGE_SATURATION",
            "SLAG_BACKPRESSURE",
            "SHIP_IDLE_WITH_WORK",
            "THROUGHPUT_DROP",
            "EXPLORATION_STALL",
            "MODULE_WEAR_HIGH",
            "REFINERY_STALLED",
            "RESEARCH_STALLED",
            "OVERHEAT_WARNING",
            "OVERHEAT_CRITICAL",
            "PROPELLANT_LOW",
        ] {
            assert!(
                ids.contains(expected_id),
                "missing expected rule ID: {expected_id}"
            );
        }
    }

    #[test]
    fn check_condition_eq_tolerates_float_rounding() {
        // f32→f64 conversion introduces ~1.2e-8 error, exceeding f64::EPSILON
        // but within the 1e-6 game tolerance. Mirrors real metric value paths.
        assert!(check_condition(0.3_f32 as f64, "eq", 0.3_f64));
        // Values that genuinely differ should not be equal
        assert!(!check_condition(5.0, "eq", 5.1));
    }
}
