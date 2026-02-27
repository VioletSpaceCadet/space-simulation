---
name: Frontend Development
triggers: [ui_web, React, component, panel, hook, CSS, Tailwind, frontend, FE, tsx, SSE client, vitest]
agents: [fe-chrome-tester]
---

## When to Use
Any work touching `ui_web/` — React components, hooks, styling, SSE subscriptions, or frontend tests.

## Checklist
- [ ] Read existing component/hook before modifying — understand current patterns
- [ ] Follow Tailwind v4 conventions (CSS-first config, `@theme` not `tailwind.config`)
- [ ] Use react-resizable-panels **v2 API** (`PanelGroup`/`Panel`/`PanelResizeHandle`)
- [ ] SSE subscriptions: use `useEffect` cleanup to close `EventSource`
- [ ] No `any` types — use proper TS interfaces matching daemon response shapes
- [ ] ESLint: no `_` or `_name` in destructuring — use `Object.fromEntries(Object.entries(...).filter(...))`

## Testing
- **Unit/logic:** vitest (`cd ui_web && npm test`)
- **Visual/SSE verification:** fe-chrome-tester agent (requires `--chrome` flag)
- **E2E:** only for critical flows — prefer vitest over Playwright for new tests

## Pitfalls
- `react-resizable-panels` v1 API (deprecated) vs v2 — always use v2
- ESLint blocks underscore-prefixed destructured vars
- SSE `EventSource` must be closed on unmount or you get leaked connections
- Vite 7 uses ESM — don't use CommonJS `require()`
- TS interfaces must match Rust `#[derive(Serialize)]` struct shapes exactly
