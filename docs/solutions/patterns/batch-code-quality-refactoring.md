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

## Metrics from this batch

| Metric | Value |
|--------|-------|
| Tickets completed | 8 |
| PRs merged | 8 (+ 1 docs PR) |
| Lines removed (net) | ~150 production, established builders for ~1,750 test |
| Clippy suppressions removed | 2 |
| Safety guards added | 7 (float-to-int clamps) |
| Bugs fixed | 1 (f64::EPSILON) |
| Review findings fixed | 3 (all should-fix level) |
