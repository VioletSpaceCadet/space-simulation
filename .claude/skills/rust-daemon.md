---
name: Rust Daemon & API
triggers: [sim_daemon, endpoint, API, SSE, HTTP, axum, handler, route, AlertEngine, analytics, daemon, perf, timings]
agents: [sim-e2e-tester]
---

## When to Use
Any work in `sim_daemon` — HTTP endpoints, SSE streaming, command queue, alert engine, analytics, or API contracts.

## Checklist
- [ ] **API contract:** response shapes match `docs/reference.md` — update docs if changed
- [ ] **SSE events:** new state fields must be included in SSE serialization
- [ ] **Error responses:** return proper HTTP status codes, not panics
- [ ] **Async safety:** no blocking calls (sync fs, heavy computation) inside async handlers
- [ ] **Command queue:** commands validated before queuing, invalid commands return errors
- [ ] **CORS/headers:** check if new endpoints need CORS headers for UI access

## Rust Analyzer Tools
Use `rust_analyzer_*` MCP tools for Rust navigation:
- **Finding handler definitions:** `rust_analyzer_definition` to jump to route handler implementations
- **Tracing API usage:** `rust_analyzer_references` to find all callers of shared state types (`AppState`, `AlertEngine`)
- **Checking axum types:** `rust_analyzer_hover` to verify extractor types and return types on handlers

## Testing
- **Unit:** `cargo test -p sim_daemon`
- **Integration:** start daemon (`cargo run -p sim_daemon -- run --seed 42`), curl endpoints
- **SSE:** verify event stream format matches what `ui_web` expects
- **Balance/E2E:** sim-e2e-tester agent for daemon + sim integration testing

## Performance Instrumentation
- Daemon enables the `instrumentation` feature on sim_core and collects `TickTimings` every tick
- `SimState.timings_history`: rolling `VecDeque<TickTimings>` capped at 1,000 entries
- `GET /api/v1/perf`: returns per-step stats (mean/p50/p95/max µs) from the buffer. Clones the buffer and drops the mutex before computing to avoid tick stutter.
- Advisor digest (`GET /api/v1/advisor/digest`) includes an optional `perf` field with `sample_count` + per-step mean/p95

## Pitfalls
- Blocking calls in async handlers freeze the tokio runtime — use `spawn_blocking` for heavy work
- SSE serialization must match the TypeScript interfaces in `ui_web` exactly
- `AlertEngine` state is ephemeral — alerts lost on daemon restart
- axum 0.7 extractors: body-consuming extractors (`Json`, `Form`) must be last
- `analytics` module aggregates over time — needs enough ticks to produce meaningful data
- `perf_handler` must clone timings and drop the lock before sorting — holding the mutex during 14 sorts blocks the tick loop
