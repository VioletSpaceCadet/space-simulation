---
title: "CI-enforced event sync between Rust backend and React frontend"
category: integration-issues
date: 2026-02-27
module: sim_core, ui_web
component: Event enum, applyEvents.ts
tags: [sse, events, ci, be-fe-sync, exhaustiveness]
project: BE/FE Sync Audit
tickets: [VIO-81, VIO-74, VIO-75, VIO-76, VIO-77, VIO-78, VIO-79]
---

## Problem

When new `Event` variants were added to the Rust backend (`sim_core/src/types.rs`), the React frontend silently ignored them. SSE events were emitted but never applied to UI state. Over time, this caused:

- Economy events (ItemImported, ItemExported) not updating balance or inventory
- Lab module events (LabRan, LabStarved) not reflected in UI
- Module lifecycle events (ModuleUninstalled, ModuleStalled) leaving stale UI state
- TypeScript types drifting from Rust types (AssemblerState missing `capped`, ModuleKindState missing SensorArray)

The root cause was **no exhaustiveness check** across the Rust/TypeScript boundary. Rust's `match` enforces exhaustiveness within a single language, but the FE handler map had no equivalent enforcement.

## Root Cause

There was no dev-time or CI-time mechanism to detect when a Rust `Event` variant lacked a corresponding handler in `applyEvents.ts`. Developers adding new events to sim_core had no feedback that the frontend needed updating.

Additionally, `ModuleInstalled` handler used fragile string matching on `module_def_id` to guess module type, which broke when new module types (SensorArray, SolarArray, Battery) were added.

## Solution

### 1. CI event sync script (`scripts/ci_event_sync.sh`)

A shell script that:
1. Parses all `Event` variant names from `pub enum Event` in `types.rs`
2. Extracts handler keys from the handler map in `applyEvents.ts`
3. Uses `comm -23` to find variants with no FE handler
4. Fails CI if any are missing

This runs as part of the CI pipeline. When a developer adds a new Event variant without a FE handler, CI blocks the PR.

### 2. Handler map pattern in applyEvents.ts

Instead of a `switch` statement, events use a typed handler map:

```typescript
const handlers: Record<EventType, (state: SimState, payload: any) => void> = {
  AsteroidDiscovered: handleAsteroidDiscovered,
  LabRan: handleLabRan,
  OverheatWarning: noOp,  // informational events get noOp
  // ...
};
```

Informational events that don't mutate UI state use a `noOp` handler, making the "no handler needed" decision explicit.

### 3. Module type detection via behavior_kind field

Replaced string matching (`module_def_id.includes('maintenance')`) with a `behavior_kind` field on ModuleInstalled events. The backend sends the canonical type, eliminating guesswork.

```typescript
const MODULE_KIND_STATE_MAP: Record<string, ModuleKindState> = {
  Processor: { Processor: { ... } },
  Maintenance: { Maintenance: { ... } },
  Radiator: { Radiator: {} },
  // ...
};
```

## Prevention

- **Always run `./scripts/ci_event_sync.sh`** after adding a new Event variant. The PostToolUse hook doesn't run it automatically, but CI catches it.
- When adding a new Event variant: add a handler in `applyEvents.ts` OR add to the allow-list in `ci_event_sync.sh` (with a comment explaining why it's skipped).
- When adding a new module behavior type: add an entry to `MODULE_KIND_STATE_MAP` in applyEvents.ts.
- Run `./scripts/ci_event_sync.sh` locally before pushing if you've touched `types.rs` events.
