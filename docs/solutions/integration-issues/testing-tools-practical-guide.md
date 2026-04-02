---
title: "Practical guide to testing tools: MCP advisor, sim_bench, Chrome, E2E"
category: integration-issues
date: 2026-03-20
module: sim_daemon, sim_bench, mcp_advisor, ui_web, e2e
component: MCP tools, scenario runner, Playwright, Chrome browser
tags: [testing, mcp, sim-bench, chrome, e2e, playwright, balance, debugging]
---

## Overview

This project has four testing approaches beyond unit tests. Each serves a different purpose and has different tradeoffs.

| Tool | Purpose | Speed | Setup | Best for |
|------|---------|-------|-------|----------|
| **MCP Balance Advisor** | Live sim analysis via Claude Code | Fast | Zero (auto-discovered) | Balance tuning, trend analysis, quick checks |
| **sim_bench** | Deterministic batch scenarios | Fast | Zero | Regression testing, parameter sweeps, CI gates |
| **Chrome browser (fe-chrome-tester)** | Visual UI verification | Slow | Requires `--chrome` flag | Layout bugs, SSE rendering, panel interactions |
| **Playwright E2E** | Automated browser tests | Medium | `cd e2e && npm install` | CI smoke tests (fragile, keep minimal) |

---

## 1. MCP Balance Advisor

### What it is

A TypeScript MCP server that wraps the sim_daemon HTTP API. Auto-discovered via `.mcp.json` — no manual setup needed. Claude Code loads the tools automatically.

### How to use it

**Start a simulation:**
```
Use MCP tool: start_simulation (seed: 42)
```
This spawns a `sim_daemon` process in the background. The MCP server manages the lifecycle — it kills any previous daemon first.

**Run at high speed for analysis:**
```
Use MCP tool: set_speed (ticks_per_sec: 1000)
```
Then wait a few seconds for data to accumulate (need 50+ metric samples for meaningful trends).

**Get the digest:**
```
Use MCP tool: get_metrics_digest
```
Returns: current snapshot (40+ metrics), trends (Improving/Stable/Declining), rates (material production, ore consumption, wear accumulation), bottleneck classification (Healthy/starvation/etc), and active alerts.

**Check alerts:**
```
Use MCP tool: get_active_alerts
```
Returns currently firing alerts (e.g., THROUGHPUT_DROP, OVERHEAT_WARNING).

**Read game parameters (no daemon needed):**
```
Use MCP tool: get_game_parameters (file: "constants")
```
Returns current constants.json values. Also supports "module_defs", "techs", "pricing", "all".

**Stop:**
```
Use MCP tool: stop_simulation
```

### What works well

- **Zero setup** — just call the tools. MCP server auto-starts.
- **Fast feedback loop** — start sim, crank to 1000 TPS, get digest in ~5 seconds.
- **Trend analysis** — short_avg vs long_avg tells you if metrics are improving or declining.
- **Bottleneck detection** — automatically classifies the simulation health.
- **Alert surfacing** — daemon's AlertEngine fires on sustained conditions (5+ samples for warnings, 3+ for critical).

### Gotchas

- **Need ~3000+ ticks for meaningful data.** At 1000 TPS, wait 3+ seconds. Trends need 50+ metric samples (captured every 60 ticks by default).
- **Rates at 0.0 during early ticks are normal.** Ships transit for ~2880 ticks before first mining. Material production starts later.
- **Only one daemon at a time.** `start_simulation` kills any previous daemon. Don't start a daemon manually and then use MCP — they'll conflict.
- **Thermal metrics read 0 in early ticks** unless the smelter reaches operating temperature with ore available. The cold refinery path doesn't generate thermal data. See also: [balance-analysis-workflow](../logic-errors/balance-analysis-workflow.md).

### Practical workflow

1. Make a balance change (edit constants.json or module_defs.json)
2. `start_simulation` → `set_speed(1000)` → wait 5s → `get_metrics_digest`
3. Compare trends to baseline expectations
4. `stop_simulation`

---

## 2. sim_bench (Scenario Runner)

### What it is

A Rust binary that runs deterministic simulation scenarios with parallel seeds, parameter overrides, and CSV metric output. Used for regression testing and balance analysis.

### How to use it

**Run a scenario:**
```bash
cargo run -p sim_bench -- run --scenario scenarios/baseline.json
```

**Available scenarios:**

| Scenario | Ticks | Seeds | Duration | Purpose |
|----------|-------|-------|----------|---------|
| `ci_smoke.json` | 34 | 2 | ~1s | CI gate — must not collapse |
| `baseline.json` | 336 | 5 | ~2s | 14-day quick check |
| `month.json` | 720 | 5 | ~3s | 30-day balance check |
| `quarter.json` | 2160 | 5 | ~8s | 90-day sustainability |
| `economy_baseline.json` | varies | varies | varies | Economy-focused |
| `balance_v1.json` | varies | varies | varies | Post-rebalancing validation |

**Output structure:**
```
runs/<scenario>_<timestamp>/
  batch_summary.json     # Aggregated metrics (mean/min/max/stddev across seeds)
  summary.json           # Run completion info
  scenario.json          # Snapshot of scenario used
  seed_1/
    run_result.json      # Final state snapshot
    metrics_000.csv      # Per-tick time series (tick, ore, material, wear, etc.)
  seed_2/
    ...
```

### Parameter overrides

Override constants or module parameters without editing content files:

```json
{
  "name": "test_fast_refinery",
  "ticks": 720,
  "seeds": [1, 2, 3],
  "state": "./content/dev_advanced_state.json",
  "overrides": {
    "mining_rate_kg_per_minute": 30.0,
    "module.basic_iron_refinery.processing_interval_minutes": 45,
    "module.thermal.heat_capacity_j_per_k": 1000.0
  }
}
```

**Override key patterns:**
- Direct constant names: `mining_rate_kg_per_minute`, `station_cargo_capacity_m3`
- Module fields: `module.<module_name>.<field>` (e.g., `module.basic_iron_refinery.wear_per_run`)
- Thermal fields: `module.thermal.<field>` applies to ALL modules with thermal defs
- Thermal constants: `thermal_sink_temp_mk`, `thermal_overheat_warning_offset_mk`

### What works well

- **Deterministic** — same seed always produces same results. Great for A/B comparisons.
- **Parallel seeds via rayon** — 5 seeds run simultaneously, results aggregated.
- **Collapse detection** — flags "refinery starved + fleet idle" as collapsed.
- **CSV time series** — 40+ columns per tick, importable into any analysis tool.
- **CI integration** — `ci_smoke.json` runs in CI, `ci_check_summary.sh` gates on `collapsed_count == 0`.

### Gotchas

- **Debug build is slow for long scenarios.** Use `cargo run --release -p sim_bench` for quarter-length runs.
- **Output accumulates in `runs/`.** Clean up periodically — each run creates a new timestamped directory.
- **Overrides are typed** — passing a string where a number is expected gives a clear error, but watch for f64→f32 precision loss on very precise values.
- **`metrics_every` defaults to 60.** Set to 1 for per-tick granularity (larger CSV but full resolution).
- **No thermal data in baseline** unless smelter has ore AND reaches operating temperature. Early ticks show all thermal metrics at 0.

### Practical workflow

1. Create a scenario JSON with the overrides you want to test
2. Run it: `cargo run -p sim_bench -- run --scenario scenarios/your_scenario.json`
3. Check the terminal summary table for key metrics
4. Dig into `batch_summary.json` for cross-seed statistics
5. Open `metrics_000.csv` for time-series analysis

---

## 3. Chrome Browser Testing (fe-chrome-tester agent)

### What it is

Claude Code's built-in Chrome browser tools, used via the `fe-chrome-tester` agent. Takes screenshots, inspects DOM, checks console errors, runs JS in page context, and verifies UI rendering against live SSE data. Verified working as of 2026-03-20.

### Prerequisites

1. **Claude Code with `--chrome` flag**: `claude --chrome`
2. **Chrome browser** open with the **Claude in Chrome** extension installed and active (green "Claude (MCP)" tab visible)
3. **Running daemon**: use MCP `start_simulation` (recommended) or manual `cargo run -p sim_daemon`
4. **Running Vite dev server**: `cd ui_web && npm run dev` (port 5173)

### Setup gotchas

- **Extension connection can be finicky.** If `tabs_context_mcp` returns "No Chrome extension connected", try: click the extension icon in Chrome toolbar, close/reopen the Claude (MCP) tab, or restart Chrome. May take a few attempts.
- **Always call `tabs_context_mcp` first** to get tab IDs. Then either navigate an existing tab or create a new one with `tabs_create_mcp`.
- **Start the daemon via MCP** (`start_simulation`), not manually. The MCP server manages the lifecycle and avoids port conflicts.

### Verified tool workflow

```
1. tabs_context_mcp (createIfEmpty: true)    → get tab IDs
2. navigate (url: "http://localhost:5173")    → load the UI
3. computer (action: "screenshot")           → verify UI rendered
4. set_speed (ticks_per_sec: 1000)           → crank sim for data
5. computer (action: "wait", duration: 3)    → accumulate ticks
6. computer (action: "screenshot")           → verify SSE streaming
7. read_console_messages (onlyErrors: true)  → check for JS errors
8. javascript_tool (action: "javascript_exec", text: "...") → extract DOM state
9. find (query: "RUNNING button")            → locate interactive elements
10. computer (action: "left_click", ref: "ref_19") → click by ref
```

### Key tools and what they're good for

| Tool | Use for |
|------|---------|
| `computer` (screenshot) | Visual verification, layout checks |
| `computer` (left_click with ref) | Clicking buttons, nav links, alert pills |
| `computer` (key: "Space") | Testing keyboard shortcuts |
| `find` (natural language query) | Locating buttons, inputs, elements by description |
| `javascript_tool` | Extracting DOM state, comparing against API, debugging |
| `read_console_messages` (onlyErrors) | Checking for JS errors after interactions |
| `read_page` | Getting full accessibility tree of page elements |
| `zoom` (region) | Inspecting small UI elements like status bar dots |

### Practical debugging pattern: API vs UI state comparison

The most useful Chrome testing technique is comparing daemon API state against rendered UI:

```javascript
// Run via javascript_tool — extract key UI values from DOM
JSON.stringify({
  tick: document.body.innerText.match(/tick (\d+)/)?.[1],
  balance: document.body.innerText.match(/\$[\d.]+[BMK]?/)?.[0],
  alertPills: [...document.querySelectorAll('button')]
    .filter(b => b.textContent.includes('×'))
    .map(b => b.textContent.trim())
})
```

Then compare with `curl localhost:3001/api/v1/meta` — ticks should match, alerts should match. Drift means SSE handlers are dropping or misprocessing events.

### What works well

- **Visual verification** — screenshots show actual rendered state at 1440x736
- **SSE health checking** — green dot + advancing tick counter = healthy connection
- **Panel rendering** — all 6 panels (MAP, EVENTS, ASTEROIDS, FLEET, RESEARCH, ECONOMY) verified
- **Keyboard shortcuts** — spacebar pause/resume, number keys for speed presets
- **Alert pills** — rendered correctly with dismiss (×) buttons
- **Console error detection** — `onlyErrors: true` flag filters noise effectively
- **`find` tool** — natural language queries like "RUNNING button" reliably locate elements
- **Click by ref** — more reliable than coordinate-based clicking

### Gotchas

- **Extension connection is the #1 setup issue.** `tabs_context_mcp` returning "No Chrome extension connected" means the WebSocket between Claude Code and the extension isn't established. Clicking the extension icon or restarting Chrome usually fixes it.
- **Requires `--chrome` flag** — without it, Chrome tools won't even load. Not a graceful error.
- **Console tracking starts on first call.** Call `read_console_messages` early, then again after actions to catch errors.
- **HMR may miss deep hook changes** — use hard reload (Cmd+R via `computer key "cmd+r"`) for hooks like `applyEvents.ts`.
- **SSE readyState is unreliable** — check the status bar green/red dot or whether tick counter advances, not `EventSource.readyState`.
- **Port conflicts** — if something is already on 5173, Vite falls back. Check `lsof -i :5173` first.

### When to use

- After changing React components, CSS, or panel rendering
- To verify SSE event handlers actually update the UI
- To compare API state vs rendered UI state (the killer debugging technique)
- When investigating a visual bug
- NOT for automated regression testing (use vitest + Playwright for that)

---

## 4. Playwright E2E Tests

### What it is

11 automated browser tests covering core UI flows: app loading, pause/resume, speed controls, save game. Intentionally minimal — kept small for CI stability.

### How to run

```bash
cd e2e && npm install         # First time only
npx playwright test           # Headless
npx playwright test --headed  # Visible browser
```

### Current status (as of 2026-03-20)

**11/11 tests pass** when ports are clear and `workers: 1` is set.

### Architecture

- `global-setup.ts` builds daemon, spawns it on port 3002 (paused), spawns Vite on port 5174
- `global-teardown.ts` kills both processes via saved PID file
- Tests share a single daemon instance — state accumulates across tests
- `workers: 1` is required — all tests share one daemon, parallel `beforeEach` calls race on pause/resume/speed

### What works well

- **All 11 tests pass reliably** with `workers: 1` and clean ports
- **CI integration** — `./scripts/ci_e2e.sh` runs them in CI

### Gotchas

- **Port conflicts are the #1 failure mode.** If port 5174 is already in use (leftover Vite process from dev), the E2E Vite falls back to another port. The daemon CORS origin is hardcoded to `http://localhost:5174`, so the UI can't connect → "reconnecting..." → all interactive tests fail. Fix: `lsof -i :5174` and kill the process, then retry.
- **Must use `workers: 1`.** All tests share one daemon instance. With multiple workers, `beforeEach` calls (resume/speed) from different tests race against each other, causing flaky pause/resume assertions.
- **Daemon starts paused.** Tests that interact with speed/pause call `daemonPost("/api/v1/resume")` in `beforeEach`.
- **Fragile** — UI changes break tests. This is why CLAUDE.md says "keep E2E minimal."
- **Not for complex scenarios.** Use vitest for component logic, sim-e2e-tester for data flow, fe-chrome-tester for visual checks.
- **Test results directory** — screenshots and traces saved to `e2e/test-results/` on failure. Use `npx playwright show-trace <trace.zip>` to debug.

---

## Recommended Testing Strategy

| What changed | Test with |
|-------------|-----------|
| sim_core logic (tick, types, inventory) | `cargo test -p sim_core` + `sim_bench` baseline |
| sim_daemon endpoints/SSE | `cargo test -p sim_daemon` + MCP `start_simulation` + `get_metrics_digest` |
| React components/hooks | `cd ui_web && npm test` (vitest) |
| SSE event handling (applyEvents.ts) | vitest + fe-chrome-tester agent for visual |
| Balance constants | MCP advisor workflow + sim_bench extended scenarios |
| New module type | All of the above + `ci_event_sync.sh` |
| CSS/layout changes | fe-chrome-tester agent (requires `--chrome`) |

### Quick validation sequence

```bash
cargo test                          # All Rust tests
cd ui_web && npm test               # All FE tests
./scripts/ci_event_sync.sh          # Event handler parity
cargo run -p sim_bench -- run --scenario scenarios/baseline.json  # No collapse
```
