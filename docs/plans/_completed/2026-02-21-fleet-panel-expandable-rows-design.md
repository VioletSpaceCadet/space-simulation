# Fleet Panel: Expandable Row Details

## Problem

The Fleet panel crams full inventory breakdowns and module details into table cells. Station rows overflow, module columns get cut off, and the table is unscannable. There's no way to drill into station or ship details without reading a wall of inline text.

## Design

### Summary Rows (collapsed — default)

**Ships:**

| Column | Content |
|--------|---------|
| ID | ship_0001 |
| Location | node_earth_orbit |
| Task | deposit |
| Progress | Visual bar + percentage |
| Cargo | Total kg only (e.g., "23,994 kg") |

**Stations:**

| Column | Content |
|--------|---------|
| ID | station_earth_orbit |
| Location | node_earth_orbit |
| Storage | Capacity bar + percentage + total kg (e.g., "82% — 329k kg") |

No module count or inventory breakdown in the summary. Rows are clickable (cursor pointer, hover highlight).

### Expanded Station Detail

Click a station row to expand an inline detail section below it. Two-column layout (stack on narrow viewports):

**Left — Inventory:**
- Capacity bar at top: `328,976 / 400,000 kg (82%)`, color-coded green → yellow → red
- Grouped by type:
  - Ore: total kg, lot count, top composition elements
  - Materials: element, kg, quality tier
  - Slag: total kg
  - Components: name + count
  - Modules (in storage): def_id

**Right — Installed Modules:**
- Each module as a small card:
  - Name + enabled/disabled badge
  - Wear bar (green/yellow/red bands) with percentage
  - Stall indicator if stalled
  - Type-specific:
    - Processor: threshold, interval, stall status
    - Maintenance: interval

### Expanded Ship Detail

Click a ship row to expand:
- Cargo capacity bar (same style as station)
- Inventory breakdown (ore lots with composition, materials, etc.)
- Task detail: transit origin→dest with ETA, mining asteroid + duration, deposit station + blocked status

### Interaction

- Click to expand, click again to collapse (toggle)
- Only one row expanded at a time (clicking another collapses the current)
- Subtle height transition animation
- Expanded row gets a slightly lighter background to separate from summary rows

## Non-Goals

- No drag-and-drop or movable panels
- No floating DetailCard reuse (expandable row is the pattern for data tables)
- No tooltip system changes — the map tooltips stay as-is

## Files Affected

- `ui_web/src/components/FleetPanel.tsx` — main changes (summary rows, expand logic, detail sections)
- `ui_web/src/components/FleetPanel.test.tsx` — update tests for new structure
- Possibly extract `CapacityBar` or `ModuleCard` sub-components if FleetPanel gets too large
