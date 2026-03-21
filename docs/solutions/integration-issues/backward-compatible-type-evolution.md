---
title: "Backward-compatible type evolution with serde(default)"
category: integration-issues
date: 2026-02-28
module: sim_core
component: types.rs
tags: [serde, backward-compat, save-files, serialization, type-evolution]
project: Heat System
tickets: [VIO-196, VIO-197, VIO-198, VIO-199, VIO-203]
---

## Problem

Adding new fields to serialized types breaks deserialization of existing save files and state JSON. When the Heat System H0 phase added 6 new type extensions (ThermalState, MaterialThermalProps, thermal constants, element properties, ThermalDef), each new field needed to be backward-compatible with saves created before the Heat System existed.

Without this, loading an old save would produce a deserialization error like:
```
missing field `thermal` at line X column Y
```

## Root Cause

Serde's default behavior requires all struct fields to be present in the serialized data. Adding a mandatory field to any type that appears in save files or content JSON creates an immediate backward-compatibility break.

## Solution

Every new optional field uses `#[serde(default)]`:

```rust
pub struct ModuleState {
    pub instance_id: ModuleInstanceId,
    pub wear: WearState,
    // ... existing fields ...

    #[serde(default)]  // <-- deserializes to None when missing
    pub thermal: Option<ThermalState>,
}
```

For non-Option fields with meaningful defaults, use explicit default functions:

```rust
pub struct Constants {
    // ...
    #[serde(default = "default_thermal_sink_temp_mk")]
    pub thermal_sink_temp_mk: u32,
}

fn default_thermal_sink_temp_mk() -> u32 {
    293_000 // 20C ambient
}
```

### Pattern applied consistently in H0

All 6 H0 tickets followed this pattern:
- `ThermalState` on `ModuleState` → `Option<ThermalState>` with `serde(default)`
- `MaterialThermalProps` on `InventoryItem::Material` → `Option<MaterialThermalProps>` with `serde(default)`
- 5 thermal constants → each with `serde(default = "default_...")`
- Element thermal properties → `Option<u32>` fields with `serde(default)`
- `ThermalDef` on `ModuleDef` → `Option<ThermalDef>` with `serde(default)`

### Test coverage

Each new field gets a backward-compat deserialization test:

```rust
#[test]
fn thermal_state_backward_compat() {
    // Old save format without thermal field
    let json = r#"{"instance_id": "mod1", "wear": {"value": 0.0}, ...}"#;
    let state: ModuleState = serde_json::from_str(json).unwrap();
    assert!(state.thermal.is_none());
}
```

## Prevention

- **Every new field on a serialized type** must use `#[serde(default)]`.
- If the field is not `Option<T>`, provide a `default = "fn_name"` that returns a sensible fallback.
- Always write a backward-compat test that deserializes JSON without the new field.
- Update test fixtures (`base_content()`, `minimal_content()`, etc.) to include the new field explicitly.
