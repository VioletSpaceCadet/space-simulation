# MCP Advisor: Daemon Auto-Start

## Problem

The MCP balance advisor's `get_metrics_digest` and `get_active_alerts` tools require a running `sim_daemon` on port 3001. Currently the user must manually start the daemon in a separate terminal before Claude Code can use these tools. This friction makes the advisor less useful for interactive balance analysis during long sim runs.

## Design

Add two new MCP tools: `start_simulation` and `stop_simulation`.

### `start_simulation`

- **Parameters:** `seed` (optional integer, default random), `max_ticks` (optional integer, 0 = unlimited)
- **Behavior:**
  1. If a managed daemon is already running, kill it first
  2. Spawn `cargo run -p sim_daemon -- run --seed <seed>` via `node:child_process.spawn()` with `detached: false`
  3. If `max_ticks` provided and > 0, pass `--max-ticks <max_ticks>`
  4. Poll `http://localhost:3001/api/v1/meta` every 500ms, up to 120 retries (60s timeout to account for first compilation)
  5. Return `{status: "started", seed, pid}` on success, or `{status: "error", message}` on failure
- **Process management:** Store the `ChildProcess` handle in module-level state

### `stop_simulation`

- **Parameters:** none
- **Behavior:** Kill the managed daemon (SIGTERM), return `{status: "stopped"}` or `{status: "not_running"}`

### Lifecycle

- Single daemon at a time. Starting a new sim stops the previous one.
- `process.on('exit', ...)` handler auto-kills the daemon when the MCP server disconnects (Claude Code session ends).
- Fixed port 3001.

### Changes

- `mcp_advisor/src/index.ts` — add ~60 lines for the two new tools + process management
- `CLAUDE.md` — update Balance Advisor section with start_simulation workflow

### Out of scope

- `sim_bench` and `sim_cli` unchanged
- No dynamic port allocation
- No multi-daemon support
- Post-hoc CSV analysis doesn't need MCP
