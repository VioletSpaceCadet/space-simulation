# Repair Kit Oversupply Fix (VIO-15)

## Problem

The assembler produces repair kits far faster than maintenance consumes them, leading to 694 kits stockpiled at 90 days. No scarcity pressure, wasted storage, no trade-off for assembler time.

## Changes

### 1. Assembler interval: 120 → 360 ticks (2h → 6h)

Update `assembly_interval_ticks` in `content/module_defs.json`. Production drops from 12 kits/day to 4 kits/day.

### 2. Material cost: 100 kg → 200 kg Fe per kit

Update the repair kit recipe input in `content/module_defs.json`. Assembler consumes 800 kg Fe/day instead of 1,200 kg.

### 3. Content-driven production cap (`max_stock`)

Add `max_stock: Option<HashMap<ComponentId, u32>>` to `AssemblerBehaviorDef`. Default: `{"repair_kit": 50}`.

Before running a recipe, `tick_assembler_modules` counts matching components in station inventory. If count >= cap for any output component, skip the run (don't reset timer — re-check next tick). Emit `AssemblerCapped` event when first hitting the cap, `AssemblerResumed` when dropping below.

### 4. Autopilot command to adjust the cap

Add `SetAssemblerCap { station_id, module_id, component_id, max_stock }` command. Overrides the content-defined cap at runtime, stored on `AssemblerKindState`. Autopilot logic to dynamically adjust is a future enhancement — this adds the command plumbing only.

## What doesn't change

- Assembler recipe structure, output specs, wear mechanics
- Maintenance bay behavior
- `dev_base_state.json` (starts with 10 kits, well under cap)

## Expected impact

| Scenario | Before | After |
|----------|--------|-------|
| 2-week | ~168 kits | ~50 (capped) |
| 90-day | ~694 kits | ~50 (capped) |

Early game still works: 4 kits/day at 200 kg Fe each is sustainable with a single refinery.

## Files touched

- `crates/sim_core/src/types.rs` — `max_stock` on `AssemblerBehaviorDef`, `cap_override` on `AssemblerKindState`, new command variant, new event variants
- `crates/sim_core/src/station.rs` — cap check in `tick_assembler_modules`, command handling
- `content/module_defs.json` — interval, cost, max_stock values
- `crates/sim_core/src/tests/assembler.rs` — tests for cap behavior
- `docs/reference.md` — update assembler docs
