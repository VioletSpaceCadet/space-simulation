---
title: "Gameplay deadlock from missing starting equipment"
category: logic-errors
date: 2026-02-24
module: sim_world
component: build_initial_state, dev_advanced_state.json
tags: [deadlock, starting-state, dependency-chain, balance, gameplay-loop]
project: Balance & Tuning
tickets: [VIO-5, VIO-7]
---

## Problem

The gameplay loop was completely deadlocked. Neither `dev_advanced_state.json` nor `build_initial_state()` included labs, which meant:

```
Mine requires → knowledge.composition (set by DeepScan)
DeepScan requires → tech_deep_scan_v1 unlocked
tech_deep_scan_v1 requires → 100 Exploration domain points
Domain points require → Exploration Lab installed and running
Exploration Lab requires → module_exploration_lab in station inventory
Neither starting state included labs → DEADLOCK
```

No player action could break the cycle. The sim would run indefinitely with ships idling and no progression.

## Root Cause

The starting state was designed incrementally — maintenance bay first, then refinery and assembler were added later. Labs were never included because they were added in a separate system (Research). Nobody traced the full dependency chain from "player wants to mine" back to "what equipment must exist at game start."

Additionally, `build_initial_state()` (programmatic) and `dev_advanced_state.json` (authored JSON) had diverged. The programmatic version only had a maintenance bay and 5 repair kits, while the JSON version had a refinery, assembler, and more starting materials.

## Solution

1. **Added labs to dev_advanced_state.json** (VIO-5): Added `module_exploration_lab` and `module_materials_lab` to starting station inventory, enabling the research → tech unlock → deep scan → mining chain.

2. **Synced build_initial_state()** (VIO-7): Updated the programmatic initial state to match the JSON version — refinery, assembler, maintenance bay, labs, 500 kg Fe, 10 repair kits.

3. **Traced the full dependency chain**: Mapped every gameplay progression dependency to verify the starting state bootstraps all required loops.

## Prevention

- When adding a new system that gates an existing gameplay loop (e.g., research gates mining), verify the starting state includes the minimum equipment to enter that loop.
- Trace dependency chains end-to-end: "What does the player need to do X?" → "What equipment enables that?" → "Is that equipment in the starting state?"
- Keep `build_initial_state()` and `dev_advanced_state.json` in sync. If you update one, update the other.
- Use `sim_bench` to run extended scenarios (30-day, 90-day) — deadlocks show up as flat metrics (zero progression over time).
