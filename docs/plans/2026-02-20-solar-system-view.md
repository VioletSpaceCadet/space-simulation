# Solar System View Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a toggleable SVG orbital map of the solar system to the React UI, showing stations, ships, asteroids, and scan sites with d3-zoom pan/zoom, hover tooltips, click-to-select, and transit animation.

**Architecture:** New `SolarSystemMap` component renders an SVG with concentric orbital rings. Entities are placed at angular positions along their node's ring. d3-zoom handles pan/zoom on a root `<g>`. Transit ships interpolate position between origin and destination rings. A view toggle in `StatusBar` switches between the existing dashboard panels and the full-bleed map.

**Tech Stack:** React 19, SVG, d3-zoom + d3-selection, Tailwind CSS v4, Vitest + React Testing Library

**Design doc:** `docs/plans/2026-02-20-solar-system-view-design.md`

**Working directory:** `ui_web/` (all paths below are relative to the repo root)

---

### Task 1: Install d3 dependencies

**Files:**
- Modify: `ui_web/package.json`

**Step 1: Install d3-zoom and d3-selection with types**

Run:
```bash
cd ui_web && npm install d3-zoom d3-selection && npm install -D @types/d3-zoom @types/d3-selection
```

**Step 2: Verify install succeeded**

Run: `cd ui_web && npm ls d3-zoom d3-selection`
Expected: Both packages listed with versions, no errors

**Step 3: Verify existing tests still pass**

Run: `cd ui_web && npm test`
Expected: All tests pass

**Step 4: Commit**

```bash
git add ui_web/package.json ui_web/package-lock.json
git commit -m "feat(ui): add d3-zoom and d3-selection dependencies"
```

---

### Task 2: View toggle in StatusBar and App

**Files:**
- Modify: `ui_web/src/App.tsx`
- Modify: `ui_web/src/components/StatusBar.tsx`
- Modify: `ui_web/src/App.test.tsx`

**Step 1: Update StatusBar to accept and render a view toggle**

Add `view` and `onToggleView` props. Render a button on the left side of the status bar.

`ui_web/src/components/StatusBar.tsx`:
```tsx
interface Props {
  tick: number
  connected: boolean
  view: 'dashboard' | 'map'
  onToggleView: () => void
}

export function StatusBar({ tick, connected, view, onToggleView }: Props) {
  const day = Math.floor(tick / 1440)
  const hour = Math.floor((tick % 1440) / 60)
  const minute = tick % 60

  return (
    <div className="flex gap-6 items-center px-4 py-1.5 bg-surface border-b border-edge text-xs shrink-0">
      <button
        onClick={onToggleView}
        className="text-accent hover:text-bright transition-colors cursor-pointer"
      >
        {view === 'dashboard' ? '◈ System Map' : '☰ Dashboard'}
      </button>
      <span className="text-bright font-bold">tick {tick}</span>
      <span className="text-dim">
        day {day} | {String(hour).padStart(2, '0')}:{String(minute).padStart(2, '0')}
      </span>
      <span className={connected ? 'text-online' : 'text-offline'}>
        {connected ? '● connected' : '○ reconnecting...'}
      </span>
    </div>
  )
}
```

**Step 2: Add view state to App.tsx and wire it up**

Add `useState` for view, pass to StatusBar, conditionally render dashboard vs a placeholder for the map.

`ui_web/src/App.tsx` — add to imports:
```tsx
import { useState } from 'react'
```

Add inside `App()` before `return`:
```tsx
const [view, setView] = useState<'dashboard' | 'map'>('dashboard')
const toggleView = () => setView(v => v === 'dashboard' ? 'map' : 'dashboard')
```

Update the return to:
```tsx
return (
  <div className="flex flex-col h-screen overflow-hidden">
    <StatusBar tick={currentTick} connected={connected} view={view} onToggleView={toggleView} />
    {view === 'dashboard' ? (
      <PanelGroup direction="horizontal" className="flex-1 overflow-hidden">
        {/* ... existing panels unchanged ... */}
      </PanelGroup>
    ) : (
      <div className="flex-1 overflow-hidden bg-void flex items-center justify-center text-dim">
        Solar System Map (coming soon)
      </div>
    )}
  </div>
)
```

**Step 3: Update StatusBar tests**

`ui_web/src/components/StatusBar.test.tsx` — update all `render(<StatusBar .../>)` calls to pass the new required props: `view="dashboard"` and `onToggleView={vi.fn()}`. Add a test for the toggle button:

```tsx
it('renders toggle button showing System Map when in dashboard view', () => {
  render(<StatusBar tick={0} connected={true} view="dashboard" onToggleView={vi.fn()} />)
  expect(screen.getByText(/System Map/)).toBeInTheDocument()
})

it('renders toggle button showing Dashboard when in map view', () => {
  render(<StatusBar tick={0} connected={true} view="map" onToggleView={vi.fn()} />)
  expect(screen.getByText(/Dashboard/)).toBeInTheDocument()
})

it('calls onToggleView when button clicked', async () => {
  const toggle = vi.fn()
  render(<StatusBar tick={0} connected={true} view="dashboard" onToggleView={toggle} />)
  await userEvent.click(screen.getByText(/System Map/))
  expect(toggle).toHaveBeenCalledOnce()
})
```

**Step 4: Update App tests**

`ui_web/src/App.test.tsx` — the existing "renders all four panel headings" test still works (dashboard is default). Add a test for toggling:

```tsx
it('shows map placeholder when toggled to map view', async () => {
  render(<App />)
  await userEvent.click(screen.getByText(/System Map/))
  expect(screen.getByText(/Solar System Map/)).toBeInTheDocument()
  // Dashboard panels should not be rendered
  expect(screen.queryByText('Events')).not.toBeInTheDocument()
})
```

Import `userEvent` from `@testing-library/user-event` in both test files.

**Step 5: Run tests**

Run: `cd ui_web && npm test`
Expected: All tests pass

**Step 6: Commit**

```bash
git add ui_web/src/App.tsx ui_web/src/App.test.tsx ui_web/src/components/StatusBar.tsx ui_web/src/components/StatusBar.test.tsx
git commit -m "feat(ui): add view toggle between dashboard and system map"
```

---

### Task 3: d3-zoom hook

**Files:**
- Create: `ui_web/src/hooks/useSvgZoomPan.ts`
- Create: `ui_web/src/hooks/useSvgZoomPan.test.ts`

**Step 1: Write the hook**

The hook takes an SVG ref and a group ref. It attaches d3-zoom behavior to the SVG and applies transforms to the group.

`ui_web/src/hooks/useSvgZoomPan.ts`:
```tsx
import { useEffect } from 'react'
import { select } from 'd3-selection'
import { zoom, zoomIdentity } from 'd3-zoom'
import type { D3ZoomEvent } from 'd3-zoom'

interface ZoomPanOptions {
  minZoom?: number
  maxZoom?: number
}

export function useSvgZoomPan(
  svgRef: React.RefObject<SVGSVGElement | null>,
  groupRef: React.RefObject<SVGGElement | null>,
  options: ZoomPanOptions = {},
) {
  const { minZoom = 0.3, maxZoom = 5 } = options

  useEffect(() => {
    const svgEl = svgRef.current
    const groupEl = groupRef.current
    if (!svgEl || !groupEl) return

    const svgSelection = select<SVGSVGElement, unknown>(svgEl)
    const zoomBehavior = zoom<SVGSVGElement, unknown>()
      .scaleExtent([minZoom, maxZoom])
      .on('zoom', (event: D3ZoomEvent<SVGSVGElement, unknown>) => {
        select(groupEl).attr('transform', event.transform.toString())
      })

    svgSelection.call(zoomBehavior)

    // Set initial transform to center the map
    svgSelection.call(zoomBehavior.transform, zoomIdentity)

    return () => {
      svgSelection.on('.zoom', null)
    }
  }, [svgRef, groupRef, minZoom, maxZoom])
}
```

**Step 2: Write a basic smoke test**

Since d3-zoom needs a real DOM with SVG measurements, the test verifies the hook doesn't crash and attaches zoom behavior.

`ui_web/src/hooks/useSvgZoomPan.test.ts`:
```tsx
import { renderHook } from '@testing-library/react'
import { useRef } from 'react'
import { describe, expect, it } from 'vitest'

// Just verify the module exports correctly and doesn't crash
describe('useSvgZoomPan', () => {
  it('exports without error', async () => {
    const mod = await import('./useSvgZoomPan')
    expect(mod.useSvgZoomPan).toBeDefined()
    expect(typeof mod.useSvgZoomPan).toBe('function')
  })
})
```

**Step 3: Run tests**

Run: `cd ui_web && npm test`
Expected: All tests pass

**Step 4: Commit**

```bash
git add ui_web/src/hooks/useSvgZoomPan.ts ui_web/src/hooks/useSvgZoomPan.test.ts
git commit -m "feat(ui): add d3-zoom pan/zoom hook for SVG"
```

---

### Task 4: SolarSystemMap component — orbital rings and labels

**Files:**
- Create: `ui_web/src/components/SolarSystemMap.tsx`
- Create: `ui_web/src/components/SolarSystemMap.test.tsx`
- Modify: `ui_web/src/App.tsx` (swap placeholder for real component)

This task builds the base map with rings, labels, and zoom — no entities yet.

**Step 1: Write the test**

`ui_web/src/components/SolarSystemMap.test.tsx`:
```tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { SolarSystemMap } from './SolarSystemMap'
import type { SimSnapshot } from '../types'

const emptySnapshot: SimSnapshot = {
  meta: { tick: 100, seed: 42, content_version: '0.0.1' },
  scan_sites: [],
  asteroids: {},
  ships: {},
  stations: {},
  research: { unlocked: [], data_pool: {}, evidence: {} },
}

describe('SolarSystemMap', () => {
  it('renders an SVG element', () => {
    const { container } = render(
      <SolarSystemMap snapshot={emptySnapshot} currentTick={100} oreCompositions={{}} />
    )
    expect(container.querySelector('svg')).toBeInTheDocument()
  })

  it('renders orbital ring labels', () => {
    render(
      <SolarSystemMap snapshot={emptySnapshot} currentTick={100} oreCompositions={{}} />
    )
    expect(screen.getByText('Earth Orbit')).toBeInTheDocument()
    expect(screen.getByText('Inner Belt')).toBeInTheDocument()
    expect(screen.getByText('Mid Belt')).toBeInTheDocument()
    expect(screen.getByText('Outer Belt')).toBeInTheDocument()
  })
})
```

**Step 2: Run tests to verify they fail**

Run: `cd ui_web && npm test`
Expected: FAIL — module not found

**Step 3: Implement the base SolarSystemMap**

`ui_web/src/components/SolarSystemMap.tsx`:
```tsx
import { useRef } from 'react'
import { useSvgZoomPan } from '../hooks/useSvgZoomPan'
import type { OreCompositions } from '../hooks/useSimStream'
import type { SimSnapshot } from '../types'

interface Props {
  snapshot: SimSnapshot | null
  currentTick: number
  oreCompositions: OreCompositions
}

// Node ID → ring configuration
const RINGS: { nodeId: string; label: string; radius: number; isBelt: boolean }[] = [
  { nodeId: 'node_earth_orbit', label: 'Earth Orbit', radius: 100, isBelt: false },
  { nodeId: 'node_belt_inner', label: 'Inner Belt', radius: 200, isBelt: true },
  { nodeId: 'node_belt_mid', label: 'Mid Belt', radius: 300, isBelt: true },
  { nodeId: 'node_belt_outer', label: 'Outer Belt', radius: 400, isBelt: true },
]

export function SolarSystemMap({ snapshot, currentTick, oreCompositions }: Props) {
  const svgRef = useRef<SVGSVGElement>(null)
  const groupRef = useRef<SVGGElement>(null)

  useSvgZoomPan(svgRef, groupRef)

  return (
    <div className="relative w-full h-full bg-void overflow-hidden">
      <svg
        ref={svgRef}
        className="w-full h-full"
        viewBox="-500 -500 1000 1000"
        preserveAspectRatio="xMidYMid meet"
      >
        <g ref={groupRef}>
          {/* Sun at center */}
          <circle cx={0} cy={0} r={12} fill="#f5c842" opacity={0.9} />
          <circle cx={0} cy={0} r={18} fill="none" stroke="#f5c842" opacity={0.2} strokeWidth={4} />

          {/* Orbital rings */}
          {RINGS.map((ring) => (
            <g key={ring.nodeId}>
              <circle
                cx={0}
                cy={0}
                r={ring.radius}
                fill="none"
                stroke="var(--color-edge)"
                strokeWidth={ring.isBelt ? 0.5 : 0.8}
                strokeDasharray={ring.isBelt ? '4 4' : undefined}
                opacity={0.6}
              />
              {/* Label at top of ring */}
              <text
                x={0}
                y={-ring.radius - 8}
                textAnchor="middle"
                fill="var(--color-label)"
                fontSize={10}
                fontFamily="monospace"
              >
                {ring.label}
              </text>
            </g>
          ))}
        </g>
      </svg>
    </div>
  )
}
```

Note: `snapshot`, `currentTick`, and `oreCompositions` are accepted but not used yet — they'll be used when we add entities in the next tasks.

**Step 4: Wire into App.tsx**

Replace the placeholder in `App.tsx`:

Import: `import { SolarSystemMap } from './components/SolarSystemMap'`

Replace the placeholder div:
```tsx
) : (
  <SolarSystemMap snapshot={snapshot} currentTick={currentTick} oreCompositions={oreCompositions} />
)}
```

**Step 5: Run tests**

Run: `cd ui_web && npm test`
Expected: All tests pass

**Step 6: Commit**

```bash
git add ui_web/src/components/SolarSystemMap.tsx ui_web/src/components/SolarSystemMap.test.tsx ui_web/src/App.tsx
git commit -m "feat(ui): add SolarSystemMap with orbital rings and d3-zoom"
```

---

### Task 5: Entity placement utilities

**Files:**
- Create: `ui_web/src/components/solar-system/layout.ts`
- Create: `ui_web/src/components/solar-system/layout.test.ts`

These pure functions compute angular positions and polar-to-cartesian conversions, tested independently from React.

**Step 1: Write the tests**

`ui_web/src/components/solar-system/layout.test.ts`:
```tsx
import { describe, expect, it } from 'vitest'
import { angleFromId, polarToCartesian, transitPosition } from './layout'

describe('angleFromId', () => {
  it('returns a number between 0 and 2*PI', () => {
    const angle = angleFromId('asteroid_0001')
    expect(angle).toBeGreaterThanOrEqual(0)
    expect(angle).toBeLessThan(Math.PI * 2)
  })

  it('returns the same angle for the same ID', () => {
    expect(angleFromId('asteroid_0001')).toBe(angleFromId('asteroid_0001'))
  })

  it('returns different angles for different IDs', () => {
    expect(angleFromId('asteroid_0001')).not.toBe(angleFromId('asteroid_0002'))
  })
})

describe('polarToCartesian', () => {
  it('converts radius and angle 0 to (radius, 0)', () => {
    const { x, y } = polarToCartesian(100, 0)
    expect(x).toBeCloseTo(100)
    expect(y).toBeCloseTo(0)
  })

  it('converts angle PI/2 to (0, radius)', () => {
    const { x, y } = polarToCartesian(100, Math.PI / 2)
    expect(x).toBeCloseTo(0)
    expect(y).toBeCloseTo(100)
  })
})

describe('transitPosition', () => {
  it('returns origin position at progress 0', () => {
    const pos = transitPosition(
      { radius: 100, angle: 0 },
      { radius: 200, angle: Math.PI },
      0,
    )
    expect(pos.x).toBeCloseTo(100)
    expect(pos.y).toBeCloseTo(0)
  })

  it('returns destination position at progress 1', () => {
    const pos = transitPosition(
      { radius: 100, angle: 0 },
      { radius: 200, angle: Math.PI },
      1,
    )
    expect(pos.x).toBeCloseTo(-200)
    expect(pos.y).toBeCloseTo(0, 0)
  })

  it('returns midpoint at progress 0.5', () => {
    const pos = transitPosition(
      { radius: 100, angle: 0 },
      { radius: 300, angle: 0 },
      0.5,
    )
    expect(pos.x).toBeCloseTo(200)
    expect(pos.y).toBeCloseTo(0)
  })
})
```

**Step 2: Run tests to verify they fail**

Run: `cd ui_web && npm test`
Expected: FAIL — module not found

**Step 3: Implement the layout utilities**

`ui_web/src/components/solar-system/layout.ts`:
```ts
/**
 * Deterministic angle from an entity ID. Uses a simple hash to spread
 * entities around their orbital ring.
 */
export function angleFromId(id: string): number {
  let hash = 0
  for (let i = 0; i < id.length; i++) {
    hash = ((hash << 5) - hash + id.charCodeAt(i)) | 0
  }
  // Map to [0, 2*PI)
  return ((hash >>> 0) / 0xffffffff) * Math.PI * 2
}

export function polarToCartesian(radius: number, angle: number): { x: number; y: number } {
  return {
    x: radius * Math.cos(angle),
    y: radius * Math.sin(angle),
  }
}

/**
 * Interpolate between two polar positions for transit animation.
 * Linearly interpolates both radius and angle, then converts to cartesian.
 */
export function transitPosition(
  origin: { radius: number; angle: number },
  destination: { radius: number; angle: number },
  progress: number,
): { x: number; y: number } {
  const t = Math.max(0, Math.min(1, progress))
  const radius = origin.radius + (destination.radius - origin.radius) * t
  const angle = origin.angle + (destination.angle - origin.angle) * t
  return polarToCartesian(radius, angle)
}

/** Node ID → ring radius lookup */
const RING_RADII: Record<string, number> = {
  node_earth_orbit: 100,
  node_belt_inner: 200,
  node_belt_mid: 300,
  node_belt_outer: 400,
}

export function ringRadiusForNode(nodeId: string): number {
  return RING_RADII[nodeId] ?? 250
}
```

**Step 4: Run tests**

Run: `cd ui_web && npm test`
Expected: All tests pass

**Step 5: Commit**

```bash
git add ui_web/src/components/solar-system/layout.ts ui_web/src/components/solar-system/layout.test.ts
git commit -m "feat(ui): add layout utilities for entity placement on orbital rings"
```

---

### Task 6: Station and ship markers

**Files:**
- Modify: `ui_web/src/components/SolarSystemMap.tsx`
- Modify: `ui_web/src/components/SolarSystemMap.test.tsx`

**Step 1: Write the tests**

Add to `SolarSystemMap.test.tsx`:
```tsx
import type { ShipState, StationState } from '../types'

const snapshotWithEntities: SimSnapshot = {
  ...emptySnapshot,
  ships: {
    ship_001: {
      id: 'ship_001',
      location_node: 'node_earth_orbit',
      owner: 'player',
      cargo: {},
      cargo_capacity_m3: 20,
      task: null,
    },
  },
  stations: {
    station_001: {
      id: 'station_001',
      location_node: 'node_earth_orbit',
      power_available_per_tick: 100,
      cargo: {},
      cargo_capacity_m3: 10000,
      facilities: { compute_units_total: 10, power_per_compute_unit_per_tick: 1, efficiency: 1 },
    },
  },
}

it('renders station markers', () => {
  const { container } = render(
    <SolarSystemMap snapshot={snapshotWithEntities} currentTick={100} oreCompositions={{}} />
  )
  // Station is rendered as a rotated rect (diamond)
  const stationMarkers = container.querySelectorAll('[data-entity-type="station"]')
  expect(stationMarkers.length).toBe(1)
})

it('renders ship markers', () => {
  const { container } = render(
    <SolarSystemMap snapshot={snapshotWithEntities} currentTick={100} oreCompositions={{}} />
  )
  const shipMarkers = container.querySelectorAll('[data-entity-type="ship"]')
  expect(shipMarkers.length).toBe(1)
})
```

**Step 2: Run tests to verify they fail**

Run: `cd ui_web && npm test`
Expected: FAIL — no elements with data-entity-type found

**Step 3: Add entity markers to SolarSystemMap**

Add imports at top of `SolarSystemMap.tsx`:
```tsx
import { angleFromId, polarToCartesian, ringRadiusForNode, transitPosition } from './solar-system/layout'
```

Add helper to determine ship color from task:
```tsx
function shipColor(task: ShipState['task']): string {
  if (!task) return 'var(--color-dim)'
  const kind = Object.keys(task.kind)[0]
  switch (kind) {
    case 'Survey': return '#5b9bd5'
    case 'DeepScan': return '#7b68ee'
    case 'Mine': return '#d4a44c'
    case 'Deposit': return '#4caf7d'
    case 'Transit': return 'var(--color-accent)'
    default: return 'var(--color-dim)'
  }
}
```

Inside the `<g ref={groupRef}>`, after the rings, add:

```tsx
{/* Stations */}
{snapshot && Object.values(snapshot.stations).map((station) => {
  const radius = ringRadiusForNode(station.location_node)
  const angle = angleFromId(station.id)
  const { x, y } = polarToCartesian(radius, angle)
  return (
    <rect
      key={station.id}
      data-entity-type="station"
      data-entity-id={station.id}
      x={x - 6}
      y={y - 6}
      width={12}
      height={12}
      fill="var(--color-accent)"
      transform={`rotate(45 ${x} ${y})`}
      className="cursor-pointer"
    />
  )
})}

{/* Ships */}
{snapshot && Object.values(snapshot.ships).map((ship) => {
  let x: number, y: number
  const taskKind = ship.task ? Object.keys(ship.task.kind)[0] : null

  if (taskKind === 'Transit' && ship.task) {
    const transit = (ship.task.kind as { Transit: { destination: string } }).Transit
    const originRadius = ringRadiusForNode(ship.location_node)
    const originAngle = angleFromId(ship.id + ':origin')
    const destRadius = ringRadiusForNode(transit.destination)
    const destAngle = angleFromId(ship.id + ':dest')
    const progress = ship.task.eta_tick > ship.task.started_tick
      ? (currentTick - ship.task.started_tick) / (ship.task.eta_tick - ship.task.started_tick)
      : 1
    const pos = transitPosition(
      { radius: originRadius, angle: originAngle },
      { radius: destRadius, angle: destAngle },
      progress,
    )
    x = pos.x
    y = pos.y
  } else {
    const radius = ringRadiusForNode(ship.location_node)
    const angle = angleFromId(ship.id)
    const pos = polarToCartesian(radius, angle)
    x = pos.x
    y = pos.y
  }

  return (
    <polygon
      key={ship.id}
      data-entity-type="ship"
      data-entity-id={ship.id}
      points={`${x},${y - 6} ${x - 4},${y + 4} ${x + 4},${y + 4}`}
      fill={shipColor(ship.task)}
      className="cursor-pointer"
    />
  )
})}
```

**Step 4: Run tests**

Run: `cd ui_web && npm test`
Expected: All tests pass

**Step 5: Commit**

```bash
git add ui_web/src/components/SolarSystemMap.tsx ui_web/src/components/SolarSystemMap.test.tsx
git commit -m "feat(ui): add station and ship markers to solar system map"
```

---

### Task 7: Asteroid and scan site markers

**Files:**
- Modify: `ui_web/src/components/SolarSystemMap.tsx`
- Modify: `ui_web/src/components/SolarSystemMap.test.tsx`

**Step 1: Write the tests**

Add to `SolarSystemMap.test.tsx`:
```tsx
const snapshotWithAsteroids: SimSnapshot = {
  ...emptySnapshot,
  asteroids: {
    asteroid_0001: {
      id: 'asteroid_0001',
      location_node: 'node_belt_inner',
      anomaly_tags: ['IronRich'],
      mass_kg: 5000,
      knowledge: { tag_beliefs: [['IronRich', 0.85]], composition: null },
    },
  },
  scan_sites: [
    { id: 'site_001', node: 'node_belt_mid', template_id: 'tmpl_iron' },
  ],
}

it('renders asteroid markers', () => {
  const { container } = render(
    <SolarSystemMap snapshot={snapshotWithAsteroids} currentTick={100} oreCompositions={{}} />
  )
  const markers = container.querySelectorAll('[data-entity-type="asteroid"]')
  expect(markers.length).toBe(1)
})

it('renders scan site markers', () => {
  const { container } = render(
    <SolarSystemMap snapshot={snapshotWithAsteroids} currentTick={100} oreCompositions={{}} />
  )
  const markers = container.querySelectorAll('[data-entity-type="scan-site"]')
  expect(markers.length).toBe(1)
})
```

**Step 2: Run tests to verify they fail**

Run: `cd ui_web && npm test`
Expected: FAIL

**Step 3: Add asteroid and scan site rendering**

Inside the `<g ref={groupRef}>`, after ship markers:

```tsx
{/* Asteroids */}
{snapshot && Object.values(snapshot.asteroids).map((asteroid) => {
  const radius = ringRadiusForNode(asteroid.location_node)
  const angle = angleFromId(asteroid.id)
  const { x, y } = polarToCartesian(radius, angle)
  // Scale size by mass (log scale, clamped)
  const massKg = asteroid.mass_kg ?? 1000
  const size = Math.max(2, Math.min(8, Math.log10(massKg) - 1))
  const isIronRich = asteroid.anomaly_tags.includes('IronRich')

  return (
    <circle
      key={asteroid.id}
      data-entity-type="asteroid"
      data-entity-id={asteroid.id}
      cx={x}
      cy={y}
      r={size}
      fill={isIronRich ? '#a0522d' : 'var(--color-muted)'}
      opacity={0.8}
      className="cursor-pointer"
    />
  )
})}

{/* Scan sites */}
{snapshot && snapshot.scan_sites.map((site) => {
  const radius = ringRadiusForNode(site.node)
  const angle = angleFromId(site.id)
  const { x, y } = polarToCartesian(radius, angle)

  return (
    <text
      key={site.id}
      data-entity-type="scan-site"
      data-entity-id={site.id}
      x={x}
      y={y}
      textAnchor="middle"
      dominantBaseline="central"
      fill="var(--color-faint)"
      fontSize={8}
      fontFamily="monospace"
      className="cursor-pointer"
    >
      ?
    </text>
  )
})}
```

**Step 4: Run tests**

Run: `cd ui_web && npm test`
Expected: All tests pass

**Step 5: Commit**

```bash
git add ui_web/src/components/SolarSystemMap.tsx ui_web/src/components/SolarSystemMap.test.tsx
git commit -m "feat(ui): add asteroid and scan site markers to solar system map"
```

---

### Task 8: Hover tooltip

**Files:**
- Create: `ui_web/src/components/solar-system/Tooltip.tsx`
- Modify: `ui_web/src/components/SolarSystemMap.tsx`

**Step 1: Create the Tooltip component**

`ui_web/src/components/solar-system/Tooltip.tsx`:
```tsx
interface TooltipProps {
  x: number
  y: number
  children: React.ReactNode
}

export function Tooltip({ x, y, children }: TooltipProps) {
  return (
    <div
      className="absolute pointer-events-none bg-surface border border-edge rounded px-2 py-1 text-[11px] text-fg z-10 max-w-[200px]"
      style={{ left: x + 12, top: y - 8 }}
    >
      {children}
    </div>
  )
}
```

**Step 2: Add hover state and tooltip rendering to SolarSystemMap**

Add state to SolarSystemMap:
```tsx
const [hovered, setHovered] = useState<{
  type: string
  id: string
  screenX: number
  screenY: number
} | null>(null)
```

Add a helper to generate mouse handlers on entity elements:
```tsx
function entityMouseHandlers(type: string, id: string) {
  return {
    onMouseEnter: (e: React.MouseEvent) => {
      setHovered({ type, id, screenX: e.clientX, screenY: e.clientY })
    },
    onMouseMove: (e: React.MouseEvent) => {
      setHovered((prev) => prev ? { ...prev, screenX: e.clientX, screenY: e.clientY } : null)
    },
    onMouseLeave: () => setHovered(null),
  }
}
```

Spread `{...entityMouseHandlers('station', station.id)}` etc. onto each entity SVG element.

Add tooltip rendering after the SVG, inside the wrapping div:
```tsx
{hovered && snapshot && (
  <Tooltip x={hovered.screenX} y={hovered.screenY}>
    {/* Render entity details based on hovered.type and hovered.id */}
  </Tooltip>
)}
```

The tooltip content depends on entity type:
- **station**: ID, location, cargo total kg
- **ship**: ID, location, task label, cargo total kg
- **asteroid**: ID, mass, tags, composition summary
- **scan-site**: ID, template

**Step 3: Run tests**

Run: `cd ui_web && npm test`
Expected: All tests pass

**Step 4: Commit**

```bash
git add ui_web/src/components/solar-system/Tooltip.tsx ui_web/src/components/SolarSystemMap.tsx
git commit -m "feat(ui): add hover tooltips to solar system map entities"
```

---

### Task 9: Click-to-select with detail card

**Files:**
- Create: `ui_web/src/components/solar-system/DetailCard.tsx`
- Modify: `ui_web/src/components/SolarSystemMap.tsx`

**Step 1: Create the DetailCard component**

`ui_web/src/components/solar-system/DetailCard.tsx`:
```tsx
import type { OreCompositions } from '../../hooks/useSimStream'
import type { AsteroidState, ScanSite, ShipState, StationState } from '../../types'

interface DetailCardProps {
  entity:
    | { type: 'station'; data: StationState }
    | { type: 'ship'; data: ShipState }
    | { type: 'asteroid'; data: AsteroidState }
    | { type: 'scan-site'; data: ScanSite }
  oreCompositions: OreCompositions
  onClose: () => void
}

export function DetailCard({ entity, oreCompositions, onClose }: DetailCardProps) {
  return (
    <div className="absolute top-4 right-4 w-64 bg-surface border border-edge rounded p-3 text-[11px] text-fg z-20">
      <div className="flex justify-between items-center mb-2">
        <span className="text-bright font-bold uppercase tracking-wider">{entity.type}</span>
        <button onClick={onClose} className="text-faint hover:text-bright cursor-pointer">✕</button>
      </div>
      {/* Render details based on entity.type — reuse patterns from FleetPanel/AsteroidTable */}
      <div className="text-accent mb-1">{entity.data.id}</div>
      {entity.type === 'station' && <StationDetail station={entity.data} oreCompositions={oreCompositions} />}
      {entity.type === 'ship' && <ShipDetail ship={entity.data} />}
      {entity.type === 'asteroid' && <AsteroidDetail asteroid={entity.data} />}
      {entity.type === 'scan-site' && <ScanSiteDetail site={entity.data} />}
    </div>
  )
}

function StationDetail({ station, oreCompositions }: { station: StationState; oreCompositions: OreCompositions }) {
  const totalKg = Object.values(station.cargo).reduce((s, v) => s + v, 0)
  return (
    <>
      <div className="text-dim">{station.location_node}</div>
      <div className="text-muted mt-1">cargo: {totalKg.toLocaleString(undefined, { maximumFractionDigits: 1 })} kg</div>
    </>
  )
}

function ShipDetail({ ship }: { ship: ShipState }) {
  const taskKey = ship.task ? Object.keys(ship.task.kind)[0] : 'idle'
  const totalKg = Object.values(ship.cargo).reduce((s, v) => s + v, 0)
  return (
    <>
      <div className="text-dim">{ship.location_node} · {taskKey.toLowerCase()}</div>
      <div className="text-muted mt-1">
        cargo: {totalKg.toLocaleString(undefined, { maximumFractionDigits: 1 })} kg
      </div>
    </>
  )
}

function AsteroidDetail({ asteroid }: { asteroid: AsteroidState }) {
  return (
    <>
      <div className="text-dim">{asteroid.location_node}</div>
      {asteroid.mass_kg != null && (
        <div className="text-muted mt-1">mass: {asteroid.mass_kg.toLocaleString()} kg</div>
      )}
      {asteroid.anomaly_tags.length > 0 && (
        <div className="text-muted mt-1">tags: {asteroid.anomaly_tags.join(', ')}</div>
      )}
      {asteroid.knowledge.composition && (
        <div className="text-dim mt-1">
          {Object.entries(asteroid.knowledge.composition)
            .sort(([, a], [, b]) => b - a)
            .filter(([, frac]) => frac > 0.001)
            .map(([el, frac]) => `${el} ${Math.round(frac * 100)}%`)
            .join(' · ')}
        </div>
      )}
    </>
  )
}

function ScanSiteDetail({ site }: { site: ScanSite }) {
  return (
    <>
      <div className="text-dim">{site.node}</div>
      <div className="text-muted mt-1">template: {site.template_id}</div>
    </>
  )
}
```

**Step 2: Add selection state to SolarSystemMap**

```tsx
const [selected, setSelected] = useState<{ type: string; id: string } | null>(null)
```

Add click handler to entity elements:
```tsx
onClick={() => setSelected({ type: 'station', id: station.id })
```
(same pattern for ships, asteroids, scan sites)

Add click handler on SVG background to deselect:
```tsx
<svg ... onClick={(e) => { if (e.target === e.currentTarget) setSelected(null) }}>
```

Render DetailCard after the tooltip:
```tsx
{selected && snapshot && (() => {
  const entity = lookupEntity(selected, snapshot)
  if (!entity) return null
  return <DetailCard entity={entity} oreCompositions={oreCompositions} onClose={() => setSelected(null)} />
})()}
```

Implement `lookupEntity` as a helper that finds the entity by type/id from the snapshot.

**Step 3: Add highlight outline on selected entity**

When rendering each entity, check if it matches `selected` and add a stroke/outline:
```tsx
stroke={selected?.id === station.id ? 'var(--color-bright)' : undefined}
strokeWidth={selected?.id === station.id ? 2 : undefined}
```

**Step 4: Run tests**

Run: `cd ui_web && npm test`
Expected: All tests pass

**Step 5: Commit**

```bash
git add ui_web/src/components/solar-system/DetailCard.tsx ui_web/src/components/SolarSystemMap.tsx
git commit -m "feat(ui): add click-to-select with detail card on solar system map"
```

---

### Task 10: Visual polish and final testing

**Files:**
- Modify: `ui_web/src/components/SolarSystemMap.tsx` (minor tweaks)
- Modify: `ui_web/src/components/SolarSystemMap.test.tsx` (add integration tests)

**Step 1: Add integration test for view toggle flow**

Add to `App.test.tsx`:
```tsx
it('toggles between dashboard and map views', async () => {
  render(<App />)
  // Start in dashboard
  expect(screen.getByText('Events')).toBeInTheDocument()

  // Switch to map
  await userEvent.click(screen.getByText(/System Map/))
  expect(screen.queryByText('Events')).not.toBeInTheDocument()
  expect(screen.getByText('Earth Orbit')).toBeInTheDocument()

  // Switch back
  await userEvent.click(screen.getByText(/Dashboard/))
  expect(screen.getByText('Events')).toBeInTheDocument()
})
```

**Step 2: Add starfield background**

At the start of the `<g ref={groupRef}>`, add subtle background stars:
```tsx
{/* Starfield */}
{Array.from({ length: 80 }, (_, i) => {
  const sx = ((i * 7919 + 1) % 1000) - 500
  const sy = ((i * 6271 + 3) % 1000) - 500
  const size = (i % 3 === 0) ? 1.5 : 0.8
  return <circle key={`star-${i}`} cx={sx} cy={sy} r={size} fill="var(--color-faint)" opacity={0.3 + (i % 5) * 0.1} />
})}
```

**Step 3: Run all tests**

Run: `cd ui_web && npm test`
Expected: All tests pass

**Step 4: Manual verification**

Run: `cd ui_web && npm run build`
Expected: Build succeeds with no type errors

**Step 5: Final commit**

```bash
git add -A
git commit -m "feat(ui): polish solar system map with starfield and integration tests"
```

---

### Summary of all commits

1. `feat(ui): add d3-zoom and d3-selection dependencies`
2. `feat(ui): add view toggle between dashboard and system map`
3. `feat(ui): add d3-zoom pan/zoom hook for SVG`
4. `feat(ui): add SolarSystemMap with orbital rings and d3-zoom`
5. `feat(ui): add layout utilities for entity placement on orbital rings`
6. `feat(ui): add station and ship markers to solar system map`
7. `feat(ui): add asteroid and scan site markers to solar system map`
8. `feat(ui): add hover tooltips to solar system map entities`
9. `feat(ui): add click-to-select with detail card on solar system map`
10. `feat(ui): polish solar system map with starfield and integration tests`
