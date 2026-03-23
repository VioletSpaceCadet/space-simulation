---
title: Recipe Catalog Extraction and Manufacturing DAG System
category: patterns
date: 2026-03-23
tags: [recipe-catalog, content-extraction, manufacturing-priority, tech-gates, dag-visualization, multi-ticket, refactor-cascade, subagent-parallelism]
severity: medium
component: sim_core, sim_world, sim_daemon, ui_web
related_tickets: [VIO-369, VIO-370, VIO-371, VIO-372, VIO-373, VIO-374]
---

# Recipe Catalog Extraction and Manufacturing DAG System

## Problem

Recipes were inlined within `ProcessorDef.recipes` and `AssemblerDef.recipes` as `Vec<RecipeDef>`, selected by index (`selected_recipe_idx: usize`). This made it impossible to build multi-tier manufacturing chains, share recipes across module types, or visualize production dependencies. 6-ticket project to extract recipes to a standalone catalog and build the full manufacturing DAG system.

6 PRs merged (#194, #199, #201, #203, #208, #213) across sim_core, sim_world, sim_daemon, and ui_web.

## Key Learnings

### 1. Inline-to-Catalog Extraction Creates a Replace-All Cascade

**Pattern:** Changing `Vec<RecipeDef>` to `Vec<RecipeId>` in type definitions requires updating every struct literal construction across the entire codebase simultaneously — Rust won't compile with mixed old/new types.

**Scale:** `selected_recipe_idx: 0` appeared in 55+ locations across 15 files (test fixtures, station/mod.rs install_module, metrics.rs, sim_control/lib.rs). All needed to become `selected_recipe: None`.

**Technique:** Use `Edit` with `replace_all: true` for the mechanical rename, then fix the semantic changes (command tests, recipe resolution logic) individually. Read each file first (required by the tool), then batch the `replace_all` calls in parallel.

**Rule:** When migrating a field that appears in struct literals across many files, do the mechanical rename first with `replace_all`, then handle the behavioral changes.

### 2. Test Fixtures Need Recipe Catalog Population

When recipes move from inline `ProcessorDef` to `GameContent.recipes`, every test content function that constructs a `ProcessorDef` or `AssemblerDef` with recipes must also populate `content.recipes`.

**Solution:** Create helper functions in `test_fixtures.rs`:
- `test_iron_recipe()` → returns a standard test RecipeDef
- `test_smelt_recipe()` → returns a thermal recipe
- `insert_recipe(&mut content, recipe) -> RecipeId` → inserts and returns the ID

This pattern avoids duplicating recipe construction across 20+ test content functions.

### 3. Subagents for Bulk Refactoring Across Many Files

The test fixture migration touched 15+ files with similar but not identical changes. Two parallel subagents (one for sim_core tests, one for external crates) handled this efficiently:

- **sim_core agent:** 15 files, all test content functions with inline recipes
- **external crate agent:** sim_control, sim_bench, sim_world — content loading + validation updates

**Key for subagent success:** Include the complete file operation rules block, the exact before/after pattern, and list every file that needs changes. Agents that know exactly which files to touch finish much faster than those that need to search.

### 4. Recipe Resolution Pattern: Fallback + Emit

The `resolve_recipe()` function handles invalid state gracefully:
1. Try `selected_recipe` from module state
2. If set but not in module's recipe list → fall back to first recipe, emit `RecipeSelectionReset` event, mutate state
3. If None → use first recipe in module's list

**Pitfall caught by reviewer:** Don't call `resolve_recipe()` twice per tick (once in `execute()`, once in `resolve_processor_run()`). The first call mutates state on fallback; the second is wasteful and could re-emit events. Pass the resolved recipe ID through instead.

### 5. Component Output Must Use Modifier System

When adding a new output type (Component) to processor.rs, it's tempting to compute quality directly (`quality * thermal_quality`). But the Material path routes quality through `proc_mods.resolve_with_f32()` which applies global modifiers. Component output must follow the same path for consistency.

**Rule:** Any new output type in processor.rs must route through the modifier system and include `.clamp(0.0, 1.0)` on quality calculations.

### 6. FE Recipe Types: Serde Tagged Enums Need `Record<string, unknown>`

Rust's serde externally-tagged enums (like `InputFilter::ElementWithMinQuality { element, min_quality }`) serialize as `{ "ElementWithMinQuality": { "element": "Fe", "min_quality": 0.5 } }`. The FE type for the filter field must be `Record<string, unknown>` (not `Record<string, string>`) to handle nested object values.

### 7. Flow Stats: Assembler vs Processor Throughput

Processors produce kg (material output), assemblers produce counts (component output). A single `throughput_per_hour` field that only divides `total_output_kg` by window hours will always be 0 for assemblers. Use whichever output measure is non-zero:
```typescript
const outputTotal = stat.total_output_kg > 0 ? stat.total_output_kg : stat.total_output_count;
```

## Prevention

- **Before extracting inline data to a catalog:** Grep for all struct literal constructions and list them. The migration is proportional to the number of construction sites, not the number of types changed.
- **Test helpers for shared content:** When content moves to a central catalog, create test fixture helpers immediately — don't let each test construct its own copy.
- **PR reviewer catches parity bugs:** Both the modifier system bypass and the double-resolution were caught by the pr-reviewer agent. Always dispatch review, even for seemingly straightforward changes.
