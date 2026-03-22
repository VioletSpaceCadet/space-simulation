# Manufacturing DAG Panel — Design Document

## Overview

Production chain visualization panel showing manufacturing flow as a directed acyclic graph. Renders recipe topology, real-time throughput, inventory trends, and module utilization within the existing draggable panel system.

**Mockup:** `docs/mockups/manufacturing-dag-mockup.html` (open in browser — guidance only, not pixel-exact)
**Linear project:** [Manufacturing DAG System](https://linear.app/violetspacecadet/project/manufacturing-dag-system-8796e526acf5) (VIO-369 through VIO-374)
**Requirements doc:** `docs/brainstorms/manufacturing-dag-requirements.md`

> **Note:** The mockup is a design exploration tool, not a pixel-perfect spec. The real implementation will use dagre for layout (solving arrow routing) and DOM for tooltips (solving text clarity). The mockup captures the design language, data model, and interaction patterns.

---

## What the Panel Must Answer

The panel must answer three questions at a glance:

1. **"What connects to what?"** — Recipe topology (static, from content)
2. **"How fast is stuff moving?"** — Actual throughput per edge/node (live, from events)
3. **"Where are the bottlenecks?"** — Stall reasons, utilization gaps, depletion trends (computed)

Phase 1 (VIO-373) delivers #1. This design covers all three.

---

## Data Architecture

### Flow Data Source: FE Event Accumulation

The FE already receives granular production events via SSE (`RefineryRan`, `AssemblerRan`). These contain everything needed:

| Event | Relevant Fields |
|-------|----------------|
| `RefineryRan` | `station_id`, `module_id`, `ore_consumed_kg`, `material_produced_kg`, `material_element`, `slag_produced_kg` |
| `AssemblerRan` | `station_id`, `module_id`, `recipe_id`, `material_consumed_kg`, `component_produced_id`, `component_produced_count` |

**No new BE endpoints needed.** FE accumulates events into a per-module rolling window:

```typescript
interface ModuleFlowStats {
  module_id: string
  recipe_id: string
  // Rolling window (last N ticks, configurable — default 100)
  runs_in_window: number
  total_input_kg: number
  total_output_kg: number    // or count for components
  total_output_count: number
  last_run_tick: number
  // Derived
  throughput_per_hour: number  // runs_in_window / window_hours
  utilization_pct: number      // actual_runs / max_possible_runs
  stall_reason: string | null  // last known: 'starved' | 'no_power' | 'thermal' | 'stock_cap' | 'locked' | null
}
```

### Net Flow on Items

Computed from inventory deltas between consecutive snapshots:

```typescript
interface ItemFlowStats {
  item_id: string
  current_qty: number
  delta_per_hour: number   // positive = accumulating, negative = depleting
  trend: 'rising' | 'falling' | 'stable'
  ticks_at_zero: number   // how long this has been empty (bottleneck signal)
}
```

### What Comes from BE (Already Exists)

- **Bottleneck detection:** `GET /api/v1/advisor/digest` → `bottleneck` enum (OreSupply, StorageFull, SlagBackpressure, etc.)
- **Alert state:** Active alerts (`ORE_STARVATION`, `REFINERY_STALLED`, etc.)
- **Aggregate rates:** `rates.material_production`, `rates.ore_consumption` from digest

These provide system-level context. Per-module detail comes from FE accumulation.

---

## Visual Design

### Module Summary Bar (toolbar area)

Shows station-wide module allocation at a glance:

```
Processors: 4 total · 3 active · 1 idle    Assemblers: 3 total · 2 active · 1 unassigned
```

- Grouped by module behavior type (Processor, Assembler)
- Counts: total installed, actively running a recipe, idle/unassigned
- Clicking a module type filters the DAG to show only recipes using that type
- Provides the "how much manufacturing capacity do I have?" answer without opening station details

### Node Types

**Item Nodes (Circles)**
- Size: fixed 14px radius
- Color: type-coded (raw amber `#c47038`, refined gold `#c89a4a`, component blue `#5ca0c8`, final green `#4caf7d`)
- Interior: 2-char abbreviation (FE, ST, HU, etc.)
- Below: name + inventory count
- **Flow indicator:** Small arrow (▲ rising, ▼ falling) next to count, colored green/red
- **Capacity ring:** If item has a storage cap, thin arc around node showing fill level
- **Bottleneck highlight:** Red pulsing border if `ticks_at_zero > threshold`
- **No rigid tier columns** — position emerges from recipe graph topology via dagre

**Recipe Nodes (Rounded Rectangles)**
- Size: ~100×22px (wide enough for full recipe names — no truncation)
- Background: `#161a24` (raised surface)
- Left edge: status dot (green pulsing = active, blue = available, red = starved)
- Interior: recipe name
- **Utilization bar:** Thin bar along bottom edge, width = utilization %, colored by status
- **Locked recipes are NOT shown** — they appear when unlocked via research

### Edges

- **Routed by dagre** — no manual bezier positioning. dagre handles edge routing, fan-in/fan-out, and crossing minimization.
- **Thickness:** Proportional to throughput rate (1px idle, 2-3px active flow)
- **Color:** Status-driven (green active, blue available)
- **No edge labels** — amounts shown in recipe tooltip. Edge labels clutter the graph.
- **Arrowheads:** Small filled triangles at destination end

### Tooltip Content

Tooltips are DOM elements (not canvas text) for crisp rendering.

**Item tooltip:**
```
┌──────────────────────────────┐
│ Iron                         │
│ REFINED                      │
│                              │
│ In Stock      1,200 kg       │
│ Net Flow      +120 kg/hr ▲   │
│ Capacity      —              │
│──────────────────────────────│
│ Consumed by                  │
│  · Plate Pressing    (active)│
│  · Repair Kit        (active)│
│  · Circuit Assembly  (idle)  │
│ Produced by                  │
│  · Iron Smelting     (active)│
└──────────────────────────────┘
```

**Recipe tooltip — clear bill of materials format:**
```
┌──────────────────────────────┐
│ Beam Fabrication             │
│ RECIPE · active              │
│──────────────────────────────│
│ INPUTS (per cycle)           │
│  3× Fe Plate        (×8)    │
│──────────────────────────────│
│ OUTPUTS (per cycle)          │
│  1× Structural Beam (×3)    │
│──────────────────────────────│
│ Module   Structural Assembler│
│ Type     Assembler           │
│ Assigned 1 of 1 available    │
│──────────────────────────────│
│ Rate     0.3/hr (max 0.33)  │
│ Util     ████████░░ 91%      │
│ Stall    Fe plates intermit. │
└──────────────────────────────┘
```

Key tooltip design rules:
- **Input/output amounts use item names** not abstract "units" — "3× Fe Plate" not "3 plates"
- **Current stock shown in parentheses** next to each input — "(×8)" tells you whether there's enough without hovering elsewhere
- **Rate units are consistent** — always "X.X/hr" for count-based items, "X kg/hr" for mass-based
- **Utilization bar is visual** — colored bar + percentage
- **Stall reason only shown when < 100%** — explains why the module isn't at full throughput

### Layout

**Left-to-right flow**, topology-driven:
- Positions determined by dagre topological sort — NO fixed tier columns
- Items placed at their natural depth in the recipe graph
- dagre handles vertical spacing and edge crossing minimization
- **No tier labels** — the color coding (raw/refined/component/final in the legend) provides category context without imposing rigid columns

### Panel Chrome

- **Header:** "PRODUCTION CHAIN" with fit-to-view and close buttons
- **Module Summary Bar:** `Processors: 4 (3 active · 1 idle)  |  Assemblers: 3 (2 active · 1 unassigned)`
- **Toolbar:**
  - Chain / Station view toggle (Chain = full recipe graph, Station = only recipes active on selected station)
  - Station selector dropdown
  - All / Active / Locked filter
- **Legend:** Bottom-left overlay with tier colors and status indicators
- **Zoom/Pan:** Same canvas camera pattern as solar system map (scroll to zoom, drag to pan)

---

## Interaction Model

### Hover
- Item nodes: show item tooltip with flow stats
- Recipe nodes: show recipe tooltip with utilization
- Edges: highlight the full chain path (upstream + downstream) for that edge

### Click
- Item node: select → highlight all edges in/out, dim others
- Recipe node: select → show detail card (right side) with full recipe info, module assignment, throughput history sparkline

### Chain Highlighting
- Clicking any node highlights its full upstream/downstream dependency chain
- All other nodes and edges dim to 20% opacity
- Click empty space to clear selection

### Station View
- When "Station" tab is active, only show recipes that the selected station can run
- Overlay station-specific data: which module is assigned to each recipe, that module's utilization
- Recipes the station doesn't have modules for are shown as empty dashed outlines

---

## Implementation Plan

### Phase 1: Static Topology (VIO-373 scope)

**Data layer (VIO-372):**
- Recipe graph utility (`utils/recipeGraph.ts`): build DAG from `recipes.json` content
- Types: `RecipeNode { id, inputs, outputs, status }`, `ItemNode { id, type, inventory }`, `Edge { from, to, amount }`
- Fetch recipe catalog from `GET /api/v1/content`
- Map station module state to recipe status (active/available/locked)

**Panel component:**
- Canvas-based rendering (consistent with solar system map approach)
- dagre for layout computation
- DOM overlay for tooltips and toolbar
- Hover/click interaction with hit detection

### Phase 2: Live Flow Data

**FE accumulation hook** (`useModuleFlowStats`):
- Subscribe to SSE event stream
- Accumulate `RefineryRan` / `AssemblerRan` events per module into rolling window
- Compute throughput rates, utilization %
- Expose `Map<module_id, ModuleFlowStats>`

**Item flow hook** (`useItemFlowStats`):
- Track inventory deltas between snapshots
- Compute net flow per item
- Expose `Map<item_id, ItemFlowStats>`

**Panel updates:**
- Edge thickness scales with throughput
- Item nodes show flow direction arrows
- Recipe nodes show utilization bar
- Animated edge flow for active recipes

### Phase 3: Bottleneck Intelligence

- Integrate advisor digest alerts into DAG (highlight bottleneck nodes)
- Per-module stall reason from events (`ProcessorStalled` reason field)
- "Bottleneck path" highlighting — trace from final product back to the starved input
- Storage pressure indicators (items approaching capacity)

---

## Existing Infrastructure Leveraged

| What | Where | How Used |
|------|-------|----------|
| Recipe definitions | `content/recipes.json` (after MD-01) | DAG topology |
| Module state | `SimSnapshot.stations[].modules[]` | Recipe status (active/available) |
| Tech unlocks | `SimSnapshot.research.unlocked[]` | Recipe locked/unlocked |
| Production events | SSE stream (`RefineryRan`, `AssemblerRan`) | FE throughput accumulation |
| Inventory levels | `SimSnapshot.stations[].inventory[]` | Item node quantities |
| Bottleneck detection | `GET /api/v1/advisor/digest` | System-level bottleneck overlay |
| Alert state | `GET /api/v1/alerts` | Starvation/stall indicators |
| dagre layout | Already in `package.json` (used by TechTreeDAG) | DAG auto-layout |
| Theme colors | `config/theme.ts` | All colors from centralized config |

---

## Files to Create/Modify

| File | Change |
|------|--------|
| `ui_web/src/components/ManufacturingDAG.tsx` | New — main panel component |
| `ui_web/src/components/manufacturing/RecipeTooltip.tsx` | New — recipe tooltip |
| `ui_web/src/components/manufacturing/ItemTooltip.tsx` | New — item tooltip |
| `ui_web/src/utils/recipeGraph.ts` | New — DAG construction from content |
| `ui_web/src/hooks/useModuleFlowStats.ts` | New (Phase 2) — SSE event accumulation |
| `ui_web/src/hooks/useItemFlowStats.ts` | New (Phase 2) — inventory delta tracking |
| `ui_web/src/config/theme.ts` | Add TIER_COLORS, RECIPE_STATUS_COLORS |
| `content/recipes.json` | Created by MD-01 — recipe catalog |

---

## Design Decisions

### Why Canvas + DOM (not pure React/SVG)?
- Consistent with solar system map approach
- Canvas handles edge rendering (beziers, thickness, animation) efficiently
- DOM overlay for tooltips gives crisp text without canvas font issues
- dagre computes layout, canvas renders it — clean separation

### Why FE accumulation (not new BE endpoint)?
- Events already contain all needed data
- No BE changes required — purely additive FE work
- Real-time updates without polling
- Per-module granularity that the current digest doesn't provide
- Rolling window is configurable client-side

### Why dagre for layout?
- Already a dependency (TechTreeDAG uses it)
- Handles fan-in/fan-out gracefully
- Topological sorting built in
- Rank-based layout produces the left-to-right tier structure we want

### Why not merge with TechTreeDAG?
- Different data model (recipes vs techs)
- Different interaction needs (flow monitoring vs research planning)
- Different visual language (industrial process diagram vs skill tree)
- Shared dagre dependency is enough code reuse

---

## Known Limitations

1. **No inter-station flow** — Phase 1 is single-station. Multi-station supply chains need a different visualization (geographic + flow).
2. **Rolling window size** — FE accumulation window needs tuning. Too short = noisy, too long = laggy. Default 100 ticks (100 hours game time at 60 min/tick).
3. **dagre layout stability** — Adding/removing recipes could cause layout jumps. May need to pin positions or use animated transitions.
4. **Alternative recipes** — Multiple recipes producing the same output need visual treatment (tabbed or stacked recipe nodes).
5. **Scale** — With 20+ intermediates the DAG gets dense. May need collapsible sub-chains or a zoom/filter system.
