---
title: Cross-Layer Enum Refactor with DAG-Based UI
category: patterns
date: 2026-03-21
tags: [enum-rename, cross-layer, serde, dagre, progressive-disclosure, memoization, event-emission, content-endpoint]
severity: medium
component: sim_core, sim_daemon, ui_web
related_tickets: [VIO-273, VIO-274, VIO-275, VIO-276, VIO-277, VIO-278, VIO-292, VIO-293, VIO-294, VIO-295]
---

# Cross-Layer Enum Refactor with DAG-Based UI

## Problem

Replaced a 3-domain research model (DataKind: ScanData/MiningData/EngineeringData, ResearchDomain: Materials/Exploration/Engineering) with a 4-domain model (SurveyData/AssayData/ManufacturingData/TransitData, Survey/Materials/Manufacturing/Propulsion). Required changes across Rust sim_core, JSON content, daemon API, and React UI — including a new DAG-based tech tree visualization.

10 tickets planned across 3 layers; 5 PRs merged (#103-107).

## Key Learnings

### 1. Enum Rename Cascade Must Be Atomic

**Planned:** 5 separate tickets for enum definition (RD-01), call site updates (RD-02), content file remap (RD-03), test fixtures (RD-05), and FE types (RD-07).

**Actual:** RD-01 had to do all of them in a single PR because Rust won't compile with partial renames. Old enum variants don't exist after the definition changes, so every call site, content file, and test fixture must update simultaneously.

**Result:** 5 of 10 tickets were completed by the first PR. The remaining 5 (TransitData generation, docs, daemon API, TechTreeDAG, ResearchPanel) were genuine new work.

**Rule:** When renaming serialized Rust enums that flow to JSON and TypeScript, treat the entire rename as one atomic changeset. Don't split across tickets — Rust's compiler won't let you do it incrementally anyway.

### 2. Event Emission Pattern for New Call Sites

When adding `generate_data()` for TransitData in `resolve_transit()`, the initial implementation discarded the return value and didn't emit `DataGenerated`. The pr-reviewer caught this — the FE relies on SSE events to update the Research Panel's data pool in real-time.

**Pattern:** Before adding a new call to an effect-producing function, scan existing call sites for the emission pattern:

```rust
// Existing pattern in resolve_survey / resolve_deep_scan:
let data_amount = crate::research::generate_data(&mut state.research, DataKind::SurveyData, "survey", &content.constants);
events.push(crate::emit(&mut state.counters, current_tick, Event::DataGenerated { kind: DataKind::SurveyData, amount: data_amount }));

// New call site MUST follow the same pattern
```

**Note:** Pre-existing tech debt — `resolve_mine` and `assembler.rs` also omit the event emission.

### 3. dagre Layout Memoization with Structural Keys

`useMemo` with Map/Array dependencies re-runs on every render because `computeTreeState()` returns new object references each time. At high sim speeds (1000 ticks/sec), this caused dagre to re-layout the entire graph every 100ms.

**Fix:** Compute a structural equality key from node IDs + edge pairs:

```typescript
const structureKey = useMemo(() => {
  const nodeIds = [...treeState.nodes.keys()].sort().join(',');
  const edgeIds = treeState.edges.map(e => `${e.from}-${e.to}`).sort().join(',');
  return `${nodeIds}|${edgeIds}`;
}, [treeState.nodes, treeState.edges]);

const layout = useMemo(
  () => computeLayout(treeState.nodes, treeState.edges),
  // eslint-disable-next-line react-hooks/exhaustive-deps
  [structureKey],
);
```

**Rule:** When memoizing expensive computations that depend on derived data structures, use structural equality keys instead of raw object references.

### 4. cargo clippy --fix Then cargo fmt

`cargo clippy --fix` produces `From::from()` conversions that may exceed line length limits. Local rustfmt may not wrap them, but CI rustfmt does. Always run `cargo fmt` after `cargo clippy --fix` before committing.

### 5. Content Endpoint vs Snapshot Bloat

The FE needed tech definitions (for DAG), lab rates (for status), and data pool rates (for display). Rather than embedding in the snapshot (which would bloat every SSE tick), a separate `GET /api/v1/content` endpoint was created. The `useContent` hook refetches every 30 seconds.

**Pattern:** Separate static/rarely-changing data (tech definitions) from dynamic state (tick data). Use a dedicated endpoint with periodic refetch instead of inflating the event stream.

## Prevention Checklist

**For enum renames crossing serialization boundaries:**
- [ ] Single PR covering: Rust enums, all call sites, JSON content, FE types/test fixtures
- [ ] `cargo build` compiles after all changes
- [ ] `cargo test` + `npm test` pass
- [ ] Event sync check passes (`scripts/ci_event_sync.sh`)

**For new generate_data() or similar effect-producing call sites:**
- [ ] Scan existing call sites for event emission pattern
- [ ] Capture return value and emit corresponding Event
- [ ] Verify FE event handler exists in `applyEvents.ts`

**For expensive useMemo in components receiving SSE updates:**
- [ ] Check if dependencies are new object references on each render
- [ ] If so, compute a structural key from stable identifiers
- [ ] Re-run layout only on structural changes, not value changes

## Related Documentation

- [Cross-Layer Feature Development](cross-layer-feature-development.md) — architectural template for multi-layer features
- [Event Sync Enforcement](../integration-issues/event-sync-enforcement.md) — CI script for Rust↔TS event exhaustiveness
- [Backward-Compatible Type Evolution](../integration-issues/backward-compatible-type-evolution.md) — `serde(default)` requirements for new fields
- [Iterative FE Design with Visual Companion](iterative-fe-design-with-visual-companion.md) — design process used for the research panel mockups
