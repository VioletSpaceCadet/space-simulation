---
name: Rust Daemon & API
triggers: [sim_daemon, endpoint, API, SSE, HTTP, axum, handler, route, AlertEngine, analytics, daemon]
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

## Testing
- **Unit:** `cargo test -p sim_daemon`
- **Integration:** start daemon (`cargo run -p sim_daemon -- run --seed 42`), curl endpoints
- **SSE:** verify event stream format matches what `ui_web` expects
- **Balance/E2E:** sim-e2e-tester agent for daemon + sim integration testing

## Pitfalls
- Blocking calls in async handlers freeze the tokio runtime — use `spawn_blocking` for heavy work
- SSE serialization must match the TypeScript interfaces in `ui_web` exactly
- `AlertEngine` state is ephemeral — alerts lost on daemon restart
- axum 0.7 extractors: body-consuming extractors (`Json`, `Form`) must be last
- `analytics` module aggregates over time — needs enough ticks to produce meaningful data
