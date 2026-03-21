---
name: Frontend Development
triggers: [ui_web, React, component, panel, hook, CSS, Tailwind, frontend, FE, tsx, SSE client, vitest, UI, design, layout, styling]
agents: [fe-chrome-tester, compound-engineering:design:design-iterator, compound-engineering:design:design-implementation-reviewer]
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
- [ ] **Theme centralization:** game-concept colors come from `config/theme.ts` — never inline hex values for game types
- [ ] **No hardcoded content IDs:** never use `new Set(['Fe', ...])` or `=== 'ore'` for categorization — read categories from the content API
- [ ] **No silent catches:** every `.catch()` must log the error with context — empty catch bodies are banned
- [ ] **Error boundaries:** new panels/top-level components wrapped in `ErrorBoundary`

## Design Quality
- [ ] Use `compound-engineering:frontend-design` skill for UI implementation — produces polished, distinctive code
- [ ] For visual changes: run `design-iterator` agent (requires `--chrome`) to iterate via screenshot→analyze→improve cycles
- [ ] For PR review: dispatch `design-implementation-reviewer` agent alongside pr-reviewer to catch visual issues (spacing, contrast, hierarchy)

## Testing
- **Unit/logic:** vitest (`cd ui_web && npm test`)
- **Visual/SSE verification:** fe-chrome-tester agent (requires `--chrome` flag)
- **Design iteration:** design-iterator agent (requires `--chrome` flag) — iteratively refines layout, spacing, typography
- **E2E:** only for critical flows — prefer vitest over Playwright for new tests

## Pitfalls
- `react-resizable-panels` v1 API (deprecated) vs v2 — always use v2
- ESLint blocks underscore-prefixed destructured vars
- SSE `EventSource` must be closed on unmount or you get leaked connections
- Vite 7 uses ESM — don't use CommonJS `require()`
- TS interfaces must match Rust `#[derive(Serialize)]` struct shapes exactly
- Design iteration requires `--chrome` flag and a running Vite dev server on port 5173
