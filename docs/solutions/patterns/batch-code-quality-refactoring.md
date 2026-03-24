---
title: Batch code quality refactoring workflow
category: patterns
date: 2026-03-23
tags: [code-quality, refactoring, workflow, linear, pr-review]
---

## Pattern

Autonomous batch execution of code quality tickets: pick 5-10 tickets from a Linear project, order by dependencies, implement each as branch→PR→review→merge→next.

## When to Use

- Code quality backlog has accumulated 10+ tickets
- Tickets are mostly independent refactors (not features)
- Each ticket is well-scoped with clear acceptance criteria

## Execution Order Strategy

1. **Quick wins first** (1-line fixes, safety guards) — builds momentum, catches easy regressions early
2. **Independent production refactors** (function decomposition, method extraction) — no dependency ordering needed
3. **Blocking chain** — tickets that unblock others go before their dependents (e.g., InventoryItem methods before TradeItemSpec methods)
4. **Test fixture work last** — production refactors may change APIs that tests depend on; doing fixtures last avoids rework

## Key Learnings

### Serde serialize-patch-deserialize for config overrides (VIO-406)

Replace large match statements mapping string keys to struct fields with:
```rust
let Value::Object(mut map) = serde_json::to_value(&*config)? else { unreachable!() };
if !map.contains_key(key) { bail!("unknown key '{key}'") }
map.insert(key.to_string(), value.clone());
*config = serde_json::from_value(Value::Object(map))?;
```

**Gotcha:** Fields with `#[serde(skip_deserializing)]` appear in the serialized map (pass validation) but are silently discarded on deserialization. Must explicitly reject these keys to prevent silent no-ops. Solution: maintain a `DERIVED_FIELDS` constant and check before insertion.

### Enum method extraction (VIO-404, VIO-405, VIO-407)

When an enum is pattern-matched in N standalone functions, move those matches into methods on the enum itself. Benefits:
- Adding a new variant requires changes in one `impl` block, not N scattered files
- Clippy's method-reference syntax (`Type::method`) avoids redundant closure warnings
- `filter_map` → `filter + map(Type::method)` is more idiomatic

### Function decomposition with context structs (VIO-409)

When extracting helpers from a large function that shares many parameters:
1. Group shared read-only params into a context struct
2. Pass `&mut GameState` separately (the mutable state)
3. Return computed values instead of mutating through the context
4. Check both `clippy::too_many_lines` (100 limit) AND `clippy::too_many_arguments` (7 limit) after extraction

### PR review catches real issues

Every PR in this batch got a pr-reviewer dispatch. Findings that prevented bugs:
- **VIO-416**: Test used `0.1 + 0.2 == 0.3` which doesn't actually exceed `f64::EPSILON` — would have passed with the old broken code. Fixed to use `0.3_f32 as f64` vs `0.3_f64` which genuinely demonstrates the f32→f64 conversion bug.
- **VIO-406**: Derived fields (`skip_deserializing`) would silently accept overrides that have no effect. Added explicit rejection.

## Batch 2 Learnings (2026-03-24)

Tickets: VIO-358, VIO-410, VIO-411, VIO-413, VIO-417, VIO-418, VIO-419

### File reorganization breaks CI scripts with hardcoded paths

When splitting `types.rs` (2,195 lines) into `types/` submodules (VIO-411), `scripts/ci_event_sync.sh` broke because it hardcoded `crates/sim_core/src/types.rs`. After any file move or split, **grep the repo for hardcoded paths to the moved file** — especially CI scripts, linters, and documentation.

### Rust module split pattern for backward compatibility

Convert `foo.rs` to `foo/mod.rs` + submodules:
1. `mkdir foo/ && git mv foo.rs foo/mod.rs`
2. Create submodule files (`foo/state.rs`, `foo/content.rs`, etc.)
3. In `mod.rs`: declare `mod submod;` + `pub use submod::*;` for each
4. Submodules use `crate::TypeName` for cross-references (works because lib.rs re-exports everything)
5. Serde `#[serde(default = "fn_name")]` functions must stay in the same file as the struct — they resolve relative to the defining module

**Gotcha:** `use super::*` from a submodule imports types re-exported from sibling submodules, which can conflict with local definitions. Use specific `crate::` imports instead.

### Python scripts for mechanical Rust refactoring bypass hooks

Using a Python script to transform 125 `.unwrap()` → `?` across 27 test functions (VIO-419) was efficient, but the after-edit hook (`cargo fmt` + `cargo test`) only runs on Edit/Write tool calls. **Always run `cargo fmt` manually after Python-scripted transformations.**

### Curating explicit exports requires workspace-wide analysis

Replacing `pub use types::*` with explicit re-exports (VIO-417) required checking all 5 downstream crates. Nearly all types were used — the explicit list was 110 items. Use `rust_analyzer_references` or grep `use crate_name::` across the workspace to build the list. The value is documentation (making the API surface intentional), not reduction.

### HashMap serialization order is non-deterministic

Determinism canary tests (VIO-413) initially compared serialized state as strings, which failed because `HashMap` key order varies between runs. Fix: use `serde_json::to_value()` for order-independent comparison, not string comparison.

## Metrics

### Batch 1 (2026-03-23)

| Metric | Value |
|--------|-------|
| Tickets completed | 8 |
| PRs merged | 8 (+ 1 docs PR) |
| Lines removed (net) | ~150 production, established builders for ~1,750 test |
| Clippy suppressions removed | 2 |
| Safety guards added | 7 (float-to-int clamps) |
| Bugs fixed | 1 (f64::EPSILON) |
| Review findings fixed | 3 (all should-fix level) |

### Batch 2 (2026-03-24)

| Metric | Value |
|--------|-------|
| Tickets completed | 7 (1 closed as dup) |
| PRs merged | 6 |
| Largest file split | types.rs 2,195 → 7 files (mod.rs ~190, state ~450, content ~560, events ~280, commands ~90, inventory ~180, constants ~320) |
| Constants extracted | 7 (previously hardcoded in production code) |
| Test unwraps removed | 125 → 0 in sim_daemon |
| Clippy suppressions removed | 2 (too_many_arguments) |
| Review findings fixed | 4 (CI path, import consistency, docstring accuracy, debug format) |
