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

**Node states and styling:**

| State | Background | Border | Name color | Visibility rule |
|-------|-----------|--------|------------|-----------------|
| Unlocked | green tint rgba(76,175,125,0.07) | solid green 0.35 | green #4caf7d | Always shown |
| Researching | blue tint rgba(92,160,200,0.06) | solid blue 0.30 | default #e0e2e8 | Shown if ALL prereqs are unlocked |
| Locked | #13161e | dashed #2a2e38, opacity 0.5 | default (dimmed) | Shown if it's a direct child of a researching tech (one tier only) |
| Mystery | #0e1018 | dashed #1e2228 | "???" in #2a2e38 | Everything beyond locked tier |

**Progressive disclosure rules:**
1. Show all unlocked techs
2. Show researching techs (only if all prereqs unlocked)
3. Show one tier of locked children (direct dependents of researching techs)
4. Everything deeper renders as `???` mystery nodes
5. `???` nodes are same width/height as tech nodes for connector alignment
6. DAG edges continue into the `???` zone — if a hidden tech requires both a locked node and a `???` node, show converging edges into a `???`
7. The tree grows as techs unlock — newly unlocked techs reveal the next locked tier

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

### New API data needed

The FE needs the following from the snapshot/SSE to render the tree:

1. **Tech definitions with names** — `TechDef` must include `name` field (already exists in `techs.json`)
2. **Prereq graph** — `TechDef.prereqs` already contains prerequisite tech IDs
3. **Data generation rates** — net rate per data kind (generation - consumption). Can be computed FE-side from sensor/lab tick rates, or provided by the daemon.
4. **Lab production rates** — points produced per tick by each lab. Can derive from `LabDef.points_per_run` and tick interval, or emit in events.

### Existing data that's sufficient

- `ResearchState.data_pool` — current data amounts
- `ResearchState.evidence` — per-tech domain progress
- `ResearchState.unlocked` — unlocked tech set
- `LabState.assigned_tech`, `LabState.starved` — lab assignments and status
- `TechDef.domain_requirements` — target values for progress bars
- `TechDef.prereqs` — DAG edges

### Content data flow

The snapshot already includes `content` with tech definitions. The FE reads the full tech list from content, builds the DAG from prereqs, and applies visibility rules based on `ResearchState`. No new endpoints needed for the tree structure itself.

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
- Progressive disclosure logic is pure: given unlocked set + prereq graph → compute visible set + node states.
- Rate computation: track delta between consecutive SSE snapshots, or derive from content constants.
