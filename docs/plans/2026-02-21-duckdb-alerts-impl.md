# DuckDB + Actionable Alerts Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Embed DuckDB in the daemon for SQL-over-HTTP metrics querying, then build an alert engine that evaluates rules via DuckDB and pushes alerts to the React UI as dismissible badges.

**Architecture:** DuckDB reads from existing rotating CSV files (zero sim-loop changes). Alert engine runs after each metrics sample, evaluates 7 rules via SQL, emits AlertRaised/AlertCleared events on the existing SSE stream. Frontend renders active alerts as color-coded dismissible badges in the top-right status bar. CLI gets an `analyze` subcommand for offline querying.

**Tech Stack:** Rust (duckdb crate), axum, tokio::task::spawn_blocking, React 19, TypeScript 5, Tailwind v4

**Reference docs:** `docs/plans/2026-02-20-metrics-query-design.md`, `docs/plans/2026-02-21-actionable-alerts-design.md`

---

## Phase 1: DuckDB Integration

### Task 1: Add duckdb dependency to workspace

**Files:**
- Modify: `crates/sim_daemon/Cargo.toml`
- Modify: `crates/sim_cli/Cargo.toml`

**Step 1: Add duckdb to sim_daemon Cargo.toml**

Add under `[dependencies]`:
```toml
duckdb = { version = "1", features = ["bundled"] }
```

**Step 2: Add duckdb to sim_cli Cargo.toml**

Same addition.

**Step 3: Verify it compiles**

Run: `cargo build -p sim_daemon -p sim_cli`
Expected: Compiles (first build will be slow — duckdb bundles a C library).

**Step 4: Commit**

```
feat(deps): add duckdb crate to sim_daemon and sim_cli
```

---

### Task 2: Add DuckDB connection to daemon AppState

**Files:**
- Modify: `crates/sim_daemon/src/state.rs`
- Modify: `crates/sim_daemon/src/main.rs`

**Step 1: Add run_dir and DuckDB connection to AppState**

In `state.rs`, add a new field to `AppState`:
```rust
pub struct AppState {
    pub sim: SharedSim,
    pub event_tx: EventTx,
    pub ticks_per_sec: f64,
    pub duckdb: Arc<Mutex<duckdb::Connection>>,
    pub run_dir: Option<std::path::PathBuf>,
}
```

Import `Arc` and `Mutex` from std::sync (already imported). Add `use duckdb;` at top.

**Step 2: Initialize DuckDB in main.rs**

After the metrics_writer setup block (where `run_dir` is created), create an in-memory DuckDB connection and create a view over the CSV files:

```rust
let duckdb_conn = duckdb::Connection::open_in_memory()
    .context("opening DuckDB in-memory database")?;

if let Some(ref dir) = run_dir_path {
    let glob_pattern = dir.join("metrics_*.csv").display().to_string();
    duckdb_conn.execute_batch(&format!(
        "CREATE VIEW metrics AS SELECT * FROM read_csv_auto('{glob_pattern}')"
    )).context("creating DuckDB metrics view")?;
}

let duckdb = Arc::new(std::sync::Mutex::new(duckdb_conn));
```

The `run_dir_path` variable needs to be extracted from the existing metrics setup block — currently `run_dir` is scoped inside the `if no_metrics { ... } else { ... }` block. Hoist it to an `Option<PathBuf>`.

**Step 3: Pass duckdb and run_dir to AppState**

Update the AppState construction to include the new fields.

**Step 4: Verify it compiles and tests pass**

Run: `cargo test -p sim_daemon`
Expected: All existing tests pass. Tests use `metrics_writer: None` and won't need DuckDB — pass a dummy connection or make `duckdb` optional. Simplest: create an in-memory connection with no view in test setup.

**Step 5: Commit**

```
feat(daemon): initialize DuckDB in-memory with metrics view
```

---

### Task 3: Add POST /api/v1/query endpoint

**Files:**
- Modify: `crates/sim_daemon/src/routes.rs`

**Step 1: Add the query handler**

Add a new route in `make_router`:
```rust
.route("/api/v1/query", post(query_handler))
```

Import `post` from axum::routing (alongside existing `get`).

Add the handler:
```rust
#[derive(serde::Deserialize)]
struct QueryRequest {
    sql: String,
}

#[derive(serde::Serialize)]
struct QueryResponse {
    columns: Vec<String>,
    rows: Vec<Vec<serde_json::Value>>,
}

async fn query_handler(
    State(app_state): State<AppState>,
    Json(req): Json<QueryRequest>,
) -> Result<Json<QueryResponse>, (StatusCode, String)> {
    let duckdb = app_state.duckdb.clone();
    tokio::task::spawn_blocking(move || {
        let conn = duckdb.lock().unwrap();
        let mut stmt = conn.prepare(&req.sql)
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("SQL error: {e}")))?;
        let column_count = stmt.column_count();
        let columns: Vec<String> = (0..column_count)
            .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
            .collect();
        let rows_iter = stmt.query_map([], |row| {
            let mut vals = Vec::with_capacity(column_count);
            for i in 0..column_count {
                let val: duckdb::types::Value = row.get(i)?;
                vals.push(duckdb_value_to_json(val));
            }
            Ok(vals)
        }).map_err(|e| (StatusCode::BAD_REQUEST, format!("Query error: {e}")))?;
        let rows: Vec<Vec<serde_json::Value>> = rows_iter
            .filter_map(|r| r.ok())
            .collect();
        Ok(Json(QueryResponse { columns, rows }))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Task join error: {e}")))?
}
```

Add helper to convert DuckDB values to JSON:
```rust
fn duckdb_value_to_json(val: duckdb::types::Value) -> serde_json::Value {
    match val {
        duckdb::types::Value::Null => serde_json::Value::Null,
        duckdb::types::Value::Boolean(b) => serde_json::Value::Bool(b),
        duckdb::types::Value::TinyInt(n) => serde_json::json!(n),
        duckdb::types::Value::SmallInt(n) => serde_json::json!(n),
        duckdb::types::Value::Int(n) => serde_json::json!(n),
        duckdb::types::Value::BigInt(n) => serde_json::json!(n),
        duckdb::types::Value::Float(n) => serde_json::json!(n),
        duckdb::types::Value::Double(n) => serde_json::json!(n),
        duckdb::types::Value::Text(s) => serde_json::Value::String(s),
        _ => serde_json::Value::String(format!("{val:?}")),
    }
}
```

**Note:** The exact DuckDB `Value` variants may differ by crate version. Check `duckdb::types::Value` docs and adjust the match arms. The fallback `_ =>` handles any unmatched types as debug strings.

**Step 2: Verify it compiles**

Run: `cargo build -p sim_daemon`

**Step 3: Manual smoke test**

Run daemon: `cargo run -p sim_daemon -- run --seed 42`
Wait for some ticks, then:
```bash
curl -s -X POST http://localhost:3001/api/v1/query \
  -H 'Content-Type: application/json' \
  -d '{"sql": "SELECT tick, total_ore_kg FROM metrics LIMIT 5"}' | jq .
```
Expected: JSON with columns and rows arrays.

**Step 4: Commit**

```
feat(daemon): add POST /api/v1/query endpoint for SQL-over-HTTP
```

---

### Task 4: Add sim_cli analyze subcommand

**Files:**
- Modify: `crates/sim_cli/src/main.rs`

**Step 1: Add Analyze subcommand to CLI enum**

```rust
/// Query metrics from a completed run using SQL.
Analyze {
    /// Run directory (e.g. runs/20260220_143052_seed42).
    #[arg(long)]
    run: String,
    /// SQL query to execute against the metrics.
    #[arg(long)]
    sql: String,
},
```

**Step 2: Implement analyze handler**

```rust
fn analyze(run_dir: &str, sql: &str) -> Result<()> {
    let glob_pattern = format!("{run_dir}/metrics_*.csv");
    let conn = duckdb::Connection::open_in_memory()
        .context("opening DuckDB")?;
    conn.execute_batch(&format!(
        "CREATE VIEW metrics AS SELECT * FROM read_csv_auto('{glob_pattern}')"
    )).context("creating metrics view")?;

    let mut stmt = conn.prepare(sql).context("preparing SQL")?;
    let column_count = stmt.column_count();

    // Print header
    let columns: Vec<String> = (0..column_count)
        .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
        .collect();
    println!("{}", columns.join("\t"));

    // Print rows
    let rows = stmt.query_map([], |row| {
        let vals: Vec<String> = (0..column_count)
            .map(|i| {
                row.get::<_, duckdb::types::Value>(i)
                    .map(|v| format!("{v:?}"))
                    .unwrap_or_default()
            })
            .collect();
        Ok(vals.join("\t"))
    }).context("executing query")?;
    for row in rows {
        println!("{}", row?);
    }
    Ok(())
}
```

**Step 3: Wire into main match**

Add the `Analyze` arm in the main match block:
```rust
Commands::Analyze { run, sql } => {
    analyze(&run, &sql)?;
}
```

**Step 4: Smoke test**

```bash
# First create a run
cargo run -p sim_cli -- run --ticks 500 --seed 42
# Then analyze it
cargo run -p sim_cli -- analyze --run runs/*seed42 --sql "SELECT COUNT(*) FROM metrics"
```

**Step 5: Commit**

```
feat(cli): add analyze subcommand for SQL queries on run metrics
```

---

## Phase 2: Alert Engine (Rust)

### Task 5: Add AlertRaised / AlertCleared event variants

**Files:**
- Modify: `crates/sim_core/src/types.rs` (insert before line 397, the closing `}` of Event enum)

**Step 1: Add AlertSeverity enum**

Insert before the Event enum (around line 295):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Warning,
    Critical,
}
```

**Step 2: Add event variants**

Insert before the closing `}` of the Event enum (line 397):
```rust
    AlertRaised {
        alert_id: String,
        severity: AlertSeverity,
        message: String,
        suggested_action: String,
    },
    AlertCleared {
        alert_id: String,
    },
```

**Step 3: Verify tests pass**

Run: `cargo test`
Expected: All pass — new variants don't break existing matches because events are pattern-matched with specific variants (not exhaustive matches in most places).

**Step 4: Commit**

```
feat(types): add AlertRaised/AlertCleared event variants and AlertSeverity enum
```

---

### Task 6: Create AlertEngine in sim_daemon

**Files:**
- Create: `crates/sim_daemon/src/alerts.rs`
- Modify: `crates/sim_daemon/src/main.rs` (add `mod alerts;`)

**Step 1: Create alerts.rs with AlertEngine struct and rule definitions**

```rust
use sim_core::{AlertSeverity, Event, EventEnvelope};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

struct AlertRule {
    id: &'static str,
    severity: AlertSeverity,
    /// SQL query that returns a single row with a `fired` boolean column.
    sql: &'static str,
    message: &'static str,
    suggested_action: &'static str,
}

const RULES: &[AlertRule] = &[
    AlertRule {
        id: "ORE_STARVATION",
        severity: AlertSeverity::Warning,
        sql: "SELECT CASE WHEN COUNT(*) >= 3 THEN true ELSE false END AS fired \
              FROM (SELECT * FROM metrics ORDER BY tick DESC LIMIT 3) \
              WHERE refinery_starved_count > 0",
        message: "Refineries starved — insufficient ore buffer for 3+ samples",
        suggested_action: "Assign more ships to mining or lower refinery ore threshold",
    },
    AlertRule {
        id: "STORAGE_SATURATION",
        severity: AlertSeverity::Warning,
        sql: "SELECT CASE WHEN station_storage_used_pct > 0.95 THEN true ELSE false END AS fired \
              FROM metrics ORDER BY tick DESC LIMIT 1",
        message: "Station storage above 95% capacity",
        suggested_action: "Jettison slag, expand storage, or slow mining intake",
    },
    AlertRule {
        id: "SLAG_BACKPRESSURE",
        severity: AlertSeverity::Warning,
        sql: "WITH recent AS (SELECT * FROM metrics ORDER BY tick DESC LIMIT 5), \
              deltas AS (SELECT MAX(total_slag_kg) - MIN(total_slag_kg) AS slag_delta, \
                         MAX(total_material_kg) - MIN(total_material_kg) AS mat_delta FROM recent) \
              SELECT CASE WHEN slag_delta > 10 AND mat_delta < 1 THEN true ELSE false END AS fired \
              FROM deltas",
        message: "Slag accumulating while material production is flat",
        suggested_action: "Manage slag output — jettison or reduce refinery throughput",
    },
    AlertRule {
        id: "SHIP_IDLE_WITH_WORK",
        severity: AlertSeverity::Warning,
        sql: "SELECT CASE WHEN fleet_idle > 0 THEN true ELSE false END AS fired \
              FROM metrics ORDER BY tick DESC LIMIT 1",
        message: "Ships sitting idle while other alerts are active",
        suggested_action: "Assign idle ships to address active bottlenecks",
    },
    AlertRule {
        id: "THROUGHPUT_DROP",
        severity: AlertSeverity::Warning,
        sql: "WITH recent AS (SELECT MAX(total_material_kg) - MIN(total_material_kg) AS delta \
              FROM (SELECT * FROM metrics ORDER BY tick DESC LIMIT 10)), \
              longer AS (SELECT MAX(total_material_kg) - MIN(total_material_kg) AS delta \
              FROM (SELECT * FROM metrics ORDER BY tick DESC LIMIT 50)) \
              SELECT CASE WHEN longer.delta > 0 AND recent.delta < longer.delta * 0.5 \
              THEN true ELSE false END AS fired FROM recent, longer",
        message: "Material production rate dropped significantly vs recent average",
        suggested_action: "Check for ore starvation, stalled ships, or depleted asteroids",
    },
    AlertRule {
        id: "EXPLORATION_STALL",
        severity: AlertSeverity::Warning,
        sql: "WITH recent AS (SELECT * FROM metrics ORDER BY tick DESC LIMIT 10) \
              SELECT CASE WHEN MAX(asteroids_discovered) = MIN(asteroids_discovered) \
              AND MAX(scan_sites_remaining) > 0 AND MAX(fleet_idle) > 0 \
              THEN true ELSE false END AS fired FROM recent",
        message: "No new asteroids discovered despite available scan sites and idle ships",
        suggested_action: "Assign idle ships to survey scan sites",
    },
    AlertRule {
        id: "RESEARCH_STALLED",
        severity: AlertSeverity::Warning,
        sql: "WITH recent AS (SELECT * FROM metrics ORDER BY tick DESC LIMIT 20) \
              SELECT CASE WHEN MAX(max_tech_evidence) = MIN(max_tech_evidence) \
              AND (SELECT MAX(techs_unlocked) FROM recent) < {total_techs} \
              THEN true ELSE false END AS fired FROM recent",
        message: "Research evidence not accumulating — no scan data flowing",
        suggested_action: "Need more survey and deep scan activity to generate research data",
    },
];
```

**Note:** `RESEARCH_STALLED` uses `{total_techs}` placeholder — this must be substituted at runtime from `content.techs.len()`. The engine will format the SQL string before executing.

**Step 2: Implement AlertEngine**

```rust
pub struct AlertEngine {
    active: HashMap<String, ()>,
    total_techs: usize,
}

impl AlertEngine {
    pub fn new(total_techs: usize) -> Self {
        Self {
            active: HashMap::new(),
            total_techs,
        }
    }

    /// Evaluate all alert rules. Returns events for state changes (raised/cleared).
    pub fn evaluate(
        &mut self,
        conn: &Arc<Mutex<duckdb::Connection>>,
        tick: u64,
        counters: &mut sim_core::Counters,
    ) -> Vec<EventEnvelope> {
        let conn = conn.lock().unwrap();
        let mut events = Vec::new();

        for rule in RULES {
            let sql = rule.sql.replace("{total_techs}", &self.total_techs.to_string());
            let fired = match conn.query_row(&sql, [], |row| row.get::<_, bool>(0)) {
                Ok(b) => b,
                Err(_) => false, // Query failed (e.g. no data yet) — treat as not fired
            };

            let was_active = self.active.contains_key(rule.id);

            if fired && !was_active {
                // SHIP_IDLE_WITH_WORK only fires if another alert is already active
                if rule.id == "SHIP_IDLE_WITH_WORK" && self.active.is_empty() {
                    continue;
                }
                self.active.insert(rule.id.to_string(), ());
                events.push(sim_core::emit(counters, tick, Event::AlertRaised {
                    alert_id: rule.id.to_string(),
                    severity: rule.severity.clone(),
                    message: rule.message.to_string(),
                    suggested_action: rule.suggested_action.to_string(),
                }));
            } else if !fired && was_active {
                self.active.remove(rule.id);
                events.push(sim_core::emit(counters, tick, Event::AlertCleared {
                    alert_id: rule.id.to_string(),
                }));
            }
        }

        events
    }
}
```

**Note:** `sim_core::emit` is `pub(crate)` — it's only available inside sim_core. We need to either:
- Make `emit` public in sim_core, OR
- Construct `EventEnvelope` directly in the daemon (it's a simple struct)

The simpler approach: construct `EventEnvelope` directly since it's just `{ id, tick, event }`. Use the daemon's `next_event_id` counter from `SimState.game_state.counters`.

Revise to construct envelopes directly:
```rust
fn make_envelope(counters: &mut sim_core::Counters, tick: u64, event: Event) -> EventEnvelope {
    let id = sim_core::EventId(format!("evt_{:06}", counters.next_event_id));
    counters.next_event_id += 1;
    EventEnvelope { id, tick, event }
}
```

**Step 3: Add `mod alerts;` to main.rs**

**Step 4: Verify compilation**

Run: `cargo build -p sim_daemon`

**Step 5: Commit**

```
feat(daemon): add AlertEngine with 7 DuckDB-backed alert rules
```

---

### Task 7: Wire AlertEngine into tick loop and SSE

**Files:**
- Modify: `crates/sim_daemon/src/state.rs` — add AlertEngine to SimState
- Modify: `crates/sim_daemon/src/tick_loop.rs` — call evaluate after metrics sample
- Modify: `crates/sim_daemon/src/main.rs` — construct AlertEngine at startup

**Step 1: Add AlertEngine to SimState**

In `state.rs`, add:
```rust
pub alert_engine: Option<crate::alerts::AlertEngine>,
```

**Step 2: Initialize in main.rs**

After constructing `metrics_writer`, create the alert engine:
```rust
let alert_engine = if no_metrics {
    None
} else {
    Some(crate::alerts::AlertEngine::new(content.techs.len()))
};
```

Pass to SimState construction.

**Step 3: Call evaluate in tick_loop.rs**

After the metrics collection block (`if metrics_every > 0 && ...`), add:
```rust
// Evaluate alert rules after metrics sample
let alert_events = if metrics_every > 0
    && guard.game_state.meta.tick.is_multiple_of(metrics_every)
{
    if let Some(ref mut engine) = guard.alert_engine {
        engine.evaluate(
            &duckdb,
            guard.game_state.meta.tick,
            &mut guard.game_state.counters,
        )
    } else {
        vec![]
    }
} else {
    vec![]
};
```

Then append alert_events to the game events before broadcasting:
```rust
let mut all_events = events;
all_events.extend(alert_events);
let _ = event_tx.send(all_events);
```

**Note:** The DuckDB `Arc<Mutex<Connection>>` must be passed into `run_tick_loop`. Add it as a parameter.

**Step 4: Update run_tick_loop signature**

```rust
pub async fn run_tick_loop(
    sim: SharedSim,
    event_tx: EventTx,
    ticks_per_sec: f64,
    max_ticks: Option<u64>,
    duckdb: Arc<Mutex<duckdb::Connection>>,
)
```

Update the call site in main.rs.

**Step 5: Verify tests pass**

Run: `cargo test -p sim_daemon`
Update test `make_test_state` to include `alert_engine: None`.

**Step 6: Commit**

```
feat(daemon): wire AlertEngine into tick loop, emit alerts via SSE
```

---

### Task 8: Add GET /api/v1/alerts endpoint

**Files:**
- Modify: `crates/sim_daemon/src/routes.rs`
- Modify: `crates/sim_daemon/src/state.rs` (expose active alerts)

**Step 1: Add method to get active alerts from SimState**

The alert engine tracks `active: HashMap<String, ()>`. Add a method or access it through SimState to return the current active alert IDs.

**Step 2: Add route and handler**

```rust
.route("/api/v1/alerts", get(alerts_handler))
```

Handler returns the active alert set from the alert engine (locks SimState briefly).

**Step 3: Commit**

```
feat(daemon): add GET /api/v1/alerts endpoint for current active set
```

---

## Phase 3: Frontend Alerts UI

### Task 9: Add alert types to TypeScript

**Files:**
- Modify: `ui_web/src/types.ts`

**Step 1: Add alert types**

```typescript
export type AlertSeverity = 'Warning' | 'Critical'

export interface ActiveAlert {
  alert_id: string
  severity: AlertSeverity
  message: string
  suggested_action: string
  tick: number
}
```

**Step 2: Commit**

```
feat(ui): add TypeScript alert types
```

---

### Task 10: Handle alert events in useSimStream reducer

**Files:**
- Modify: `ui_web/src/hooks/useSimStream.ts`
- Modify: `ui_web/src/hooks/applyEvents.ts`

**Step 1: Add alert state to reducer**

Extend `State` interface:
```typescript
interface State {
  snapshot: SimSnapshot | null
  events: SimEvent[]
  connected: boolean
  currentTick: number
  activeAlerts: Map<string, ActiveAlert>
  dismissedAlerts: Set<string>
}
```

Update `initialState`:
```typescript
const initialState: State = {
  snapshot: null,
  events: [],
  connected: false,
  currentTick: 0,
  activeAlerts: new Map(),
  dismissedAlerts: new Set(),
}
```

Add a new action type:
```typescript
| { type: 'DISMISS_ALERT'; alertId: string }
```

**Step 2: Process alert events in EVENTS_RECEIVED case**

After the existing `applyEvents` call, scan the incoming events for alert events:

```typescript
const newAlerts = new Map(state.activeAlerts)
let newDismissed = state.dismissedAlerts
for (const e of action.events) {
  const eventKey = Object.keys(e.event)[0]
  const data = e.event[eventKey] as Record<string, unknown>
  if (eventKey === 'AlertRaised') {
    newAlerts.set(data.alert_id as string, {
      alert_id: data.alert_id as string,
      severity: data.severity as AlertSeverity,
      message: data.message as string,
      suggested_action: data.suggested_action as string,
      tick: e.tick,
    })
    // If previously dismissed but re-raised, un-dismiss it
    if (newDismissed.has(data.alert_id as string)) {
      newDismissed = new Set([...newDismissed].filter(id => id !== data.alert_id))
    }
  } else if (eventKey === 'AlertCleared') {
    newAlerts.delete(data.alert_id as string)
    newDismissed = new Set([...newDismissed].filter(id => id !== data.alert_id))
  }
}
```

Include `activeAlerts: newAlerts, dismissedAlerts: newDismissed` in the returned state.

**Step 3: Handle DISMISS_ALERT action**

```typescript
case 'DISMISS_ALERT':
  return {
    ...state,
    dismissedAlerts: new Set([...state.dismissedAlerts, action.alertId]),
  }
```

**Step 4: Return alerts from the hook**

The hook's return value needs to include `activeAlerts`, `dismissedAlerts`, and a `dismissAlert` callback:
```typescript
const dismissAlert = useCallback((alertId: string) => {
  dispatch({ type: 'DISMISS_ALERT', alertId })
}, [])

return { snapshot, events, connected, currentTick, activeAlerts, dismissedAlerts, dismissAlert }
```

**Step 5: Run frontend tests**

Run: `cd ui_web && npm test`

**Step 6: Commit**

```
feat(ui): handle AlertRaised/AlertCleared in useSimStream reducer
```

---

### Task 11: Build AlertBadges component

**Files:**
- Create: `ui_web/src/components/AlertBadges.tsx`
- Modify: `ui_web/src/App.tsx`
- Modify: `ui_web/src/components/StatusBar.tsx`

**Step 1: Create AlertBadges.tsx**

```tsx
import { useState } from 'react'
import type { ActiveAlert } from '../types'

interface Props {
  alerts: Map<string, ActiveAlert>
  dismissed: Set<string>
  onDismiss: (alertId: string) => void
}

export function AlertBadges({ alerts, dismissed, onDismiss }: Props) {
  const [expandedId, setExpandedId] = useState<string | null>(null)

  const visible = [...alerts.values()].filter(a => !dismissed.has(a.alert_id))
  if (visible.length === 0) return null

  return (
    <div className="flex gap-1.5 items-center">
      {visible.map((alert) => {
        const isWarning = alert.severity === 'Warning'
        const bgColor = isWarning ? 'bg-amber-500/20' : 'bg-red-500/20'
        const textColor = isWarning ? 'text-amber-400' : 'text-red-400'
        const borderColor = isWarning ? 'border-amber-500/40' : 'border-red-500/40'
        const isExpanded = expandedId === alert.alert_id

        return (
          <div key={alert.alert_id} className="relative">
            <button
              type="button"
              onClick={() => setExpandedId(isExpanded ? null : alert.alert_id)}
              className={`flex items-center gap-1.5 px-2 py-0.5 rounded border text-[10px] font-medium uppercase tracking-wide ${bgColor} ${textColor} ${borderColor} cursor-pointer hover:brightness-125 transition-all`}
            >
              <span>{alert.alert_id.replace(/_/g, ' ')}</span>
              <span
                role="button"
                className="ml-1 opacity-60 hover:opacity-100"
                onClick={(e) => { e.stopPropagation(); onDismiss(alert.alert_id) }}
              >
                ×
              </span>
            </button>
            {isExpanded && (
              <div className={`absolute top-full right-0 mt-1 z-50 w-72 p-3 rounded border ${borderColor} bg-surface text-xs shadow-lg`}>
                <p className={`font-medium mb-1 ${textColor}`}>{alert.message}</p>
                <p className="text-dim">{alert.suggested_action}</p>
                <p className="text-muted mt-1.5">Since tick {alert.tick}</p>
              </div>
            )}
          </div>
        )
      })}
    </div>
  )
}
```

**Step 2: Update StatusBar to accept and render alerts**

Add alert props to StatusBar:
```typescript
interface Props {
  tick: number
  connected: boolean
  measuredTickRate: number
  alerts: Map<string, ActiveAlert>
  dismissedAlerts: Set<string>
  onDismissAlert: (alertId: string) => void
}
```

Add `AlertBadges` in the status bar div, positioned with `ml-auto` to push it right:
```tsx
<div className="ml-auto">
  <AlertBadges alerts={alerts} dismissed={dismissedAlerts} onDismiss={onDismissAlert} />
</div>
```

**Step 3: Update App.tsx to pass alert props**

Destructure `activeAlerts`, `dismissedAlerts`, `dismissAlert` from `useSimStream()` and pass to `StatusBar`.

**Step 4: Verify it renders**

Run: `cd ui_web && npm run dev`
Open browser — status bar should render (no alerts yet until daemon is running with enough data).

**Step 5: Run frontend tests**

Run: `cd ui_web && npm test`

**Step 6: Commit**

```
feat(ui): add AlertBadges component with dismissible badges in status bar
```

---

## Phase 4: Integration Testing & Polish

### Task 12: Integration smoke test

**Step 1: Run daemon with enough ticks to trigger alerts**

```bash
cargo run -p sim_daemon -- run --seed 42 --ticks-per-sec 0 --max-ticks 2000
```

In another terminal:
```bash
# Check if alerts are firing
curl -s http://localhost:3001/api/v1/alerts | jq .

# Query metrics
curl -s -X POST http://localhost:3001/api/v1/query \
  -H 'Content-Type: application/json' \
  -d '{"sql": "SELECT tick, refinery_starved_count, fleet_idle FROM metrics ORDER BY tick DESC LIMIT 10"}' | jq .
```

**Step 2: Open UI and verify badges appear**

Open `http://localhost:5173` — should see badges in the top-right for any active alerts. Click to expand detail. Click X to dismiss.

**Step 3: Tune thresholds if needed**

Adjust SQL queries in alerts.rs based on observed behavior. The threshold constants (3 samples, 95%, 50% drop) may need tuning.

**Step 4: Commit any threshold adjustments**

```
fix(alerts): tune alert thresholds from integration testing
```

---

### Task 13: Final verification and docs

**Step 1: Run all tests**

```bash
cargo test
cd ui_web && npm test
```

**Step 2: Update CLAUDE.md**

Add `/api/v1/query` and `/api/v1/alerts` endpoints to the sim_daemon description. Mention DuckDB dependency.

**Step 3: Commit**

```
docs: update CLAUDE.md with DuckDB query and alerts endpoints
```
