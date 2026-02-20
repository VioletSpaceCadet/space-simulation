# FE Foundations Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add sortable table columns and collapsible panels with persistent state to the mission control UI.

**Architecture:** A generic `useSortableData<T>` hook provides reusable sorting logic. A shared `<PanelHeader>` component provides clickable collapse/expand headers with hover highlighting. Panel collapse uses react-resizable-panels v2 native `collapsible`/`collapsedSize` props. Collapse state persists to `localStorage`.

**Tech Stack:** React 18, TypeScript 5, Tailwind CSS v4, react-resizable-panels v2, Vitest + React Testing Library

**Working directory:** `/Users/joshuamcmorris/space-simulation/.worktrees/fe-foundations`

**CRITICAL — file operations, use the correct tools:**
- READ files: Read tool only (NOT cat/head/tail)
- CREATE new files: Write tool only (NOT cat heredoc, NOT echo redirection)
- MODIFY existing files: Edit tool only (NOT sed/awk/cat)
- Bash is only for: git, npm install/test, other shell commands

**Run tests with:** `cd /Users/joshuamcmorris/space-simulation/.worktrees/fe-foundations/ui_web && npm test -- --run`

---

### Task 1: Create `useSortableData` hook

**Files:**
- Create: `ui_web/src/hooks/useSortableData.ts`
- Create: `ui_web/src/hooks/useSortableData.test.ts`

**Step 1: Write the test file**

Create `ui_web/src/hooks/useSortableData.test.ts`:

```ts
import { act, renderHook } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { useSortableData } from './useSortableData'

interface Row {
  id: string
  name: string
  mass: number
}

const data: Row[] = [
  { id: 'c', name: 'Charlie', mass: 300 },
  { id: 'a', name: 'Alpha', mass: 100 },
  { id: 'b', name: 'Bravo', mass: 200 },
]

describe('useSortableData', () => {
  it('returns data in original order when no sort applied', () => {
    const { result } = renderHook(() => useSortableData(data))
    expect(result.current.sortedData.map((r) => r.id)).toEqual(['c', 'a', 'b'])
    expect(result.current.sortConfig).toBeNull()
  })

  it('sorts ascending on first click', () => {
    const { result } = renderHook(() => useSortableData(data))
    act(() => result.current.requestSort('id'))
    expect(result.current.sortedData.map((r) => r.id)).toEqual(['a', 'b', 'c'])
    expect(result.current.sortConfig).toEqual({ key: 'id', direction: 'asc' })
  })

  it('sorts descending on second click', () => {
    const { result } = renderHook(() => useSortableData(data))
    act(() => result.current.requestSort('id'))
    act(() => result.current.requestSort('id'))
    expect(result.current.sortedData.map((r) => r.id)).toEqual(['c', 'b', 'a'])
    expect(result.current.sortConfig).toEqual({ key: 'id', direction: 'desc' })
  })

  it('clears sort on third click', () => {
    const { result } = renderHook(() => useSortableData(data))
    act(() => result.current.requestSort('id'))
    act(() => result.current.requestSort('id'))
    act(() => result.current.requestSort('id'))
    expect(result.current.sortedData.map((r) => r.id)).toEqual(['c', 'a', 'b'])
    expect(result.current.sortConfig).toBeNull()
  })

  it('sorts numbers correctly', () => {
    const { result } = renderHook(() => useSortableData(data))
    act(() => result.current.requestSort('mass'))
    expect(result.current.sortedData.map((r) => r.mass)).toEqual([100, 200, 300])
  })

  it('resets to ascending when switching columns', () => {
    const { result } = renderHook(() => useSortableData(data))
    act(() => result.current.requestSort('id'))
    act(() => result.current.requestSort('id')) // desc
    act(() => result.current.requestSort('mass')) // new column -> asc
    expect(result.current.sortConfig).toEqual({ key: 'mass', direction: 'asc' })
  })

  it('updates when data changes', () => {
    const { result, rerender } = renderHook(
      ({ items }) => useSortableData(items),
      { initialProps: { items: data } },
    )
    act(() => result.current.requestSort('mass'))
    const newData = [...data, { id: 'd', name: 'Delta', mass: 50 }]
    rerender({ items: newData })
    expect(result.current.sortedData[0].mass).toBe(50)
  })
})
```

**Step 2: Run test to verify it fails**

Run: `cd /Users/joshuamcmorris/space-simulation/.worktrees/fe-foundations/ui_web && npm test -- --run`
Expected: FAIL — module `./useSortableData` not found

**Step 3: Write the hook implementation**

Create `ui_web/src/hooks/useSortableData.ts`:

```ts
import { useMemo, useState } from 'react'

export type SortDirection = 'asc' | 'desc'

export interface SortConfig<T> {
  key: keyof T & string
  direction: SortDirection
}

export interface SortableResult<T> {
  sortedData: T[]
  sortConfig: SortConfig<T> | null
  requestSort: (key: keyof T & string) => void
}

export function useSortableData<T>(data: T[]): SortableResult<T> {
  const [sortConfig, setSortConfig] = useState<SortConfig<T> | null>(null)

  const sortedData = useMemo(() => {
    if (!sortConfig) return data

    const { key, direction } = sortConfig
    return [...data].sort((a, b) => {
      const aVal = a[key]
      const bVal = b[key]
      if (aVal == null && bVal == null) return 0
      if (aVal == null) return 1
      if (bVal == null) return -1

      let cmp: number
      if (typeof aVal === 'number' && typeof bVal === 'number') {
        cmp = aVal - bVal
      } else {
        cmp = String(aVal).localeCompare(String(bVal))
      }
      return direction === 'asc' ? cmp : -cmp
    })
  }, [data, sortConfig])

  function requestSort(key: keyof T & string) {
    setSortConfig((prev) => {
      if (!prev || prev.key !== key) return { key, direction: 'asc' }
      if (prev.direction === 'asc') return { key, direction: 'desc' }
      return null
    })
  }

  return { sortedData, sortConfig, requestSort }
}
```

**Step 4: Run tests to verify they pass**

Run: `cd /Users/joshuamcmorris/space-simulation/.worktrees/fe-foundations/ui_web && npm test -- --run`
Expected: All `useSortableData` tests PASS

**Step 5: Commit**

```bash
cd /Users/joshuamcmorris/space-simulation/.worktrees/fe-foundations
git add ui_web/src/hooks/useSortableData.ts ui_web/src/hooks/useSortableData.test.ts
git commit -m "feat(ui): add useSortableData hook with tests"
```

---

### Task 2: Create `PanelHeader` component

**Files:**
- Create: `ui_web/src/components/PanelHeader.tsx`
- Create: `ui_web/src/components/PanelHeader.test.tsx`

**Step 1: Write the test file**

Create `ui_web/src/components/PanelHeader.test.tsx`:

```tsx
import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { PanelHeader } from './PanelHeader'

describe('PanelHeader', () => {
  it('renders the title', () => {
    render(<PanelHeader title="Events" collapsed={false} onToggle={() => {}} />)
    expect(screen.getByText('Events')).toBeInTheDocument()
  })

  it('shows expanded indicator when not collapsed', () => {
    render(<PanelHeader title="Events" collapsed={false} onToggle={() => {}} />)
    expect(screen.getByText('▾')).toBeInTheDocument()
  })

  it('shows collapsed indicator when collapsed', () => {
    render(<PanelHeader title="Events" collapsed={true} onToggle={() => {}} />)
    expect(screen.getByText('▸')).toBeInTheDocument()
  })

  it('calls onToggle when clicked', () => {
    const onToggle = vi.fn()
    render(<PanelHeader title="Events" collapsed={false} onToggle={onToggle} />)
    fireEvent.click(screen.getByText('Events'))
    expect(onToggle).toHaveBeenCalledOnce()
  })
})
```

**Step 2: Run test to verify it fails**

Run: `cd /Users/joshuamcmorris/space-simulation/.worktrees/fe-foundations/ui_web && npm test -- --run`
Expected: FAIL — module `./PanelHeader` not found

**Step 3: Write the component**

Create `ui_web/src/components/PanelHeader.tsx`:

```tsx
interface Props {
  title: string
  collapsed: boolean
  onToggle: () => void
}

export function PanelHeader({ title, collapsed, onToggle }: Props) {
  return (
    <button
      type="button"
      onClick={onToggle}
      className="flex items-center gap-1.5 w-full text-left text-[11px] uppercase tracking-widest text-label mb-2 pb-1.5 border-b border-edge shrink-0 hover:bg-edge/30 transition-colors cursor-pointer rounded-sm px-1 -mx-1"
    >
      <span className="text-[9px] leading-none">{collapsed ? '▸' : '▾'}</span>
      <span>{title}</span>
    </button>
  )
}
```

**Step 4: Run tests to verify they pass**

Run: `cd /Users/joshuamcmorris/space-simulation/.worktrees/fe-foundations/ui_web && npm test -- --run`
Expected: All `PanelHeader` tests PASS

**Step 5: Commit**

```bash
cd /Users/joshuamcmorris/space-simulation/.worktrees/fe-foundations
git add ui_web/src/components/PanelHeader.tsx ui_web/src/components/PanelHeader.test.tsx
git commit -m "feat(ui): add PanelHeader component with tests"
```

---

### Task 3: Wire collapsible panels into `App.tsx`

**Files:**
- Modify: `ui_web/src/App.tsx`
- Modify: `ui_web/src/App.test.tsx`

This task replaces the existing `<h2>` headers in App.tsx with `<PanelHeader>` components, adds collapse state with `localStorage` persistence, and wires the react-resizable-panels `collapsible` + `collapsedSize` + `onCollapse`/`onExpand` props.

**Step 1: Update the App.test.tsx test**

The existing test `renders all four panel headings` searches for text "Events", "Asteroids", etc. The PanelHeader renders these inside a `<button>`, so text queries should still find them. However, we need to add tests for collapse behavior.

Modify `ui_web/src/App.test.tsx` to add these tests at the end of the `describe('App', ...)` block:

```tsx
  it('renders PanelHeader for each panel', () => {
    render(<App />)
    // PanelHeader renders buttons with panel titles
    const buttons = screen.getAllByRole('button')
    const titles = buttons.map((b) => b.textContent?.replace(/[▸▾]\s*/, ''))
    expect(titles).toContain('Events')
    expect(titles).toContain('Asteroids')
    expect(titles).toContain('Fleet')
    expect(titles).toContain('Research')
  })
```

**Step 2: Run test to verify it fails**

Run: `cd /Users/joshuamcmorris/space-simulation/.worktrees/fe-foundations/ui_web && npm test -- --run`
Expected: FAIL — no buttons with those titles yet (the h2 elements are not buttons)

**Step 3: Modify App.tsx**

Replace the entire content of `ui_web/src/App.tsx` with:

```tsx
import { useCallback, useState } from 'react'
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels'
import { AsteroidTable } from './components/AsteroidTable'
import { EventsFeed } from './components/EventsFeed'
import { FleetPanel } from './components/FleetPanel'
import { PanelHeader } from './components/PanelHeader'
import { ResearchPanel } from './components/ResearchPanel'
import { StatusBar } from './components/StatusBar'
import { useSimStream } from './hooks/useSimStream'

function readCollapsed(key: string): boolean {
  try {
    return localStorage.getItem(`panel:${key}:collapsed`) === 'true'
  } catch {
    return false
  }
}

function writeCollapsed(key: string, collapsed: boolean) {
  try {
    localStorage.setItem(`panel:${key}:collapsed`, String(collapsed))
  } catch {
    // localStorage unavailable
  }
}

function usePanelCollapse(key: string) {
  const [collapsed, setCollapsed] = useState(() => readCollapsed(key))

  const toggle = useCallback(() => {
    setCollapsed((prev) => {
      const next = !prev
      writeCollapsed(key, next)
      return next
    })
  }, [key])

  const onCollapse = useCallback(() => {
    setCollapsed(true)
    writeCollapsed(key, true)
  }, [key])

  const onExpand = useCallback(() => {
    setCollapsed(false)
    writeCollapsed(key, false)
  }, [key])

  return { collapsed, toggle, onCollapse, onExpand }
}

export default function App() {
  const { snapshot, events, connected, currentTick, oreCompositions } = useSimStream()

  const eventsPanel = usePanelCollapse('events')
  const asteroidsPanel = usePanelCollapse('asteroids')
  const fleetPanel = usePanelCollapse('fleet')
  const researchPanel = usePanelCollapse('research')

  return (
    <div className="flex flex-col h-screen overflow-hidden">
      <StatusBar tick={currentTick} connected={connected} />
      <PanelGroup direction="horizontal" className="flex-1 overflow-hidden">
        <Panel
          defaultSize={20}
          minSize={12}
          collapsible
          collapsedSize={0}
          onCollapse={eventsPanel.onCollapse}
          onExpand={eventsPanel.onExpand}
        >
          <section className="flex flex-col h-full overflow-hidden bg-void p-3">
            <PanelHeader title="Events" collapsed={eventsPanel.collapsed} onToggle={eventsPanel.toggle} />
            {!eventsPanel.collapsed && <EventsFeed events={events} />}
          </section>
        </Panel>
        <PanelResizeHandle className="w-px bg-edge hover:bg-dim cursor-col-resize transition-colors" />
        <Panel
          defaultSize={40}
          minSize={20}
          collapsible
          collapsedSize={0}
          onCollapse={asteroidsPanel.onCollapse}
          onExpand={asteroidsPanel.onExpand}
        >
          <section className="flex flex-col h-full overflow-hidden bg-void p-3">
            <PanelHeader title="Asteroids" collapsed={asteroidsPanel.collapsed} onToggle={asteroidsPanel.toggle} />
            {!asteroidsPanel.collapsed && <AsteroidTable asteroids={snapshot?.asteroids ?? {}} />}
          </section>
        </Panel>
        <PanelResizeHandle className="w-px bg-edge hover:bg-dim cursor-col-resize transition-colors" />
        <Panel
          defaultSize={20}
          minSize={12}
          collapsible
          collapsedSize={0}
          onCollapse={fleetPanel.onCollapse}
          onExpand={fleetPanel.onExpand}
        >
          <section className="flex flex-col h-full overflow-hidden bg-void p-3">
            <PanelHeader title="Fleet" collapsed={fleetPanel.collapsed} onToggle={fleetPanel.toggle} />
            {!fleetPanel.collapsed && <FleetPanel ships={snapshot?.ships ?? {}} stations={snapshot?.stations ?? {}} oreCompositions={oreCompositions} />}
          </section>
        </Panel>
        <PanelResizeHandle className="w-px bg-edge hover:bg-dim cursor-col-resize transition-colors" />
        <Panel
          defaultSize={20}
          minSize={12}
          collapsible
          collapsedSize={0}
          onCollapse={researchPanel.onCollapse}
          onExpand={researchPanel.onExpand}
        >
          <section className="flex flex-col h-full overflow-hidden bg-void p-3">
            <PanelHeader title="Research" collapsed={researchPanel.collapsed} onToggle={researchPanel.toggle} />
            {!researchPanel.collapsed && snapshot && <ResearchPanel research={snapshot.research} />}
          </section>
        </Panel>
      </PanelGroup>
    </div>
  )
}
```

**Step 4: Run tests to verify they pass**

Run: `cd /Users/joshuamcmorris/space-simulation/.worktrees/fe-foundations/ui_web && npm test -- --run`
Expected: All tests PASS. The existing test `renders all four panel headings` should still pass since PanelHeader renders the text content.

**Step 5: Commit**

```bash
cd /Users/joshuamcmorris/space-simulation/.worktrees/fe-foundations
git add ui_web/src/App.tsx ui_web/src/App.test.tsx
git commit -m "feat(ui): wire collapsible panels with PanelHeader and localStorage persistence"
```

---

### Task 4: Add sortable columns to `AsteroidTable`

**Files:**
- Modify: `ui_web/src/components/AsteroidTable.tsx`
- Modify: `ui_web/src/components/AsteroidTable.test.tsx`

**Step 1: Add sorting test**

Add this test to `ui_web/src/components/AsteroidTable.test.tsx` in the existing `describe` block. Also update imports to include `fireEvent`.

First, change the import line from:
```ts
import { render, screen } from '@testing-library/react'
```
to:
```ts
import { fireEvent, render, screen } from '@testing-library/react'
```

Then add a second asteroid to the test data and add sorting tests:

Replace the entire `asteroids` const and add the new one:
```ts
const asteroids: Record<string, AsteroidState> = {
  'asteroid_0001': {
    id: 'asteroid_0001',
    location_node: 'node_belt_inner',
    anomaly_tags: ['IronRich'],
    mass_kg: 5000,
    knowledge: {
      tag_beliefs: [['IronRich', 0.85]],
      composition: { Fe: 0.65, Si: 0.20, He: 0.15 },
    },
  },
  'asteroid_0002': {
    id: 'asteroid_0002',
    location_node: 'node_belt_outer',
    anomaly_tags: ['IronRich'],
    mass_kg: 1000,
    knowledge: {
      tag_beliefs: [['IronRich', 0.90]],
      composition: { Fe: 0.80, Si: 0.10, He: 0.10 },
    },
  },
}
```

Add these tests inside the describe block:

```tsx
  it('renders sort indicators on column headers', () => {
    render(<AsteroidTable asteroids={asteroids} />)
    // All columns should have unsorted indicator
    const headers = screen.getAllByRole('columnheader')
    // At least one header should contain the unsorted indicator
    expect(headers.some((h) => h.textContent?.includes('⇅'))).toBe(true)
  })

  it('sorts by mass ascending on click', () => {
    render(<AsteroidTable asteroids={asteroids} />)
    const massHeader = screen.getByText(/Mass/)
    fireEvent.click(massHeader)
    const rows = screen.getAllByRole('row').slice(1) // skip header row
    const masses = rows.map((r) => r.cells?.[4]?.textContent ?? r.querySelectorAll('td')[4]?.textContent)
    // asteroid_0002 (1000) should come before asteroid_0001 (5000)
    expect(masses[0]).toMatch(/1,000/)
    expect(masses[1]).toMatch(/5,000/)
  })
```

**Step 2: Run test to verify it fails**

Run: `cd /Users/joshuamcmorris/space-simulation/.worktrees/fe-foundations/ui_web && npm test -- --run`
Expected: FAIL — no sort indicators, no click behavior

**Step 3: Update AsteroidTable component**

Replace the entire content of `ui_web/src/components/AsteroidTable.tsx`:

```tsx
import { useSortableData } from '../hooks/useSortableData'
import type { AsteroidState } from '../types'

interface Props {
  asteroids: Record<string, AsteroidState>
}

function pct(value: number): string {
  return `${Math.round(value * 100)}%`
}

function compositionSummary(composition: Record<string, number> | null): string {
  if (!composition) return '—'
  return Object.entries(composition)
    .sort(([, a], [, b]) => b - a)
    .map(([el, frac]) => `${el} ${pct(frac)}`)
    .join(' | ')
}

function tagSummary(tagBeliefs: [string, number][]): string {
  if (tagBeliefs.length === 0) return '—'
  return tagBeliefs.map(([tag, conf]) => `${tag} (${pct(conf)})`).join(', ')
}

function primaryFraction(asteroid: AsteroidState): number {
  const comp = asteroid.knowledge.composition
  if (!comp) return 0
  return Math.max(...Object.values(comp), 0)
}

interface SortableAsteroid {
  id: string
  location_node: string
  mass_kg: number
  primary_fraction: number
  asteroid: AsteroidState
}

function SortIndicator({ column, sortConfig }: {
  column: string
  sortConfig: { key: string; direction: string } | null
}) {
  if (!sortConfig || sortConfig.key !== column) {
    return <span className="text-faint/40 ml-1">⇅</span>
  }
  return (
    <span className="text-accent ml-1">
      {sortConfig.direction === 'asc' ? '▲' : '▼'}
    </span>
  )
}

export function AsteroidTable({ asteroids }: Props) {
  const rows = Object.values(asteroids)

  const sortableRows: SortableAsteroid[] = rows.map((asteroid) => ({
    id: asteroid.id,
    location_node: asteroid.location_node,
    mass_kg: asteroid.mass_kg ?? -1,
    primary_fraction: primaryFraction(asteroid),
    asteroid,
  }))

  const { sortedData, sortConfig, requestSort } = useSortableData(sortableRows)

  if (rows.length === 0) {
    return (
      <div className="overflow-auto flex-1">
        <div className="text-faint italic">no bodies discovered</div>
      </div>
    )
  }

  const headerClass = "text-left text-label px-2 py-1 border-b border-edge font-normal cursor-pointer hover:text-dim transition-colors select-none"

  return (
    <div className="overflow-auto flex-1">
      <table className="min-w-max w-full border-collapse text-[11px]">
        <thead>
          <tr>
            <th className={headerClass} onClick={() => requestSort('id')}>
              ID<SortIndicator column="id" sortConfig={sortConfig} />
            </th>
            <th className={headerClass} onClick={() => requestSort('location_node')}>
              Node<SortIndicator column="location_node" sortConfig={sortConfig} />
            </th>
            <th className="text-left text-label px-2 py-1 border-b border-edge font-normal">Tags</th>
            <th className={headerClass} onClick={() => requestSort('primary_fraction')}>
              Composition<SortIndicator column="primary_fraction" sortConfig={sortConfig} />
            </th>
            <th className={headerClass} onClick={() => requestSort('mass_kg')}>
              Mass<SortIndicator column="mass_kg" sortConfig={sortConfig} />
            </th>
          </tr>
        </thead>
        <tbody>
          {sortedData.map(({ asteroid }) => (
            <tr key={asteroid.id}>
              <td className="px-2 py-0.5 border-b border-surface">{asteroid.id}</td>
              <td className="px-2 py-0.5 border-b border-surface">{asteroid.location_node}</td>
              <td className="px-2 py-0.5 border-b border-surface">{tagSummary(asteroid.knowledge.tag_beliefs)}</td>
              <td className="px-2 py-0.5 border-b border-surface">{compositionSummary(asteroid.knowledge.composition)}</td>
              <td className="px-2 py-0.5 border-b border-surface">
                {asteroid.mass_kg === undefined
                  ? <span className="text-faint">—</span>
                  : asteroid.mass_kg > 0
                    ? `${asteroid.mass_kg.toLocaleString(undefined, { maximumFractionDigits: 0 })} kg`
                    : <span className="text-faint">depleted</span>}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
```

**Step 4: Run tests to verify they pass**

Run: `cd /Users/joshuamcmorris/space-simulation/.worktrees/fe-foundations/ui_web && npm test -- --run`
Expected: All tests PASS

**Step 5: Commit**

```bash
cd /Users/joshuamcmorris/space-simulation/.worktrees/fe-foundations
git add ui_web/src/components/AsteroidTable.tsx ui_web/src/components/AsteroidTable.test.tsx
git commit -m "feat(ui): add sortable columns to AsteroidTable"
```

---

### Task 5: Final integration test pass

**Files:**
- None modified — verification only

**Step 1: Run full test suite**

Run: `cd /Users/joshuamcmorris/space-simulation/.worktrees/fe-foundations/ui_web && npm test -- --run`
Expected: All tests PASS (33 original + new tests)

**Step 2: Run build to check for type errors**

Run: `cd /Users/joshuamcmorris/space-simulation/.worktrees/fe-foundations/ui_web && npm run build`
Expected: Build succeeds with no errors

**Step 3: Verify no lint issues**

Run: `cd /Users/joshuamcmorris/space-simulation/.worktrees/fe-foundations/ui_web && npx eslint src/ --max-warnings 0 2>&1 || true`
Expected: No new errors (warnings from existing code are ok)
