---
title: Processor FIFO functions hardcode Ore-only input, blocking Material→Material recipes
category: logic-errors
date: 2026-03-22
tags: [sim_core, processor, material, electrolysis, epic3, fifo]
ticket: VIO-108
pr: 129
---

## Problem

The Processor module claimed to support Material→Material recipes (e.g., electrolysis: H2O → LH2 + LOX) via `InputFilter::Element("H2O")`. The `matches_input_filter()` function correctly matched Material items. But the processor never consumed them — electrolysis produced nothing.

## Root cause

Three functions in `crates/sim_core/src/station/processor.rs` hardcoded Ore-only handling:

1. **Threshold check** (lines 56–71): Only counted `InventoryItem::Ore` kg against the processor threshold. Material items were invisible — even 5000 kg of H2O would read as "0 kg available" and skip processing.

2. **`peek_ore_fifo_with_lots`**: Filter condition was `matches!(item, InventoryItem::Ore { .. }) && filter(item)`. Material items matching the filter were silently skipped.

3. **`consume_ore_fifo_with_lots`**: Same Ore-only guard. Material items were never consumed, never produced output.

The bug was latent — no Material→Material recipe existed until the electrolysis module was added in Epic 3.

## Solution

**Threshold check**: Moved recipe/filter extraction before threshold check. Used `matches_input_filter()` against the recipe's actual input filter instead of hardcoded Ore counting:

```rust
let input_filter_for_threshold = recipe.inputs.first().map(|i| &i.filter);
let total_input_kg: f32 = state.stations.get(&ctx.station_id).map_or(0.0, |s| {
    s.inventory.iter()
        .filter(|item| matches_input_filter(item, input_filter_for_threshold))
        .filter_map(|i| match i {
            InventoryItem::Ore { kg, .. }
            | InventoryItem::Material { kg, .. }
            | InventoryItem::Slag { kg, .. } => Some(*kg),
            _ => None,
        })
        .sum()
});
```

**FIFO functions**: Added `InventoryItem::Material` arms alongside the existing `Ore` arms. For Material items, composition is `{element: 1.0}` (single-element, 100%):

```rust
InventoryItem::Material { element, kg, quality, thermal } => {
    let take = kg.min(remaining);
    remaining -= take;
    consumed_kg += take;
    lots.push((HashMap::from([(element.clone(), 1.0)]), take));
    let leftover = kg - take;
    if leftover > MIN_MEANINGFUL_KG {
        new_inventory.push(InventoryItem::Material {
            element, kg: leftover, quality, thermal,
        });
    }
}
```

**Verified by**: 8 acceptance tests covering production, stoichiometry, consumption, wear, stalling, power gating, continuous production, and full ore→H2O→LH2+LOX chain.

## Prevention

- **When adding a new `InputFilter` variant or recipe type**, write an end-to-end test that runs the full tick loop — don't rely on `matches_input_filter()` alone. The filter matching may work while the FIFO consumption silently skips the matched items.
- **PR review checklist item**: If a Processor recipe uses a non-Ore input filter, verify the FIFO functions handle that item type. Grep for `InventoryItem::Ore` guards in `processor.rs`.
- The processor now handles Ore and Material inputs. If future recipes need Slag or Component inputs, the same FIFO extension pattern applies.

## Related

- VIO-108 (PR #129): The fix PR
- VIO-107 (PR #128): Electrolysis module definition that exposed the bug
- `crates/sim_core/src/station/processor.rs`: The three fixed functions
- `crates/sim_core/src/tests/electrolysis.rs`: 8 acceptance tests
