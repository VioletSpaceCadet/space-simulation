use sim_core::MetricsSnapshot;
use std::collections::{HashSet, VecDeque};

/// Alert detail returned by the advisor digest endpoint.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AlertDetail {
    pub id: String,
    pub severity: String,
    pub message: String,
    pub suggested_action: String,
}

type RuleFn = fn(&VecDeque<MetricsSnapshot>, &AlertEngine) -> bool;

struct AlertRule {
    id: &'static str,
    severity: sim_core::AlertSeverity,
    check: RuleFn,
    message: &'static str,
    suggested_action: &'static str,
}

const RULES: &[AlertRule] = &[
    AlertRule {
        id: "ORE_STARVATION",
        severity: sim_core::AlertSeverity::Warning,
        check: |h, _| tail(h, 3).iter().all(|s| s.refinery_starved_count > 0) && h.len() >= 3,
        message: "Refineries starved — insufficient ore for 3+ samples",
        suggested_action: "Assign more ships to mining or lower refinery threshold",
    },
    AlertRule {
        id: "STORAGE_SATURATION",
        severity: sim_core::AlertSeverity::Warning,
        check: |h, _| latest(h).is_some_and(|s| s.station_storage_used_pct > 0.95),
        message: "Station storage above 95% capacity",
        suggested_action: "Jettison slag, expand storage, or slow mining",
    },
    AlertRule {
        id: "SLAG_BACKPRESSURE",
        severity: sim_core::AlertSeverity::Warning,
        check: |h, _| {
            let recent = tail(h, 5);
            if recent.len() < 2 {
                return false;
            }
            let slag_delta =
                max_f(&recent, |s| s.total_slag_kg) - min_f(&recent, |s| s.total_slag_kg);
            let mat_delta =
                max_f(&recent, |s| s.total_material_kg) - min_f(&recent, |s| s.total_material_kg);
            slag_delta > 10.0 && mat_delta < 1.0
        },
        message: "Slag accumulating while material production is flat",
        suggested_action: "Manage slag output — jettison or reduce refinery throughput",
    },
    AlertRule {
        id: "SHIP_IDLE_WITH_WORK",
        severity: sim_core::AlertSeverity::Warning,
        check: |h, _| latest(h).is_some_and(|s| s.fleet_idle > 0),
        message: "Ships sitting idle while other alerts are active",
        suggested_action: "Assign idle ships to address active bottlenecks",
    },
    AlertRule {
        id: "THROUGHPUT_DROP",
        severity: sim_core::AlertSeverity::Warning,
        check: |h, _| {
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
        },
        message: "Material throughput dropped significantly",
        suggested_action: "Check for starvation, stalled ships, or depleted asteroids",
    },
    AlertRule {
        id: "EXPLORATION_STALL",
        severity: sim_core::AlertSeverity::Warning,
        check: |h, _| {
            let recent = tail(h, 10);
            if recent.len() < 2 {
                return false;
            }
            let discovered_unchanged = max_u(&recent, |s| s.asteroids_discovered)
                == min_u(&recent, |s| s.asteroids_discovered);
            let has_sites = max_u(&recent, |s| s.scan_sites_remaining) > 0;
            let has_idle = max_u(&recent, |s| s.fleet_idle) > 0;
            discovered_unchanged && has_sites && has_idle
        },
        message: "No new asteroids despite available scan sites and idle ships",
        suggested_action: "Assign idle ships to survey scan sites",
    },
    AlertRule {
        id: "MODULE_WEAR_HIGH",
        severity: sim_core::AlertSeverity::Warning,
        check: |h, _| latest(h).is_some_and(|s| s.max_module_wear > 0.8),
        message: "Module wear exceeding 80% — approaching auto-disable threshold",
        suggested_action: "Run maintenance with repair kits or replace worn modules",
    },
    AlertRule {
        id: "REFINERY_STALLED",
        severity: sim_core::AlertSeverity::Warning,
        check: |h, _| tail(h, 2).iter().all(|s| s.refinery_stalled_count > 0) && h.len() >= 2,
        message: "Refineries stalled — insufficient storage capacity for 2+ samples",
        suggested_action: "Free station storage (jettison slag) or expand cargo capacity",
    },
    AlertRule {
        id: "RESEARCH_STALLED",
        severity: sim_core::AlertSeverity::Warning,
        check: |h, engine| {
            let recent = tail(h, 20);
            if recent.len() < 2 {
                return false;
            }
            let evidence_unchanged = (max_f(&recent, |s| s.max_tech_evidence)
                - min_f(&recent, |s| s.max_tech_evidence))
            .abs()
                < f32::EPSILON;
            #[allow(clippy::cast_possible_truncation)]
            let all_unlocked = max_u(&recent, |s| s.techs_unlocked) >= engine.total_techs as u32;
            evidence_unchanged && !all_unlocked
        },
        message: "Research evidence not accumulating — no scan data flowing",
        suggested_action: "Need more survey and deep scan activity",
    },
    AlertRule {
        id: "OVERHEAT_WARNING",
        severity: sim_core::AlertSeverity::Warning,
        check: |h, _| {
            tail(h, 5)
                .iter()
                .all(|s| s.overheat_warning_count > 0 || s.overheat_critical_count > 0)
                && h.len() >= 5
        },
        message: "Modules in overheat warning zone for 5+ consecutive samples",
        suggested_action: "Add radiators, reduce processing rate, or shut down overheating modules",
    },
    AlertRule {
        id: "OVERHEAT_CRITICAL",
        severity: sim_core::AlertSeverity::Critical,
        check: |h, _| tail(h, 3).iter().all(|s| s.overheat_critical_count > 0) && h.len() >= 3,
        message: "Modules in critical overheat zone — auto-disabled and wearing rapidly",
        suggested_action: "Immediately reduce thermal load or add cooling capacity",
    },
];

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

// --- AlertEngine ---

pub struct AlertEngine {
    active: HashSet<String>,
    total_techs: usize,
}

impl AlertEngine {
    pub fn new(total_techs: usize) -> Self {
        Self {
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
        RULES
            .iter()
            .filter(|rule| self.active.contains(rule.id))
            .map(|rule| AlertDetail {
                id: rule.id.to_string(),
                severity: format!("{:?}", rule.severity),
                message: rule.message.to_string(),
                suggested_action: rule.suggested_action.to_string(),
            })
            .collect()
    }

    /// Evaluate all rules against recent metrics history. Returns events for state changes.
    pub fn evaluate(
        &mut self,
        history: &VecDeque<MetricsSnapshot>,
        tick: u64,
        counters: &mut sim_core::Counters,
    ) -> Vec<sim_core::EventEnvelope> {
        let mut events = Vec::new();

        for rule in RULES {
            let fired = (rule.check)(history, self);
            let was_active = self.active.contains(rule.id);

            if fired && !was_active {
                // SHIP_IDLE_WITH_WORK only fires if another alert is already active
                if rule.id == "SHIP_IDLE_WITH_WORK" && self.active.is_empty() {
                    continue;
                }
                self.active.insert(rule.id.to_string());
                events.push(make_envelope(
                    counters,
                    tick,
                    sim_core::Event::AlertRaised {
                        alert_id: rule.id.to_string(),
                        severity: rule.severity.clone(),
                        message: rule.message.to_string(),
                        suggested_action: rule.suggested_action.to_string(),
                    },
                ));
            } else if !fired && was_active {
                self.active.remove(rule.id);
                events.push(make_envelope(
                    counters,
                    tick,
                    sim_core::Event::AlertCleared {
                        alert_id: rule.id.to_string(),
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
    let id = sim_core::EventId(format!("evt_{:06}", counters.next_event_id));
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
            total_iron_material_kg: 0.0,
            station_storage_used_pct: 0.0,
            ship_cargo_used_pct: 0.0,
            avg_ore_fe_fraction: 0.0,
            ore_lot_count: 0,
            min_ore_fe_fraction: 0.0,
            max_ore_fe_fraction: 0.0,
            avg_material_quality: 0.0,
            refinery_active_count: 0,
            refinery_starved_count: 0,
            refinery_stalled_count: 0,
            assembler_active_count: 0,
            assembler_stalled_count: 0,
            fleet_total: 0,
            fleet_idle: 0,
            fleet_mining: 0,
            fleet_transiting: 0,
            fleet_surveying: 0,
            fleet_depositing: 0,
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

    #[test]
    fn new_engine_has_no_active_alerts() {
        let engine = AlertEngine::new(5);
        assert!(engine.active_alert_ids().is_empty());
    }

    #[test]
    fn evaluate_with_empty_history_fires_nothing() {
        let history = VecDeque::new();
        let mut counters = test_counters();
        let mut engine = AlertEngine::new(5);
        let events = engine.evaluate(&history, 1, &mut counters);
        assert!(events.is_empty());
    }

    #[test]
    fn evaluate_raises_and_clears_storage_saturation() {
        let mut history = VecDeque::new();
        let mut counters = test_counters();
        let mut engine = AlertEngine::new(5);

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
        let mut engine = AlertEngine::new(5);

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
        let mut engine = AlertEngine::new(5);

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
        let mut engine = AlertEngine::new(5);

        // Only one stalled sample — should not fire
        let mut snap = empty_snapshot(1);
        snap.refinery_stalled_count = 1;
        history.push_back(snap);

        let events = engine.evaluate(&history, 1, &mut counters);
        let raised = events.iter().any(|e| {
            matches!(&e.event, sim_core::Event::AlertRaised { alert_id, .. } if alert_id == "REFINERY_STALLED")
        });
        assert!(!raised, "one sample should not fire REFINERY_STALLED");

        // Second consecutive stalled sample — should fire
        let mut snap2 = empty_snapshot(2);
        snap2.refinery_stalled_count = 1;
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
        let mut engine = AlertEngine::new(5);

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
        let mut engine = AlertEngine::new(5);

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
        let mut engine = AlertEngine::new(5);

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
        let mut engine = AlertEngine::new(5);

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
        let mut engine = AlertEngine::new(5);

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
        let mut engine = AlertEngine::new(5);

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
}
