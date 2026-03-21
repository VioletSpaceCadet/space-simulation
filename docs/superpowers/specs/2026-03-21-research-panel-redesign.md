# Research Panel Redesign

**Date:** 2026-03-21
**Status:** Approved
**Mockup:** [research-panel-mockup.html](./2026-03-21-research-panel-mockup.html)

## Overview

Replace the basic research panel (flat list of tech IDs with evidence numbers) with a DAG-based tech tree visualization with progressive disclosure, domain progress bars, data pool rates, and lab status.

The FE is a pure renderer — all tree structure (nodes, edges, prereqs, domain requirements) comes from the backend via snapshot and SSE events. The FE hardcodes nothing about tree topology.

## Sections

The panel has three sections, top to bottom:

### 1. Data Pool

Shows current data reserves by kind with net generation/consumption rates.

```
Survey       142.3  +1.2/hr
Assay         87.1  -0.4/hr
Manufacturing 51.8  +0.8/hr
Transit       23.4  +0.3/hr
```

- 2-column grid layout
- Color-coded by data kind: Survey (blue #5ca0c8), Assay (gold #c89a4a), Manufacturing (green #4caf7d), Transit (purple #a78bfa)
- Net rate shown inline: green for positive, red for negative
- Rate = generation (sensors, tasks) minus consumption (labs) per game hour

### 2. Tech Tree (DAG)

A tier-based top-to-bottom directed acyclic graph.

**Rendering approach:** Hybrid HTML nodes + SVG edges. Use dagre for layout positioning. Nodes are styled HTML cards, edges are SVG lines drawn in an overlay. Designed for 10-20+ nodes with scrolling.

**"Researching" definition:** A tech is "researching" when (a) at least one lab has `assigned_tech` pointing to it, AND (b) all its prereqs are unlocked. A tech with met prereqs but no lab assigned is NOT shown as researching — it stays hidden until a lab begins work.

**Node states and styling:**

| State | Background | Border | Name color | Visibility rule |
|-------|-----------|--------|------------|-----------------|
| Unlocked | green tint rgba(76,175,125,0.07) | solid green 0.35 | green #4caf7d | Always shown |
| Researching | blue tint rgba(92,160,200,0.06) | solid blue 0.30 | default #e0e2e8 | Lab assigned + all prereqs unlocked |
| Locked | #13161e | dashed #2a2e38, opacity 0.5 | default (dimmed) | Direct child of a researching tech (one tier only) |
| Mystery | #0e1018 | dashed #1e2228 | "???" in #2a2e38 | Everything beyond locked tier |

**Progressive disclosure rules:**
1. Show all unlocked techs
2. Show researching techs (lab assigned + all prereqs unlocked)
3. Show one tier of locked children (direct dependents of researching techs)
4. Everything deeper renders as `???` mystery nodes
5. `???` nodes are same width/height as tech nodes for connector alignment
6. DAG edges continue into the `???` zone — if a hidden tech requires both a locked node and a `???` node, show converging edges into a `???`
7. The tree grows as techs unlock — newly unlocked techs reveal the next locked tier
8. If a locked child has multiple prereqs and only some are visible (researching), show it with edges only from visible parents

**Domain ↔ Color mapping** (same colors for both DataKind and ResearchDomain — 1:1 mapping):
- Survey / SurveyData: blue #5ca0c8
- Materials / AssayData: gold #c89a4a
- Manufacturing / ManufacturingData: green #4caf7d
- Propulsion / TransitData: purple #a78bfa

**Node content:**
- Tech name (human-readable from `TechDef.name`, never raw tech_id)
- Per-domain progress: domain label above a full-width bar (12px tall) with current/required numbers inside the bar, right-aligned
- Bar fill color matches domain color at 35% opacity
- No text badges, no probability text, no icons — state communicated entirely through tile styling

**Edge styling:**
- Active edges (connecting to unlocked/researching): rgba(92,160,200,0.45)
- Dim edges (connecting to locked): #2a2e38
- Fade edges (connecting to ???): #1e2228, stroke-dasharray 4 3
- `vector-effect: non-scaling-stroke` to prevent dash pattern distortion

### 3. Lab Status

Compact rows showing each lab's assignment, production rate, and status.

```
Survey Lab        Alloy Synthesis     +3.2/hr  active
Materials Lab     Alloy Synthesis     +2.8/hr  active
Manufacturing Lab Efficient Propulsion  0/hr   starved
Propulsion Lab    Efficient Propulsion +1.5/hr  active
```

- Lab name, assigned tech name, production rate, status badge
- Status badges: active (green), starved (red), idle (gray)
- Rate = research points produced per game hour

## Backend Requirements

### New API data needed (RD-10)

The snapshot currently serializes only `GameState`, NOT `GameContent`. The FE has no access to tech definitions. This must be fixed.

1. **Tech definitions** — New `GET /api/v1/content` endpoint (preferred) or embed in snapshot. FE needs: `TechDef.id`, `TechDef.name`, `TechDef.prereqs`, `TechDef.domain_requirements`, `TechDef.difficulty`, `TechDef.accepted_data`
2. **Data pool rates** — Backend provides net rate per data kind per game hour in the snapshot (`data_rates: HashMap<DataKind, f64>`). Backend-computed is simpler and more accurate than FE delta tracking.
3. **Lab production rates** — Backend provides research points per game hour per lab, derivable from `LabDef.points_per_run` and tick interval.

### Existing data that's sufficient

- `ResearchState.data_pool` — current data amounts
- `ResearchState.evidence` — per-tech domain progress
- `ResearchState.unlocked` — unlocked tech set
- `LabState.assigned_tech`, `LabState.starved` — lab assignments and status

## Dependencies

This FE work depends on the backend Research System Redesign tickets (RD-01 through RD-06) landing first, since they change:
- `DataKind` enum (3 → 4 variants)
- `ResearchDomain` enum (3 → 4 variants)
- Tech definitions in `techs.json`
- FE types in `types.ts` and event handlers in `applyEvents.ts`

The FE tickets should be sequenced after the backend tickets.

## Implementation Notes

- Use `dagre` (~15kb gzipped) for DAG layout. It computes x,y positions given nodes and edges.
- Nodes rendered as HTML divs (styled with existing Tailwind classes), positioned absolutely within a scrollable viewport.
- SVG overlay for edges only — simple line segments with right-angle bends, not bezier curves.
- The `TechTreeDAG` component should be a standalone component that takes `ResearchState` + `TechDef[]` and handles all layout/visibility logic.
- Progressive disclosure logic is pure: given unlocked set + lab assignments + prereq graph → compute visible set + node states.
- Rates provided by backend (see RD-10). FE just displays them.
- dagre config: `rankdir: 'TB'`, reasonable `nodesep`/`ranksep` for the panel width. Re-run layout only when tree structure changes (unlock events), not on every tick.
- Empty state (tick 0, no labs): show "no research activity" placeholder. Root techs with no prereqs only appear once a lab is assigned.
