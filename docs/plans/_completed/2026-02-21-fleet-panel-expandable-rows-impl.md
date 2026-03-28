# Fleet Panel Expandable Rows Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the dense inline FleetPanel with summary rows that expand on click to show full station/ship details.

**Architecture:** Refactor `FleetPanel.tsx` — summary rows show only key metrics (ID, location, storage bar, task). Clicking a row expands an inline detail section. Extract `CapacityBar` sub-component for reuse. Existing `InventoryDisplay` and `ModulesDisplay` move into expanded sections.

**Tech Stack:** React 19, TypeScript 5, Tailwind v4

---

### Task 1: Extract CapacityBar component

A reusable capacity bar showing a filled progress bar with percentage and kg label. Used in both station and ship expanded details.

**Files:**
- Create: `ui_web/src/components/CapacityBar.tsx`
- Create: `ui_web/src/components/CapacityBar.test.tsx`

**Step 1: Write the test**

```tsx
// ui_web/src/components/CapacityBar.test.tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { CapacityBar } from './CapacityBar'

describe('CapacityBar', () => {
  it('renders percentage and kg values', () => {
    render(<CapacityBar usedKg={300} capacityKg={1000} />)
    expect(screen.getByText(/30%/)).toBeInTheDocument()
    expect(screen.getByText(/300/)).toBeInTheDocument()
  })

  it('renders progressbar with correct aria value', () => {
    render(<CapacityBar usedKg={750} capacityKg={1000} />)
    const bar = document.querySelector('[role="progressbar"]')
    expect(bar).toBeInTheDocument()
    expect(bar?.getAttribute('aria-valuenow')).toBe('75')
  })

  it('uses red color when above 90%', () => {
    const { container } = render(<CapacityBar usedKg={950} capacityKg={1000} />)
    const fill = container.querySelector('[data-testid="capacity-fill"]')
    expect(fill?.className).toMatch(/red/)
  })

  it('uses yellow color when above 70%', () => {
    const { container } = render(<CapacityBar usedKg={750} capacityKg={1000} />)
    const fill = container.querySelector('[data-testid="capacity-fill"]')
    expect(fill?.className).toMatch(/yellow/)
  })

  it('uses green color when below 70%', () => {
    const { container } = render(<CapacityBar usedKg={300} capacityKg={1000} />)
    const fill = container.querySelector('[data-testid="capacity-fill"]')
    expect(fill?.className).toMatch(/green/)
  })

  it('handles zero capacity without crashing', () => {
    render(<CapacityBar usedKg={0} capacityKg={0} />)
    expect(screen.getByText(/0%/)).toBeInTheDocument()
  })
})
```

**Step 2: Run tests to verify they fail**

Run: `cd ui_web && npm test -- --run CapacityBar`
Expected: FAIL — module not found

**Step 3: Write the implementation**

```tsx
// ui_web/src/components/CapacityBar.tsx
function formatKg(kg: number): string {
  if (kg >= 1_000_000) return `${(kg / 1_000_000).toFixed(1)}M`
  if (kg >= 1_000) return `${(kg / 1_000).toFixed(1)}k`
  return kg.toLocaleString(undefined, { maximumFractionDigits: 1 })
}

function capacityColor(pct: number): string {
  if (pct >= 90) return 'bg-red-400'
  if (pct >= 70) return 'bg-yellow-400'
  return 'bg-green-400'
}

interface Props {
  usedKg: number
  capacityKg: number
}

export function CapacityBar({ usedKg, capacityKg }: Props) {
  const pct = capacityKg > 0 ? Math.round((usedKg / capacityKg) * 100) : 0

  return (
    <div className="flex items-center gap-2 min-w-[120px]">
      <div
        role="progressbar"
        aria-valuenow={pct}
        aria-valuemin={0}
        aria-valuemax={100}
        className="flex-1 h-1.5 bg-edge rounded-full overflow-hidden"
      >
        <div
          data-testid="capacity-fill"
          className={`h-full rounded-full ${capacityColor(pct)}`}
          style={{ width: `${Math.min(pct, 100)}%` }}
        />
      </div>
      <span className="text-dim text-[10px] whitespace-nowrap">
        {pct}% — {formatKg(usedKg)} kg
      </span>
    </div>
  )
}
```

**Step 4: Run tests to verify they pass**

Run: `cd ui_web && npm test -- --run CapacityBar`
Expected: PASS (6 tests)

**Step 5: Commit**

```bash
git add ui_web/src/components/CapacityBar.tsx ui_web/src/components/CapacityBar.test.tsx
git commit -m "feat(ui): add CapacityBar component"
```

---

### Task 2: Add expand/collapse state and summary rows to StationsTable

Replace the current inline-everything station rows with compact summary rows. Add click-to-expand state. The expanded detail section comes in Task 3.

**Files:**
- Modify: `ui_web/src/components/FleetPanel.tsx` — `StationsTable` function
- Modify: `ui_web/src/components/FleetPanel.test.tsx`

**Step 1: Write the test**

Add to `FleetPanel.test.tsx`:

```tsx
it('station summary row shows storage bar instead of inventory breakdown', () => {
  const stations: Record<string, StationState> = {
    station_earth_orbit: {
      id: 'station_earth_orbit',
      location_node: 'node_earth_orbit',
      power_available_per_tick: 100,
      inventory: [{ kind: 'Material', element: 'Fe', kg: 500.0, quality: 0.85 }],
      cargo_capacity_m3: 100.0,
      facilities: { compute_units_total: 10, power_per_compute_unit_per_tick: 1, efficiency: 1.0 },
      modules: [
        {
          id: 'mod_001',
          def_id: 'module_basic_iron_refinery',
          enabled: true,
          kind_state: { Processor: { threshold_kg: 100, ticks_since_last_run: 0, stalled: false } },
          wear: { wear: 0.3 },
        },
      ],
    },
  }
  render(<FleetPanel ships={{}} stations={stations} displayTick={0} />)
  // Summary should NOT show full module def_id or inventory details
  expect(screen.queryByText(/module_basic_iron_refinery/)).not.toBeInTheDocument()
  // Should show a capacity bar
  expect(document.querySelector('[role="progressbar"]')).toBeInTheDocument()
})

it('clicking station row expands detail section', () => {
  render(<FleetPanel ships={{}} stations={mockStations} displayTick={0} />)
  const row = screen.getByText('station_earth_orbit').closest('tr')!
  fireEvent.click(row)
  // After expanding, inventory details should be visible
  expect(screen.getByText(/Fe/)).toBeInTheDocument()
})

it('clicking expanded station row collapses it', () => {
  render(<FleetPanel ships={{}} stations={mockStations} displayTick={0} />)
  const row = screen.getByText('station_earth_orbit').closest('tr')!
  fireEvent.click(row)
  fireEvent.click(row)
  // After collapsing, detailed inventory should be hidden
  // The capacity bar should still be visible (it's in the summary)
  expect(screen.queryByText(/excellent|good|poor/)).not.toBeInTheDocument()
})
```

**Step 2: Run tests to verify they fail**

Run: `cd ui_web && npm test -- --run FleetPanel`
Expected: FAIL — summary row still shows old inline inventory

**Step 3: Implement the changes**

In `FleetPanel.tsx`, modify `StationsTable`:

1. Add `expandedId` state: `const [expandedId, setExpandedId] = useState<string | null>(null)`
2. Replace table columns: remove Modules column, replace Cargo column header with "Storage"
3. Summary row: ID, Location, CapacityBar (using `totalInventoryKg`)
4. Add `onClick` handler to `<tr>` that toggles `expandedId`
5. Add `cursor-pointer hover:bg-surface` to summary row
6. After the summary `<tr>`, conditionally render an expanded `<tr>` with `colSpan={3}` containing a placeholder div (actual content in Task 3)

Note: `totalInventoryKg` returns kg-based total. For the capacity bar, we need kg not m³. The bar shows mass utilization which is the most intuitive metric for the user.

**Step 4: Run tests**

Run: `cd ui_web && npm test -- --run FleetPanel`
Expected: PASS

**Step 5: Commit**

```bash
git add ui_web/src/components/FleetPanel.tsx ui_web/src/components/FleetPanel.test.tsx
git commit -m "feat(ui): add expandable summary rows to station table"
```

---

### Task 3: Build expanded station detail section

The content that appears when a station row is expanded: two-column layout with inventory on the left and installed modules on the right.

**Files:**
- Modify: `ui_web/src/components/FleetPanel.tsx` — add `StationDetail` component, wire into expanded row

**Step 1: Write the test**

Add to `FleetPanel.test.tsx`:

```tsx
it('expanded station shows inventory grouped by type', () => {
  const stations: Record<string, StationState> = {
    station_earth_orbit: {
      id: 'station_earth_orbit',
      location_node: 'node_earth_orbit',
      power_available_per_tick: 100,
      inventory: [
        { kind: 'Ore', lot_id: 'lot_1', asteroid_id: 'a1', kg: 200.0, composition: { Fe: 0.7, Si: 0.3 } },
        { kind: 'Material', element: 'Fe', kg: 500.0, quality: 0.85 },
        { kind: 'Slag', kg: 100.0, composition: { Si: 1.0 } },
        { kind: 'Component', component_id: 'repair_kit', count: 3, quality: 1.0 },
      ],
      cargo_capacity_m3: 100.0,
      facilities: { compute_units_total: 10, power_per_compute_unit_per_tick: 1, efficiency: 1.0 },
      modules: [],
    },
  }
  render(<FleetPanel ships={{}} stations={stations} displayTick={0} />)
  fireEvent.click(screen.getByText('station_earth_orbit').closest('tr')!)
  // Should show grouped inventory
  expect(screen.getByText(/ore/i)).toBeInTheDocument()
  expect(screen.getByText(/slag/i)).toBeInTheDocument()
  expect(screen.getByText(/repair_kit/i)).toBeInTheDocument()
})

it('expanded station shows module cards with wear', () => {
  const stations: Record<string, StationState> = {
    station_earth_orbit: {
      id: 'station_earth_orbit',
      location_node: 'node_earth_orbit',
      power_available_per_tick: 100,
      inventory: [],
      cargo_capacity_m3: 100.0,
      facilities: { compute_units_total: 10, power_per_compute_unit_per_tick: 1, efficiency: 1.0 },
      modules: [
        {
          id: 'mod_001',
          def_id: 'module_basic_iron_refinery',
          enabled: true,
          kind_state: { Processor: { threshold_kg: 100, ticks_since_last_run: 5, stalled: true } },
          wear: { wear: 0.65 },
        },
      ],
    },
  }
  render(<FleetPanel ships={{}} stations={stations} displayTick={0} />)
  fireEvent.click(screen.getByText('station_earth_orbit').closest('tr')!)
  // Module details should be visible
  expect(screen.getByText(/module_basic_iron_refinery/)).toBeInTheDocument()
  expect(screen.getByText(/stalled/i)).toBeInTheDocument()
})
```

**Step 2: Run tests to verify they fail**

Run: `cd ui_web && npm test -- --run FleetPanel`
Expected: FAIL — expanded section has placeholder, not real content

**Step 3: Implement StationDetail**

Create a `StationDetail` component inside `FleetPanel.tsx` that receives a `StationState` and renders:

**Layout:** `grid grid-cols-2 gap-4` (stacks to 1-col on narrow via `@container` or just use `flex flex-wrap`)

**Left column — Inventory:**
- `CapacityBar` at top (import from `./CapacityBar`)
- Section "Ore" — `aggregateOre()` result: total kg, lot count, composition percentages
- Section "Materials" — list each with element, kg, quality tier badge
- Section "Slag" — total kg
- Section "Components" — name × count
- Section "Modules (stored)" — def_id for any `ModuleItem`

**Right column — Installed Modules:**
- Each module rendered as a card-like block:
  - Header: `def_id` (strip `module_` prefix for readability) + enabled/disabled badge
  - Wear bar: reuse the capacity bar pattern but with wear colors (green < 50%, yellow 50-80%, red > 80%). Show `{100 - wear*100}% health`
  - If Processor: show threshold, stalled badge (red "STALLED" text if `stalled === true`)
  - If Maintenance: show interval

Replace the placeholder in the expanded row `<tr>` with `<StationDetail station={station} />`.

**Step 4: Run tests**

Run: `cd ui_web && npm test -- --run FleetPanel`
Expected: PASS

**Step 5: Commit**

```bash
git add ui_web/src/components/FleetPanel.tsx ui_web/src/components/FleetPanel.test.tsx
git commit -m "feat(ui): add station detail section with inventory and module cards"
```

---

### Task 4: Add expand/collapse to ShipsTable with summary rows and detail

Same pattern as stations: compact summary row, click to expand with cargo detail and task info.

**Files:**
- Modify: `ui_web/src/components/FleetPanel.tsx` — `ShipsTable` function
- Modify: `ui_web/src/components/FleetPanel.test.tsx`

**Step 1: Write the test**

Add to `FleetPanel.test.tsx`:

```tsx
it('ship summary row shows total cargo only', () => {
  render(<FleetPanel ships={mockShips} stations={{}} displayTick={0} />)
  // Should show total kg but NOT the composition breakdown in the summary
  expect(screen.getByText(/180/)).toBeInTheDocument()
  // Composition detail should NOT be visible until expanded
  expect(screen.queryByText(/Fe 70%/)).not.toBeInTheDocument()
})

it('clicking ship row expands inventory detail', () => {
  render(<FleetPanel ships={mockShips} stations={{}} displayTick={0} />)
  const row = screen.getByText('ship_0001').closest('tr')!
  fireEvent.click(row)
  // Expanded should show inventory breakdown
  expect(screen.getByText(/ore/i)).toBeInTheDocument()
})
```

**Step 2: Run tests to verify they fail**

Run: `cd ui_web && npm test -- --run FleetPanel`

**Step 3: Implement the changes**

In `ShipsTable`:

1. Add `expandedId` state
2. Remove `InventoryDisplay` from summary row — just show `{formatKg(cargo_kg)} kg` or "empty"
3. Add click handler on `<tr>` to toggle expand
4. Add expanded `<tr>` with `colSpan={5}` containing `ShipDetail`:
   - `CapacityBar` (cargo used vs capacity — note: capacity is in m³ but we show kg, so just show the kg total without a max for now, or skip the bar and just show inventory)
   - `InventoryDisplay` (the existing component, reused as-is)
   - Task detail section: show task-specific info (transit destination, mining asteroid, deposit station + blocked status)

**Step 4: Run tests**

Run: `cd ui_web && npm test -- --run FleetPanel`
Expected: PASS

**Step 5: Commit**

```bash
git add ui_web/src/components/FleetPanel.tsx ui_web/src/components/FleetPanel.test.tsx
git commit -m "feat(ui): add expandable ship rows with inventory and task detail"
```

---

### Task 5: Polish and fix existing tests

The existing tests may need adjustment since the rendering changed (e.g., cargo is no longer in summary for ships with `InventoryDisplay`, station cargo column changed to Storage). Go through each existing test and fix any that broke.

**Files:**
- Modify: `ui_web/src/components/FleetPanel.test.tsx`

**Step 1: Run all FleetPanel tests**

Run: `cd ui_web && npm test -- --run FleetPanel`

**Step 2: Fix any failures**

Common fixes:
- Tests looking for inline inventory text need to first click to expand
- Tests checking column headers need to match new names ("Storage" instead of "Cargo" for stations)
- Sort tests should still work since sortable data keys haven't changed

**Step 3: Run tests again**

Run: `cd ui_web && npm test -- --run FleetPanel`
Expected: ALL PASS

**Step 4: Run full test suite**

Run: `cd ui_web && npm test -- --run`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add ui_web/src/components/FleetPanel.test.tsx
git commit -m "test(ui): update FleetPanel tests for expandable row changes"
```
