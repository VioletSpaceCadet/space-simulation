---
name: Cross-Layer Integration
triggers: [cross-layer, end-to-end, E2E, data flow, schema mismatch, SSE gap, type alignment, FE daemon]
agents: [sim-e2e-tester, fe-chrome-tester]
---

## When to Use
Work that spans multiple layers — Rust structs affecting TS interfaces, new SSE events consumed by the UI, API response shape changes, or debugging data flow issues.

## Checklist
- [ ] **Type alignment:** Rust `#[derive(Serialize)]` struct fields match TS interface fields (name, type, optionality)
- [ ] **SSE event coverage:** new state fields included in SSE serialization AND handled in FE `EventSource` listener
- [ ] **API response shape:** daemon JSON responses match what `ui_web` fetch calls expect
- [ ] **New endpoints:** added to both daemon routes AND FE API client
- [ ] **Error propagation:** daemon errors surface meaningfully in UI (not silent failures)

## Testing
- **Data flow:** sim-e2e-tester agent — run sim, check API responses match expectations
- **UI rendering:** fe-chrome-tester agent — verify data appears correctly in panels
- **Comparison:** curl API endpoint, compare JSON shape vs TS interface definition
- **E2E smoke:** `cd e2e && npx playwright test` (minimal but catches major regressions)

## Pitfalls
- New Rust struct fields with `Option<T>` serialize as `null` — TS must handle `null` vs `undefined`
- SSE events are JSON-stringified — adding a field to the Rust struct without updating the FE means silent data loss
- `serde(rename_all = "camelCase")` on Rust side vs literal field names — check which convention each struct uses
- Daemon port differs between dev (3001) and E2E tests (3002)
- Vite dev server (5173) vs E2E (5174) — don't hardcode ports
