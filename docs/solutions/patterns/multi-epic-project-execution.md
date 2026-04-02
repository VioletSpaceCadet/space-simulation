---
title: "Multi-Epic Project Execution: Cross-Epic Obsolescence and Dual-Path State Init"
category: patterns
date: 2026-03-21
tags: [project-planning, epic-coordination, state-initialization, build-initial-state, dev-base-state, autopilot, content-driven-features]
severity: high
component: sim_world, sim_control, ui_web, content
related_tickets: [VIO-96, VIO-97, VIO-98, VIO-99, VIO-100, VIO-101, VIO-102, VIO-103, VIO-104, VIO-105, VIO-256, VIO-257, VIO-258, VIO-259, VIO-318, VIO-328, VIO-329, VIO-330, VIO-331]
---

# Multi-Epic Project Execution: Cross-Epic Obsolescence and Dual-Path State Init

## Problem

Epic 2 (Asteroid Resource Typing) was a 14-ticket Linear project planned in parallel with Epic 1 (Spatial Positioning). By the time Epic 2 started, Epic 1 had already shipped 5 of the 14 tickets' functionality. Additionally, updating `dev_advanced_state.json` without mirroring changes to `build_initial_state()` caused the heating module to be silently absent from MCP-started simulations, and adding distant zones (Jupiter Trojans) exposed a proximity-blind autopilot routing bug.

10 tickets merged via 9 PRs (#110-#118). 5 tickets cancelled as obsolete. Testing revealed 4 new bugs (VIO-328 through VIO-331).

## Key Learnings

### 1. Cross-Epic Ticket Staleness — Audit Before Starting

**Planned:** 14 tickets covering ResourceClass enum, weighted template selection, variable travel time, H2O element, volatile templates, heating module, FE tags, autopilot targeting, tests, bench scenarios.

**Actual:** 5 tickets (VIO-96, VIO-99, VIO-256, VIO-101, VIO-257) were already implemented by Epic 1's spatial model. `ResourceClass`, `pick_zone_weighted()`, `pick_template_biased()`, and `travel_ticks()` all existed in `spatial.rs`. VIO-100 (solar system expansion) needed a complete rewrite from graph nodes + hop_dv edges to spatial bodies.

**Discovery method:** Before starting any tickets, searched the codebase for the types and functions named in ticket descriptions. Found them all already present.

**Rule:** Before starting any epic that was planned before a dependency epic shipped, spend 15 minutes grepping the codebase for the key types and functions each ticket assumes it will create. Treat ticket text as a hypothesis, not a contract.

### 2. Dual-Path State Initialization Divergence (VIO-328)

Two code paths produce initial game state:
- `content/dev_advanced_state.json` — loaded by CLI `--state` flag
- `build_initial_state()` in `sim_world/src/lib.rs` — called by daemon / MCP `start_simulation`

VIO-103 added a heating unit (`module_item_0016`) and third solar array (`module_item_0015`) to `dev_advanced_state.json` but did not update `build_initial_state()`. MCP-started simulations silently ran without the heating module:
- `station_has_heating_module()` always returned `false`
- `needs_water` always `false`
- Volatile targeting never activated
- H2O production = 0

**Detection:** sim-e2e-tester agent at tick 25,800 found `total_material_kg: 100.0` (only starting Fe, zero H2O ever produced).

**Rule:** When adding modules or content to `dev_advanced_state.json`, always check whether `build_initial_state()` needs the same addition. Better yet, add a CI test that asserts both paths produce stations with the same module set.

### 3. Autopilot Proximity-Blind Survey Selection (VIO-329)

With the expanded solar system (6 zones, Jupiter Trojans at 5.2 AU), the autopilot's `next_site` iterator picked scan sites in insertion order with no distance consideration. Seed 42: sole ship dispatched to Jupiter Trojans at tick 4,300 with a 23,906-tick round trip — production chain idle for 24k ticks.

**Rule:** Any time you expand the range of values an autopilot iterates over (new zones, new resource types), audit whether the selection order is still sensible. Insertion-order iteration is a latent bug waiting for a new distant entry. Sort survey candidates by distance from nearest station.

### 4. Content-Only Tickets via Existing Infrastructure

Several tickets required zero code changes because the sim's frameworks generalize well:
- VIO-98 (H2O element) — content-only, element loader handles arbitrary IDs
- VIO-97 (templates) — content-only, `pick_template_biased()` handles new templates automatically
- VIO-102 (heating module) — content-only, Processor recipe system handles arbitrary elements
- VIO-100 (new zones) — content-only, `pick_zone_weighted()` handles new zones automatically

Only VIO-105 (autopilot volatile targeting) and VIO-104 (FE tag badges) required actual code changes. This is a sign of healthy data-driven infrastructure.

**Signal:** If a ticket maps cleanly onto an existing content file schema, it's likely content-only. Verify by checking whether the loader and runtime logic already handle the new data generically.

## Prevention Checklist

**Before starting a multi-ticket epic:**
- [ ] Grep for key types/functions from each ticket — confirm they don't already exist
- [ ] Cancel obsolete tickets with explanation of what already ships them
- [ ] Rewrite tickets whose implementation approach was invalidated (not just the title)

**When adding content to dev_advanced_state.json:**
- [ ] Check if `build_initial_state()` needs the same change
- [ ] Run MCP `start_simulation` and verify the new content appears in the snapshot
- [ ] Or add a CI test asserting both paths produce matching module inventories

**When adding zones at new distances:**
- [ ] Document expected one-way transit time in ticks
- [ ] Verify autopilot routing still makes sensible choices at that distance
- [ ] File a companion ticket for proximity-weighted selection if needed

**For content-only features:**
- [ ] Write at least one integration test that runs N ticks and asserts output
- [ ] "The infra handles it" is not sufficient to skip the test
- [ ] Test the full production chain, not just the first link

## Related Documentation

- [Cross-Layer Enum Refactor and DAG UI](cross-layer-enum-refactor-and-dag-ui.md) — atomic enum rename cascade pattern
- [Cross-Layer Feature Development](cross-layer-feature-development.md) — architectural template for multi-layer features
- [Hierarchical Polar Coordinate Migration](hierarchical-polar-coordinate-migration.md) — the Epic 1 spatial model that made 5 Epic 2 tickets obsolete
- [Gameplay Deadlock: Missing Starting Equipment](../logic-errors/gameplay-deadlock-missing-starting-equipment.md) — prior instance of `build_initial_state()` / `dev_advanced_state.json` divergence
