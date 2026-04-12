---
title: "Serde unit variant wire format: bare string vs. object wrapper"
category: integration-issues
date: 2026-04-12
tags:
  - serde
  - rust
  - typescript
  - wire-format
  - task-state
severity: medium
components:
  - sim_core
  - ui_web
related_tickets:
  - VIO-677
  - VIO-682
---

## Symptom

```
TypeError: Cannot use 'in' operator to search for 'Idle' in Idle
    at countShipsByTask (snapshotSelector.ts:128)
```

The CopilotKit readable's `countShipsByTask` function throws when processing
ships in `Task::Idle` state. The `in` operator is used to discriminate
`TaskState.kind` variants, but `'Idle' in "Idle"` throws because `in` doesn't
work on primitives.

## Root Cause

Rust's serde default external tagging serializes enum variants differently
depending on whether they carry data:

| Variant | Rust | Serialized JSON |
|---------|------|-----------------|
| Unit (no data) | `Task::Idle` | `"Idle"` (bare string) |
| Struct (with data) | `Task::Mine { asteroid, duration_ticks }` | `{ "Mine": { "asteroid": "...", "duration_ticks": 50 } }` |

The TypeScript `TaskState` type in `ui_web/src/types.ts` models ALL variants
as object wrappers (`{ Idle: Record<string, never> }`), which is **incorrect
for the wire format**. The type compiles but the runtime data is a string for
unit variants.

## Solution

Guard any task-kind discrimination with a `typeof` check before using `in`:

```typescript
function isShipTask(
  ship: ShipState,
  kind: 'Idle' | 'Mine' | 'Transit' | 'Deposit' | 'Survey' | 'DeepScan',
): boolean {
  if (!ship.task) { return false; }
  const taskKind: unknown = ship.task.kind;
  // Serde unit variants → bare string; struct variants → object wrapper.
  if (typeof taskKind === 'string') { return taskKind === kind; }
  if (typeof taskKind === 'object' && taskKind !== null) {
    return kind in (taskKind as Record<string, unknown>);
  }
  return false;
}
```

## Prevention

- When consuming Rust-serialized enums in TypeScript, always verify the WIRE
  FORMAT (not just the Rust type) via `console.log(JSON.stringify(value))` at
  runtime. Serde external tagging produces strings for unit variants.
- The codebase has a pre-existing helper `getTaskKind()` in `ui_web/src/utils.ts`
  that also mishandles unit variants (`Object.keys("Idle")` returns `['0','1','2','3']`).
  See VIO-682 for the follow-up fix.
- Consider widening `TaskState['kind']` in `types.ts` to accept the union of
  string literals AND object wrappers, so TypeScript surfaces this at compile time.

## Related

- VIO-682: Fix `getTaskKind()` for `Task::Idle` string variant wire format
- `ui_web/src/copilot/snapshotSelector.ts`: contains the corrected `isShipTask` helper
