---
title: "Deterministic simulation via integer arithmetic and sorted collections"
category: logic-errors
date: 2026-02-28
module: sim_core
component: thermal.rs, station/mod.rs, types.rs
tags: [determinism, integer-arithmetic, milli-kelvin, float-clamping, sorted-collections]
project: Heat System
tickets: [VIO-196, VIO-200, VIO-204]
---

## Problem

The simulation must be deterministic: given the same seed, content, and commands, it must produce identical state evolution. Floating-point arithmetic and unordered collection iteration both threaten this invariant.

During the Heat System implementation, temperature calculations introduced new floating-point boundaries. Without careful design, float-to-int conversion could produce platform-dependent results, and iterating thermal groups in HashMap order would cause non-deterministic cooling.

## Root Cause

Two sources of non-determinism in Rust simulations:

1. **Float-to-int casts**: Rust's `as` cast on out-of-range floats is implementation-defined behavior. A temperature calculation producing `f64::INFINITY` cast to `i32` gives different results on different platforms.

2. **HashMap iteration order**: `HashMap` does not guarantee iteration order. When RNG is consumed during iteration (e.g., processing modules that use randomness), different iteration orders produce different random sequences, breaking determinism.

## Solution

### Integer types for sim state

All simulation state uses integer types:
- **Temperature**: `u32` milli-Kelvin (293,000 mK = 20C)
- **Energy**: `i64` Joules
- **Power**: `f32` Watts in content definitions only, converted at boundary

Floats appear only in:
- Content definitions (JSON-defined constants)
- Conversion boundary functions in `thermal.rs`

### Explicit clamping before float-to-int casts

Every float-to-int conversion clamps to the target type's range first:

```rust
// BAD: undefined behavior for out-of-range values
let delta = some_float as i32;

// GOOD: clamped to safe range
let delta = some_float.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
```

The `thermal.rs` module documents this with `#[allow(clippy::cast_possible_truncation)]` comments explaining the safety guarantee.

### Sorted collection iteration

All collection iteration that precedes RNG use or state mutation uses sorted order:

```rust
// Thermal groups processed via BTreeMap (sorted by group ID)
let mut groups: BTreeMap<&ThermalGroupId, Vec<&ModuleInstanceId>> = BTreeMap::new();

// Within each group, modules sorted by ID
group_modules.sort();
```

This applies everywhere in the tick loop: module processing, ship task resolution, research rolls.

### Seeded RNG

`sim_core::tick()` accepts `&mut impl rand::Rng`. Concrete type is `ChaCha8Rng` (seeded, deterministic, cross-platform) in `sim_cli` and `sim_daemon`.

## Prevention

- When adding new sim state fields, prefer integer types. Use milli-units (milli-Kelvin, milli-percent) to avoid floats.
- Every `as` cast from float to integer MUST be preceded by `.clamp()`.
- Never iterate `HashMap` when the iteration order could affect state mutations or RNG consumption. Use `BTreeMap` or sort keys first.
- Run `sim_bench` with multiple seeds to verify determinism: identical seeds must produce identical metrics.
