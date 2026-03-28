# Actionable Alerts System Design

## Summary

Add a server-side alert engine to `sim_daemon` that evaluates rules against metrics data (via DuckDB queries on CSV files) and emits `AlertRaised` / `AlertCleared` events over the existing SSE stream. The React UI renders active alerts as dismissible badges in the top-right corner.

This is the first feature that depends on the DuckDB integration (from the metrics-query design). DuckDB must land first.

## Goals

- Make the sim **legible** — surface systemic strain as clear, actionable signals
- Prepare for a **planning layer** — alerts become inputs to future AI advisors
- **Not noise** — each alert means "do something specific"

## Alert Set (7 alerts, all grounded in current mechanics)

### Storage / Flow

| Alert ID | Condition | Suggested Action |
|---|---|---|
| `ORE_STARVATION` | `refinery_starved_count > 0` for >= 3 consecutive samples | Assign more ships to mining, or lower refinery threshold |
| `STORAGE_SATURATION` | `station_storage_used_pct > 0.95` | Sell/jettison slag, expand storage, slow mining |
| `SLAG_BACKPRESSURE` | `total_slag_kg` increasing over 5-sample window while `total_material_kg` flat | Manage slag output — jettison or future: slag processing |
| `SHIP_IDLE_WITH_WORK` | `fleet_idle > 0` while any other alert is active | Assign idle ships to address the active alert |

### Throughput

| Alert ID | Condition | Suggested Action |
|---|---|---|
| `THROUGHPUT_DROP` | `total_material_kg` delta over last 10 samples < 50% of delta over last 50 samples | Investigate bottleneck — starvation, stalled ships, or depleted asteroids |
| `EXPLORATION_STALL` | `asteroids_discovered` unchanged for 10+ samples while `scan_sites_remaining > 0` and `fleet_idle > 0` | Ships available but not surveying — check task assignments |

### Research

| Alert ID | Condition | Suggested Action |
|---|---|---|
| `RESEARCH_STALLED` | `max_tech_evidence` unchanged for 20+ samples and `techs_unlocked < total_techs` | No scan data flowing — need more survey/deepscan activity |

## Architecture

```
MetricsFileWriter → metrics_000.csv (flushed per row)
                          ↑
              DuckDB reads CSV files
                          ↓
            AlertEngine (runs every metrics_every ticks)
              - Evaluates each rule via DuckDB query
              - Compares against previous alert state
              - Emits AlertRaised / AlertCleared events
                          ↓
            SSE stream → Frontend
                          ↓
            AlertBadges component (top-right)
```

### Alert Engine (daemon-side)

- New module: `crates/sim_daemon/src/alerts.rs`
- `AlertEngine` struct holds:
  - `duckdb::Connection` (shared with query endpoint, or separate read-only connection)
  - `active_alerts: HashMap<AlertId, ActiveAlert>` (current state)
  - Alert rule definitions (hardcoded initially, could be data-driven later)
- Runs after each metrics sample (same cadence as metrics collection)
- Each rule is a DuckDB query against the CSV files returning a boolean + optional detail
- Produces `Vec<AlertEvent>` (raised/cleared) which get broadcast on the SSE channel

### Event Types

New variants added to `sim_core::Event`:

```rust
AlertRaised {
    alert_id: String,       // e.g. "ORE_STARVATION"
    severity: AlertSeverity, // Warning | Critical
    message: String,        // Human-readable description
    suggested_action: String,
}

AlertCleared {
    alert_id: String,
}
```

`AlertSeverity`: `Warning` (yellow) and `Critical` (red). Most alerts start as Warning; `STORAGE_SATURATION` at >98% escalates to Critical.

### SSE Delivery

Alert events are injected into the same `event_tx` broadcast channel used by game events. The frontend's `useSimStream` already handles arbitrary event types — `applyEvents` just needs a new case.

### Frontend: AlertBadges

- New component: `AlertBadges.tsx` rendered in the status bar area (top-right)
- Each active alert = a colored badge (yellow/red) with:
  - Alert name (short label)
  - Hover/click for detail popover (message + suggested action)
  - Small "x" to dismiss (client-side only — the alert can re-fire if condition persists)
- State managed in the existing `useSimStream` reducer:
  - `EVENTS_RECEIVED` → check for `AlertRaised`/`AlertCleared` events
  - Maintain `activeAlerts: Map<string, Alert>` and `dismissedAlerts: Set<string>` in reducer state
  - Dismissed alerts hidden from view but still tracked (re-shown if cleared then re-raised)

### Alert lifecycle

```
Condition met (3+ samples) → AlertRaised event → Badge appears
Condition clears            → AlertCleared event → Badge auto-removed
User clicks X               → Badge hidden (client-side dismiss)
Condition re-fires later     → New AlertRaised → Badge re-appears (even if previously dismissed)
```

## Implementation Phases

This is a multi-step implementation requiring a worktree:

### Phase 1: DuckDB Integration
- Add `duckdb` crate to workspace
- Embed DuckDB connection in daemon (shared `AppState`)
- Create metrics view pointing to current run's CSV files
- Add `/api/v1/query` endpoint (read-only SQL-over-HTTP)
- Add `sim_cli analyze` subcommand

### Phase 2: Alert Engine (Rust)
- Add `AlertRaised` / `AlertCleared` to `sim_core::Event`
- Create `alerts.rs` module in `sim_daemon`
- Implement 7 alert rules as DuckDB queries
- Wire into tick loop: evaluate after each metrics sample, broadcast results
- Add `/api/v1/alerts` GET endpoint for current active set (reconnect support)

### Phase 3: Frontend Alerts UI
- Add alert types to TypeScript event handling
- Add `activeAlerts` / `dismissedAlerts` to reducer state
- Build `AlertBadges.tsx` component
- Wire into `applyEvents` for AlertRaised/AlertCleared
- Style badges (yellow warning, red critical, dismiss X)

### Phase 4: Testing & Polish
- Integration test: run sim with known seed, verify expected alerts fire
- Frontend test: verify badge rendering and dismiss behavior
- Tune thresholds against real sim runs

## Files to modify/create

| File | Change |
|---|---|
| `Cargo.toml` (workspace) | Add `duckdb` workspace dep |
| `crates/sim_core/Cargo.toml` | (no change — alerts are daemon-side) |
| `crates/sim_core/src/types.rs` | Add `AlertRaised`, `AlertCleared` event variants, `AlertSeverity` enum |
| `crates/sim_daemon/Cargo.toml` | Add `duckdb` dep |
| `crates/sim_daemon/src/alerts.rs` | New: AlertEngine, rule definitions, DuckDB queries |
| `crates/sim_daemon/src/state.rs` | Add DuckDB connection + AlertEngine to AppState |
| `crates/sim_daemon/src/tick_loop.rs` | Call alert engine after metrics sample |
| `crates/sim_daemon/src/routes.rs` | Add `/api/v1/query` and `/api/v1/alerts` endpoints |
| `crates/sim_daemon/src/main.rs` | Initialize DuckDB + AlertEngine |
| `crates/sim_cli/Cargo.toml` | Add `duckdb` dep |
| `crates/sim_cli/src/main.rs` | Add `Analyze` subcommand |
| `ui_web/src/types.ts` | Add alert event types |
| `ui_web/src/hooks/useSimStream.ts` | Handle AlertRaised/AlertCleared in reducer |
| `ui_web/src/hooks/applyEvents.ts` | Apply alert events to state |
| `ui_web/src/components/AlertBadges.tsx` | New: badge UI component |
| `ui_web/src/components/StatusBar.tsx` | Render AlertBadges in top-right |

## Open Questions

1. **DuckDB connection sharing:** Should the alert engine and `/api/v1/query` share a connection (with Mutex), or use separate connections? Separate is simpler but uses more memory.

2. **Alert evaluation frequency:** Same as `metrics_every` (default 60 ticks)? Or independent cadence? Same cadence seems natural since alerts depend on metrics data.

3. **Threshold tuning:** The specific numbers (3 samples for starvation, 95% for saturation, etc.) will need tuning against real runs. Should we make them configurable via content JSON, or hardcode and iterate?

4. **Alert history:** Should we persist fired alerts to a file in the run directory (e.g. `alerts.jsonl`)? Useful for post-run analysis. Not required for MVP but easy to add.
