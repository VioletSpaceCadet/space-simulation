# Draggable Panel Layout Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make frontend panels draggable, reorderable, and vertically stackable with VS Code-style drop zone indicators and localStorage persistence.

**Architecture:** Replace the flat `visiblePanels` array with a recursive layout tree (`LayoutNode`). Each node is a leaf (single panel) or a group (horizontal/vertical split). `@dnd-kit` provides drag interaction; `react-resizable-panels` continues to handle resize. A recursive `LayoutRenderer` component walks the tree. Drop zone overlays on panel edges guide the user during drag.

**Tech Stack:** React 19, TypeScript 5, @dnd-kit/core + @dnd-kit/sortable + @dnd-kit/utilities, react-resizable-panels v2, Tailwind v4, Vitest + Testing Library.

**Test runner:** `cd ui_web && npm test` (vitest run). For watch mode: `cd ui_web && npm run test:watch`.

**ESLint note:** ESLint blocks `_` and `_name` in destructuring — use `Object.fromEntries(Object.entries(...).filter(...))` instead.

---

### Task 1: Install @dnd-kit dependencies

**Files:**
- Modify: `ui_web/package.json`

**Step 1: Install packages**

```bash
cd ui_web && npm install @dnd-kit/core @dnd-kit/sortable @dnd-kit/utilities
```

**Step 2: Verify install**

```bash
cd ui_web && npm test
```

All existing tests should still pass.

**Step 3: Commit**

```bash
git add ui_web/package.json ui_web/package-lock.json
git commit -m "chore: add @dnd-kit dependencies for draggable panels"
```

---

### Task 2: Layout tree types and manipulation helpers

**Files:**
- Create: `ui_web/src/layout.ts`
- Create: `ui_web/src/layout.test.ts`

This task creates the core data model: the `LayoutNode` tree type, serialization/deserialization for localStorage, and pure functions to manipulate the tree (insert, remove, move panels).

**Step 1: Write the failing tests**

Create `ui_web/src/layout.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import {
  type LayoutNode,
  type PanelId,
  buildDefaultLayout,
  findPanelIds,
  removePanel,
  insertPanel,
  movePanel,
  serializeLayout,
  deserializeLayout,
} from './layout'

describe('buildDefaultLayout', () => {
  it('creates horizontal group with one leaf per visible panel', () => {
    const layout = buildDefaultLayout(['map', 'events', 'fleet'])
    expect(layout).toEqual({
      type: 'group',
      direction: 'horizontal',
      children: [
        { type: 'leaf', panelId: 'map' },
        { type: 'leaf', panelId: 'events' },
        { type: 'leaf', panelId: 'fleet' },
      ],
    })
  })

  it('returns single leaf wrapped in horizontal group for one panel', () => {
    const layout = buildDefaultLayout(['map'])
    expect(layout.type).toBe('group')
    expect(layout.children).toHaveLength(1)
  })
})

describe('findPanelIds', () => {
  it('collects all panel IDs from nested layout', () => {
    const layout: LayoutNode = {
      type: 'group',
      direction: 'horizontal',
      children: [
        { type: 'leaf', panelId: 'map' },
        {
          type: 'group',
          direction: 'vertical',
          children: [
            { type: 'leaf', panelId: 'events' },
            { type: 'leaf', panelId: 'fleet' },
          ],
        },
      ],
    }
    const ids = findPanelIds(layout)
    expect(ids.sort()).toEqual(['events', 'fleet', 'map'])
  })
})

describe('removePanel', () => {
  it('removes a leaf and returns simplified tree', () => {
    const layout = buildDefaultLayout(['map', 'events', 'fleet'])
    const result = removePanel(layout, 'events')
    expect(findPanelIds(result)).toEqual(['map', 'fleet'])
  })

  it('collapses single-child group after removal', () => {
    const layout: LayoutNode = {
      type: 'group',
      direction: 'horizontal',
      children: [
        { type: 'leaf', panelId: 'map' },
        {
          type: 'group',
          direction: 'vertical',
          children: [
            { type: 'leaf', panelId: 'events' },
            { type: 'leaf', panelId: 'fleet' },
          ],
        },
      ],
    }
    const result = removePanel(layout, 'events')
    // The vertical group should collapse since it has only one child
    expect(result).toEqual({
      type: 'group',
      direction: 'horizontal',
      children: [
        { type: 'leaf', panelId: 'map' },
        { type: 'leaf', panelId: 'fleet' },
      ],
    })
  })
})

describe('insertPanel', () => {
  it('inserts panel adjacent to target (after)', () => {
    const layout = buildDefaultLayout(['map', 'fleet'])
    const result = insertPanel(layout, 'events', 'map', 'after')
    expect(findPanelIds(result)).toEqual(['map', 'events', 'fleet'])
  })

  it('inserts panel adjacent to target (before)', () => {
    const layout = buildDefaultLayout(['map', 'fleet'])
    const result = insertPanel(layout, 'events', 'fleet', 'before')
    expect(findPanelIds(result)).toEqual(['map', 'events', 'fleet'])
  })

  it('inserts panel in vertical stack (below)', () => {
    const layout = buildDefaultLayout(['map', 'fleet'])
    const result = insertPanel(layout, 'events', 'map', 'below')
    // map and events should now be in a vertical group
    const root = result as { type: 'group'; children: LayoutNode[] }
    expect(root.children).toHaveLength(2) // vertical group + fleet
    const vertGroup = root.children[0] as { type: 'group'; direction: string; children: LayoutNode[] }
    expect(vertGroup.type).toBe('group')
    expect(vertGroup.direction).toBe('vertical')
    expect(findPanelIds(vertGroup)).toEqual(['map', 'events'])
  })

  it('inserts panel in vertical stack (above)', () => {
    const layout = buildDefaultLayout(['map', 'fleet'])
    const result = insertPanel(layout, 'events', 'map', 'above')
    const root = result as { type: 'group'; children: LayoutNode[] }
    const vertGroup = root.children[0] as { type: 'group'; direction: string; children: LayoutNode[] }
    expect(vertGroup.direction).toBe('vertical')
    // events should come before map
    expect(findPanelIds(vertGroup)).toEqual(['events', 'map'])
  })
})

describe('movePanel', () => {
  it('reorders panel within same level', () => {
    const layout = buildDefaultLayout(['map', 'events', 'fleet'])
    const result = movePanel(layout, 'fleet', 'map', 'after')
    expect(findPanelIds(result)).toEqual(['map', 'fleet', 'events'])
  })

  it('moves panel from one group to vertical stack in another', () => {
    const layout = buildDefaultLayout(['map', 'events', 'fleet'])
    const result = movePanel(layout, 'fleet', 'map', 'below')
    const ids = findPanelIds(result)
    expect(ids.sort()).toEqual(['events', 'fleet', 'map'])
  })
})

describe('serialize / deserialize', () => {
  it('round-trips a layout', () => {
    const layout = buildDefaultLayout(['map', 'events', 'fleet'])
    const json = serializeLayout(layout)
    const restored = deserializeLayout(json)
    expect(restored).toEqual(layout)
  })

  it('returns null for invalid JSON', () => {
    expect(deserializeLayout('not json')).toBeNull()
  })

  it('returns null for invalid structure', () => {
    expect(deserializeLayout('{"type":"unknown"}')).toBeNull()
  })
})
```

**Step 2: Run tests to verify they fail**

```bash
cd ui_web && npm test
```

Expected: FAIL — `./layout` module doesn't exist yet.

**Step 3: Implement layout.ts**

Create `ui_web/src/layout.ts`:

```ts
export type PanelId = 'map' | 'events' | 'asteroids' | 'fleet' | 'research'

export type LeafNode = { type: 'leaf'; panelId: PanelId }
export type GroupNode = {
  type: 'group'
  direction: 'horizontal' | 'vertical'
  children: LayoutNode[]
}
export type LayoutNode = LeafNode | GroupNode

export const ALL_PANELS: PanelId[] = ['map', 'events', 'asteroids', 'fleet', 'research']

export const PANEL_LABELS: Record<PanelId, string> = {
  map: 'Map',
  events: 'Events',
  asteroids: 'Asteroids',
  fleet: 'Fleet',
  research: 'Research',
}

/** Build a flat horizontal layout from visible panel IDs. */
export function buildDefaultLayout(panels: PanelId[]): GroupNode {
  return {
    type: 'group',
    direction: 'horizontal',
    children: panels.map((panelId) => ({ type: 'leaf', panelId })),
  }
}

/** Collect all panel IDs in a layout tree. */
export function findPanelIds(node: LayoutNode): PanelId[] {
  if (node.type === 'leaf') return [node.panelId]
  return node.children.flatMap(findPanelIds)
}

/** Remove a panel from the tree, collapsing single-child groups. */
export function removePanel(node: LayoutNode, panelId: PanelId): LayoutNode {
  if (node.type === 'leaf') return node // caller handles leaf removal
  const children = node.children
    .filter((child) => !(child.type === 'leaf' && child.panelId === panelId))
    .map((child) => (child.type === 'group' ? removePanel(child, panelId) : child))
    // Collapse single-child groups
    .flatMap((child) =>
      child.type === 'group' && child.children.length === 1 ? child.children : [child],
    )
  if (children.length === 0) {
    // Return a minimal group (shouldn't happen in practice)
    return { type: 'group', direction: node.direction, children: [] }
  }
  return { type: 'group', direction: node.direction, children }
}

type InsertPosition = 'before' | 'after' | 'above' | 'below'

/** Insert a new panel relative to a target panel. */
export function insertPanel(
  node: LayoutNode,
  panelId: PanelId,
  targetId: PanelId,
  position: InsertPosition,
): LayoutNode {
  if (node.type === 'leaf') return node

  const targetIndex = node.children.findIndex(
    (child) => child.type === 'leaf' && child.panelId === targetId,
  )

  if (targetIndex !== -1) {
    const newLeaf: LeafNode = { type: 'leaf', panelId }

    if (position === 'before' || position === 'after') {
      const insertAt = position === 'before' ? targetIndex : targetIndex + 1
      const children = [...node.children]
      children.splice(insertAt, 0, newLeaf)
      return { ...node, children }
    }

    // above/below — wrap target + new leaf in a vertical group
    const target = node.children[targetIndex]
    const vertChildren = position === 'above' ? [newLeaf, target] : [target, newLeaf]
    const vertGroup: GroupNode = { type: 'group', direction: 'vertical', children: vertChildren }
    const children = [...node.children]
    children[targetIndex] = vertGroup
    return { ...node, children }
  }

  // Target not at this level — recurse into child groups
  // Also handle inserting into an existing vertical group that contains the target
  return {
    ...node,
    children: node.children.map((child) => {
      if (child.type === 'group') {
        // If this is a vertical group containing the target, and we're inserting above/below,
        // insert directly into this group rather than nesting deeper
        if (
          (position === 'above' || position === 'below') &&
          child.direction === 'vertical'
        ) {
          const innerIdx = child.children.findIndex(
            (c) => c.type === 'leaf' && c.panelId === targetId,
          )
          if (innerIdx !== -1) {
            const newLeaf: LeafNode = { type: 'leaf', panelId }
            const insertAt = position === 'above' ? innerIdx : innerIdx + 1
            const children = [...child.children]
            children.splice(insertAt, 0, newLeaf)
            return { ...child, children }
          }
        }
        // Similarly for before/after in horizontal groups
        if (
          (position === 'before' || position === 'after') &&
          child.direction === 'horizontal'
        ) {
          const innerIdx = child.children.findIndex(
            (c) => c.type === 'leaf' && c.panelId === targetId,
          )
          if (innerIdx !== -1) {
            const newLeaf: LeafNode = { type: 'leaf', panelId }
            const insertAt = position === 'before' ? innerIdx : innerIdx + 1
            const children = [...child.children]
            children.splice(insertAt, 0, newLeaf)
            return { ...child, children }
          }
        }
        return insertPanel(child, panelId, targetId, position)
      }
      return child
    }),
  }
}

/** Move an existing panel to a new position relative to a target. */
export function movePanel(
  node: LayoutNode,
  panelId: PanelId,
  targetId: PanelId,
  position: InsertPosition,
): LayoutNode {
  const removed = removePanel(node, panelId)
  return insertPanel(removed, panelId, targetId, position)
}

/** Serialize layout to JSON string. */
export function serializeLayout(node: LayoutNode): string {
  return JSON.stringify(node)
}

/** Deserialize layout from JSON string. Returns null if invalid. */
export function deserializeLayout(json: string): LayoutNode | null {
  try {
    const parsed = JSON.parse(json)
    if (isValidNode(parsed)) return parsed
    return null
  } catch {
    return null
  }
}

function isValidNode(value: unknown): value is LayoutNode {
  if (typeof value !== 'object' || value === null) return false
  const obj = value as Record<string, unknown>
  if (obj.type === 'leaf') {
    return typeof obj.panelId === 'string' && ALL_PANELS.includes(obj.panelId as PanelId)
  }
  if (obj.type === 'group') {
    return (
      (obj.direction === 'horizontal' || obj.direction === 'vertical') &&
      Array.isArray(obj.children) &&
      (obj.children as unknown[]).every(isValidNode)
    )
  }
  return false
}
```

**Step 4: Run tests**

```bash
cd ui_web && npm test
```

Expected: All new tests and existing tests pass.

**Step 5: Commit**

```bash
git add ui_web/src/layout.ts ui_web/src/layout.test.ts
git commit -m "feat(layout): add layout tree types and manipulation helpers"
```

---

### Task 3: useLayoutState hook with localStorage persistence

**Files:**
- Create: `ui_web/src/hooks/useLayoutState.ts`
- Create: `ui_web/src/hooks/useLayoutState.test.ts`

This hook manages the layout tree in React state and syncs it to localStorage.

**Step 1: Write the failing tests**

Create `ui_web/src/hooks/useLayoutState.test.ts`:

```ts
import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { useLayoutState } from './useLayoutState'
import type { PanelId } from '../layout'

const STORAGE_KEY = 'panel-layout'

beforeEach(() => {
  localStorage.clear()
})

describe('useLayoutState', () => {
  it('starts with default horizontal layout when no localStorage', () => {
    const { result } = renderHook(() => useLayoutState())
    expect(result.current.layout.type).toBe('group')
    expect(result.current.layout.direction).toBe('horizontal')
    expect(result.current.layout.children).toHaveLength(5)
  })

  it('restores layout from localStorage', () => {
    const stored = JSON.stringify({
      type: 'group',
      direction: 'horizontal',
      children: [
        { type: 'leaf', panelId: 'map' },
        { type: 'leaf', panelId: 'fleet' },
      ],
    })
    localStorage.setItem(STORAGE_KEY, stored)
    const { result } = renderHook(() => useLayoutState())
    expect(result.current.layout.children).toHaveLength(2)
  })

  it('persists layout changes to localStorage', () => {
    const { result } = renderHook(() => useLayoutState())
    act(() => {
      result.current.move('fleet', 'map', 'after')
    })
    const stored = localStorage.getItem(STORAGE_KEY)
    expect(stored).toBeTruthy()
    const parsed = JSON.parse(stored!)
    // fleet should now be second (after map)
    expect(parsed.children[1].panelId).toBe('fleet')
  })

  it('togglePanel removes a visible panel', () => {
    const { result } = renderHook(() => useLayoutState())
    act(() => {
      result.current.togglePanel('events')
    })
    const ids = result.current.visiblePanels
    expect(ids).not.toContain('events')
  })

  it('togglePanel adds back a hidden panel', () => {
    const { result } = renderHook(() => useLayoutState())
    act(() => {
      result.current.togglePanel('events') // hide
    })
    act(() => {
      result.current.togglePanel('events') // show
    })
    expect(result.current.visiblePanels).toContain('events')
  })

  it('does not remove last visible panel', () => {
    // Start with only one panel
    const stored = JSON.stringify({
      type: 'group',
      direction: 'horizontal',
      children: [{ type: 'leaf', panelId: 'map' }],
    })
    localStorage.setItem(STORAGE_KEY, stored)
    const { result } = renderHook(() => useLayoutState())
    act(() => {
      result.current.togglePanel('map')
    })
    expect(result.current.visiblePanels).toContain('map')
  })
})
```

**Step 2: Run tests to verify they fail**

```bash
cd ui_web && npm test
```

Expected: FAIL — `./useLayoutState` doesn't exist.

**Step 3: Implement useLayoutState.ts**

Create `ui_web/src/hooks/useLayoutState.ts`:

```ts
import { useCallback, useState } from 'react'
import {
  type GroupNode,
  type LayoutNode,
  type PanelId,
  ALL_PANELS,
  buildDefaultLayout,
  deserializeLayout,
  findPanelIds,
  insertPanel,
  movePanel,
  removePanel,
  serializeLayout,
} from '../layout'

const STORAGE_KEY = 'panel-layout'

function readLayout(): GroupNode {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored) {
      const parsed = deserializeLayout(stored)
      if (parsed && parsed.type === 'group') return parsed
    }
  } catch {
    // ignore
  }
  return buildDefaultLayout([...ALL_PANELS])
}

function persistLayout(layout: LayoutNode) {
  try {
    localStorage.setItem(STORAGE_KEY, serializeLayout(layout))
  } catch {
    // ignore
  }
}

type InsertPosition = 'before' | 'after' | 'above' | 'below'

export function useLayoutState() {
  const [layout, setLayout] = useState<GroupNode>(readLayout)

  const visiblePanels = findPanelIds(layout)

  const doMove = useCallback(
    (panelId: PanelId, targetId: PanelId, position: InsertPosition) => {
      setLayout((prev) => {
        const next = movePanel(prev, panelId, targetId, position) as GroupNode
        persistLayout(next)
        return next
      })
    },
    [],
  )

  const togglePanel = useCallback((panelId: PanelId) => {
    setLayout((prev) => {
      const currentIds = findPanelIds(prev)
      if (currentIds.includes(panelId)) {
        // Remove — but don't remove the last panel
        if (currentIds.length <= 1) return prev
        const next = removePanel(prev, panelId) as GroupNode
        persistLayout(next)
        return next
      } else {
        // Add — append to end of root children
        const next = insertPanel(prev, panelId, currentIds[currentIds.length - 1], 'after') as GroupNode
        persistLayout(next)
        return next
      }
    })
  }, [])

  const resetLayout = useCallback(() => {
    const next = buildDefaultLayout([...ALL_PANELS])
    persistLayout(next)
    setLayout(next)
  }, [])

  return { layout, visiblePanels, move: doMove, togglePanel, resetLayout }
}
```

**Step 4: Run tests**

```bash
cd ui_web && npm test
```

Expected: All tests pass.

**Step 5: Commit**

```bash
git add ui_web/src/hooks/useLayoutState.ts ui_web/src/hooks/useLayoutState.test.ts
git commit -m "feat(layout): add useLayoutState hook with localStorage persistence"
```

---

### Task 4: DraggableTab component

**Files:**
- Create: `ui_web/src/components/DraggableTab.tsx`
- Create: `ui_web/src/components/DraggableTab.test.tsx`

A small header tab component that serves as the drag handle for a panel. Uses `@dnd-kit/sortable` for drag behavior.

**Step 1: Write the failing tests**

Create `ui_web/src/components/DraggableTab.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { DraggableTab } from './DraggableTab'
import { DndContext } from '@dnd-kit/core'

// DraggableTab must be wrapped in DndContext to use useDraggable
function renderWithDnd(ui: React.ReactElement) {
  return render(<DndContext>{ui}</DndContext>)
}

describe('DraggableTab', () => {
  it('renders the panel label', () => {
    renderWithDnd(<DraggableTab panelId="events" />)
    expect(screen.getByText('Events')).toBeInTheDocument()
  })

  it('renders a drag handle with role and aria attributes', () => {
    renderWithDnd(<DraggableTab panelId="map" />)
    const tab = screen.getByText('Map').closest('[data-panel-tab]')
    expect(tab).toBeInTheDocument()
  })

  it('applies active styling when isDragging is true', () => {
    renderWithDnd(<DraggableTab panelId="fleet" isDragging />)
    const tab = screen.getByText('Fleet').closest('[data-panel-tab]')
    expect(tab?.className).toContain('opacity-50')
  })
})
```

**Step 2: Run tests to verify they fail**

```bash
cd ui_web && npm test
```

Expected: FAIL — module not found.

**Step 3: Implement DraggableTab.tsx**

Create `ui_web/src/components/DraggableTab.tsx`:

```tsx
import { useDraggable } from '@dnd-kit/core'
import { PANEL_LABELS, type PanelId } from '../layout'

interface Props {
  panelId: PanelId
  isDragging?: boolean
}

export function DraggableTab({ panelId, isDragging }: Props) {
  const { attributes, listeners, setNodeRef } = useDraggable({
    id: `tab-${panelId}`,
    data: { panelId },
  })

  return (
    <div
      ref={setNodeRef}
      {...listeners}
      {...attributes}
      data-panel-tab={panelId}
      className={`flex items-center text-[11px] uppercase tracking-widest text-label pb-1.5 border-b border-edge shrink-0 cursor-grab active:cursor-grabbing select-none ${
        isDragging ? 'opacity-50' : ''
      }`}
    >
      <span className="mr-1.5 text-[9px] text-muted">⠿</span>
      <span>{PANEL_LABELS[panelId]}</span>
    </div>
  )
}
```

**Step 4: Run tests**

```bash
cd ui_web && npm test
```

Expected: All tests pass.

**Step 5: Commit**

```bash
git add ui_web/src/components/DraggableTab.tsx ui_web/src/components/DraggableTab.test.tsx
git commit -m "feat(ui): add DraggableTab component with dnd-kit integration"
```

---

### Task 5: DropZoneOverlay component

**Files:**
- Create: `ui_web/src/components/DropZoneOverlay.tsx`
- Create: `ui_web/src/components/DropZoneOverlay.test.tsx`

This component renders four drop zone regions (top, bottom, left, right) over a panel. Each zone highlights when a dragged item hovers over it. The zones use `useDroppable` from `@dnd-kit/core` with IDs that encode the target panel and the drop position.

**Step 1: Write the failing tests**

Create `ui_web/src/components/DropZoneOverlay.test.tsx`:

```tsx
import { render } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { DropZoneOverlay } from './DropZoneOverlay'
import { DndContext } from '@dnd-kit/core'

function renderWithDnd(ui: React.ReactElement) {
  return render(<DndContext>{ui}</DndContext>)
}

describe('DropZoneOverlay', () => {
  it('renders four drop zones when active', () => {
    const { container } = renderWithDnd(
      <div style={{ position: 'relative', width: 200, height: 200 }}>
        <DropZoneOverlay panelId="map" active />
      </div>,
    )
    const zones = container.querySelectorAll('[data-drop-zone]')
    expect(zones).toHaveLength(4)
  })

  it('renders nothing when not active', () => {
    const { container } = renderWithDnd(
      <div style={{ position: 'relative', width: 200, height: 200 }}>
        <DropZoneOverlay panelId="map" active={false} />
      </div>,
    )
    const zones = container.querySelectorAll('[data-drop-zone]')
    expect(zones).toHaveLength(0)
  })

  it('encodes panel and position in drop zone IDs', () => {
    const { container } = renderWithDnd(
      <div style={{ position: 'relative', width: 200, height: 200 }}>
        <DropZoneOverlay panelId="fleet" active />
      </div>,
    )
    const ids = Array.from(container.querySelectorAll('[data-drop-zone]')).map(
      (el) => el.getAttribute('data-drop-zone'),
    )
    expect(ids.sort()).toEqual(['fleet:above', 'fleet:after', 'fleet:before', 'fleet:below'])
  })
})
```

**Step 2: Run tests to verify they fail**

```bash
cd ui_web && npm test
```

**Step 3: Implement DropZoneOverlay.tsx**

Create `ui_web/src/components/DropZoneOverlay.tsx`:

```tsx
import { useDroppable } from '@dnd-kit/core'
import type { PanelId } from '../layout'

interface Props {
  panelId: PanelId
  active: boolean
}

type DropPosition = 'before' | 'after' | 'above' | 'below'

function Zone({ panelId, position }: { panelId: PanelId; position: DropPosition }) {
  const dropId = `${panelId}:${position}`
  const { isOver, setNodeRef } = useDroppable({
    id: dropId,
    data: { targetPanelId: panelId, position },
  })

  const positionClasses: Record<DropPosition, string> = {
    before: 'left-0 top-0 w-1/4 h-full',
    after: 'right-0 top-0 w-1/4 h-full',
    above: 'top-0 left-1/4 w-1/2 h-1/2',
    below: 'bottom-0 left-1/4 w-1/2 h-1/2',
  }

  return (
    <div
      ref={setNodeRef}
      data-drop-zone={dropId}
      className={`absolute ${positionClasses[position]} transition-colors z-50 ${
        isOver ? 'bg-accent/20 border-2 border-accent/50' : ''
      }`}
    />
  )
}

export function DropZoneOverlay({ panelId, active }: Props) {
  if (!active) return null

  return (
    <>
      <Zone panelId={panelId} position="before" />
      <Zone panelId={panelId} position="after" />
      <Zone panelId={panelId} position="above" />
      <Zone panelId={panelId} position="below" />
    </>
  )
}
```

**Step 4: Run tests**

```bash
cd ui_web && npm test
```

Expected: All tests pass.

**Step 5: Commit**

```bash
git add ui_web/src/components/DropZoneOverlay.tsx ui_web/src/components/DropZoneOverlay.test.tsx
git commit -m "feat(ui): add DropZoneOverlay with four directional drop zones"
```

---

### Task 6: LayoutRenderer — recursive component

**Files:**
- Create: `ui_web/src/components/LayoutRenderer.tsx`
- Create: `ui_web/src/components/LayoutRenderer.test.tsx`

This component recursively walks the `LayoutNode` tree and renders `PanelGroup`/`Panel`/`PanelResizeHandle` with `DraggableTab` headers and `DropZoneOverlay`. It receives a `renderPanel` callback to render the actual panel content.

**Step 1: Write the failing tests**

Create `ui_web/src/components/LayoutRenderer.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { LayoutRenderer } from './LayoutRenderer'
import { DndContext } from '@dnd-kit/core'
import type { GroupNode, PanelId } from '../layout'

function renderWithDnd(ui: React.ReactElement) {
  return render(<DndContext>{ui}</DndContext>)
}

const mockRenderPanel = vi.fn((id: PanelId) => <div data-testid={`panel-${id}`}>{id}</div>)

describe('LayoutRenderer', () => {
  it('renders leaf panels with their tab headers', () => {
    const layout: GroupNode = {
      type: 'group',
      direction: 'horizontal',
      children: [
        { type: 'leaf', panelId: 'map' },
        { type: 'leaf', panelId: 'events' },
      ],
    }
    renderWithDnd(
      <LayoutRenderer layout={layout} renderPanel={mockRenderPanel} isDragging={false} activeDragId={null} />,
    )
    expect(screen.getByText('Map')).toBeInTheDocument()
    expect(screen.getByText('Events')).toBeInTheDocument()
  })

  it('renders nested vertical groups', () => {
    const layout: GroupNode = {
      type: 'group',
      direction: 'horizontal',
      children: [
        {
          type: 'group',
          direction: 'vertical',
          children: [
            { type: 'leaf', panelId: 'map' },
            { type: 'leaf', panelId: 'events' },
          ],
        },
        { type: 'leaf', panelId: 'fleet' },
      ],
    }
    renderWithDnd(
      <LayoutRenderer layout={layout} renderPanel={mockRenderPanel} isDragging={false} activeDragId={null} />,
    )
    expect(screen.getByText('Map')).toBeInTheDocument()
    expect(screen.getByText('Events')).toBeInTheDocument()
    expect(screen.getByText('Fleet')).toBeInTheDocument()
  })

  it('renders resize handles between panels', () => {
    const layout: GroupNode = {
      type: 'group',
      direction: 'horizontal',
      children: [
        { type: 'leaf', panelId: 'map' },
        { type: 'leaf', panelId: 'events' },
        { type: 'leaf', panelId: 'fleet' },
      ],
    }
    const { container } = renderWithDnd(
      <LayoutRenderer layout={layout} renderPanel={mockRenderPanel} isDragging={false} activeDragId={null} />,
    )
    const handles = container.querySelectorAll('[data-panel-resize-handle-id]')
    expect(handles).toHaveLength(2) // 3 panels = 2 handles
  })

  it('calls renderPanel for each leaf', () => {
    const layout: GroupNode = {
      type: 'group',
      direction: 'horizontal',
      children: [
        { type: 'leaf', panelId: 'map' },
        { type: 'leaf', panelId: 'fleet' },
      ],
    }
    mockRenderPanel.mockClear()
    renderWithDnd(
      <LayoutRenderer layout={layout} renderPanel={mockRenderPanel} isDragging={false} activeDragId={null} />,
    )
    expect(mockRenderPanel).toHaveBeenCalledWith('map')
    expect(mockRenderPanel).toHaveBeenCalledWith('fleet')
  })
})
```

**Step 2: Run tests to verify they fail**

```bash
cd ui_web && npm test
```

**Step 3: Implement LayoutRenderer.tsx**

Create `ui_web/src/components/LayoutRenderer.tsx`:

```tsx
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels'
import { DraggableTab } from './DraggableTab'
import { DropZoneOverlay } from './DropZoneOverlay'
import type { GroupNode, LayoutNode, PanelId } from '../layout'

interface Props {
  layout: GroupNode
  renderPanel: (id: PanelId) => React.ReactNode
  isDragging: boolean
  activeDragId: PanelId | null
}

function RenderNode({
  node,
  renderPanel,
  isDragging,
  activeDragId,
}: {
  node: LayoutNode
  renderPanel: (id: PanelId) => React.ReactNode
  isDragging: boolean
  activeDragId: PanelId | null
}) {
  if (node.type === 'leaf') {
    return (
      <section className="relative flex flex-col h-full overflow-hidden bg-void p-3">
        <DraggableTab panelId={node.panelId} isDragging={activeDragId === node.panelId} />
        <div className="flex-1 overflow-hidden mt-2">{renderPanel(node.panelId)}</div>
        <DropZoneOverlay panelId={node.panelId} active={isDragging && activeDragId !== node.panelId} />
      </section>
    )
  }

  // Nested group
  return (
    <RenderGroup
      group={node}
      renderPanel={renderPanel}
      isDragging={isDragging}
      activeDragId={activeDragId}
    />
  )
}

function RenderGroup({
  group,
  renderPanel,
  isDragging,
  activeDragId,
}: {
  group: GroupNode
  renderPanel: (id: PanelId) => React.ReactNode
  isDragging: boolean
  activeDragId: PanelId | null
}) {
  const isVertical = group.direction === 'vertical'
  const handleClass = isVertical
    ? 'h-px bg-edge hover:bg-dim cursor-row-resize transition-colors'
    : 'w-px bg-edge hover:bg-dim cursor-col-resize transition-colors'

  return (
    <PanelGroup direction={group.direction} className={isVertical ? 'h-full' : 'flex-1 overflow-hidden'}>
      {group.children.map((child, index) => {
        const key = child.type === 'leaf' ? child.panelId : `group-${index}`
        return (
          <div key={key} className="contents">
            {index > 0 && <PanelResizeHandle className={handleClass} />}
            <Panel defaultSize={100 / group.children.length} minSize={10}>
              <RenderNode
                node={child}
                renderPanel={renderPanel}
                isDragging={isDragging}
                activeDragId={activeDragId}
              />
            </Panel>
          </div>
        )
      })}
    </PanelGroup>
  )
}

export function LayoutRenderer({ layout, renderPanel, isDragging, activeDragId }: Props) {
  return (
    <RenderGroup
      group={layout}
      renderPanel={renderPanel}
      isDragging={isDragging}
      activeDragId={activeDragId}
    />
  )
}
```

**Step 4: Run tests**

```bash
cd ui_web && npm test
```

Expected: All tests pass.

**Step 5: Commit**

```bash
git add ui_web/src/components/LayoutRenderer.tsx ui_web/src/components/LayoutRenderer.test.tsx
git commit -m "feat(ui): add recursive LayoutRenderer with nested PanelGroups"
```

---

### Task 7: Wire everything into App.tsx

**Files:**
- Modify: `ui_web/src/App.tsx`
- Modify: `ui_web/src/App.test.tsx`

Replace the flat panel rendering with `DndContext` + `LayoutRenderer`. Remove the old `visiblePanels` array, `useVisiblePanels` hook, `PanelId` type (now in `layout.ts`), `PANEL_LABELS`, and `ALL_PANELS` — they've moved to `layout.ts`. Wire `useLayoutState` for state management. Handle `onDragStart`/`onDragEnd` to track active drag and execute moves.

**Step 1: Rewrite App.tsx**

Replace the full contents of `ui_web/src/App.tsx` with:

```tsx
import { useCallback, useEffect, useState } from 'react'
import { DndContext, DragOverlay, PointerSensor, useSensor, useSensors } from '@dnd-kit/core'
import type { DragEndEvent, DragStartEvent } from '@dnd-kit/core'
import { StatusBar } from './components/StatusBar'
import { LayoutRenderer } from './components/LayoutRenderer'
import { DraggableTab } from './components/DraggableTab'
import { SolarSystemMap } from './components/SolarSystemMap'
import { EventsFeed } from './components/EventsFeed'
import { AsteroidTable } from './components/AsteroidTable'
import { FleetPanel } from './components/FleetPanel'
import { ResearchPanel } from './components/ResearchPanel'
import { fetchMeta } from './api'
import { useAnimatedTick } from './hooks/useAnimatedTick'
import { useSimStream } from './hooks/useSimStream'
import { useLayoutState } from './hooks/useLayoutState'
import { ALL_PANELS, PANEL_LABELS, type PanelId } from './layout'

export default function App() {
  const { snapshot, events, connected, currentTick, activeAlerts, dismissedAlerts, dismissAlert } =
    useSimStream()
  const { layout, visiblePanels, move, togglePanel } = useLayoutState()
  const [activeDragId, setActiveDragId] = useState<PanelId | null>(null)

  const [ticksPerSec, setTicksPerSec] = useState(10)
  const { displayTick, measuredTickRate } = useAnimatedTick(currentTick, ticksPerSec)

  useEffect(() => {
    fetchMeta()
      .then((meta) => setTicksPerSec(meta.ticks_per_sec))
      .catch(() => {})
  }, [])

  // Require 8px movement before starting drag to avoid accidental drags
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 8 } }))

  const renderPanel = useCallback(
    (id: PanelId) => {
      switch (id) {
        case 'map':
          return <SolarSystemMap snapshot={snapshot} currentTick={displayTick} oreCompositions={{}} />
        case 'events':
          return <EventsFeed events={events} />
        case 'asteroids':
          return <AsteroidTable asteroids={snapshot?.asteroids ?? {}} />
        case 'fleet':
          return (
            <FleetPanel
              ships={snapshot?.ships ?? {}}
              stations={snapshot?.stations ?? {}}
              displayTick={displayTick}
            />
          )
        case 'research':
          return snapshot ? <ResearchPanel research={snapshot.research} /> : null
      }
    },
    [snapshot, events, displayTick],
  )

  function handleDragStart(event: DragStartEvent) {
    const panelId = event.active.data.current?.panelId as PanelId | undefined
    if (panelId) setActiveDragId(panelId)
  }

  function handleDragEnd(event: DragEndEvent) {
    setActiveDragId(null)
    const { active, over } = event
    if (!over) return

    const sourcePanelId = active.data.current?.panelId as PanelId | undefined
    const targetPanelId = over.data.current?.targetPanelId as PanelId | undefined
    const position = over.data.current?.position as 'before' | 'after' | 'above' | 'below' | undefined

    if (sourcePanelId && targetPanelId && position && sourcePanelId !== targetPanelId) {
      move(sourcePanelId, targetPanelId, position)
    }
  }

  return (
    <div className="flex flex-col h-screen overflow-hidden">
      <StatusBar
        tick={displayTick}
        connected={connected}
        measuredTickRate={measuredTickRate}
        alerts={activeAlerts}
        dismissedAlerts={dismissedAlerts}
        onDismissAlert={dismissAlert}
      />
      <div className="flex flex-1 overflow-hidden">
        <nav className="flex flex-col shrink-0 bg-surface border-r border-edge py-2 px-1 gap-0.5">
          {ALL_PANELS.map((id) => (
            <button
              key={id}
              type="button"
              onClick={() => togglePanel(id)}
              className={`text-[10px] uppercase tracking-widest px-2 py-1.5 rounded-sm transition-colors cursor-pointer text-left ${
                visiblePanels.includes(id)
                  ? 'text-active bg-edge/40'
                  : 'text-muted hover:text-dim hover:bg-edge/15'
              }`}
            >
              {PANEL_LABELS[id]}
            </button>
          ))}
        </nav>
        {visiblePanels.length > 0 && (
          <DndContext sensors={sensors} onDragStart={handleDragStart} onDragEnd={handleDragEnd}>
            <LayoutRenderer
              layout={layout}
              renderPanel={renderPanel}
              isDragging={activeDragId !== null}
              activeDragId={activeDragId}
            />
            <DragOverlay>
              {activeDragId ? (
                <div className="bg-surface border border-accent/50 rounded px-3 py-1 shadow-lg">
                  <span className="text-[11px] uppercase tracking-widest text-accent">
                    {PANEL_LABELS[activeDragId]}
                  </span>
                </div>
              ) : null}
            </DragOverlay>
          </DndContext>
        )}
      </div>
    </div>
  )
}
```

**Step 2: Update App.test.tsx**

The existing tests should mostly still work since the structure is similar. The key changes:
- Panel headings are now rendered by `DraggableTab` (still shows panel names)
- `PanelGroup`/`Panel` still present
- Nav buttons still toggle panels

Update `ui_web/src/App.test.tsx` — the test that checks for resize handles should still pass. The test that checks for heading counts (nav + panel heading = 2 per panel) should still work since `DraggableTab` renders the label text.

Run the tests first to see if they pass as-is:

```bash
cd ui_web && npm test
```

If any tests fail due to the layout change, update them to match the new structure. Likely adjustments:
- The `renders all five panel headings by default` test should still find 2 occurrences of each label (nav + tab header)
- The `hides panel when nav button clicked` test should still work

**Step 3: Run tests and fix any failures**

```bash
cd ui_web && npm test
```

Fix any test failures from the App.test.tsx changes.

**Step 4: Commit**

```bash
git add ui_web/src/App.tsx ui_web/src/App.test.tsx
git commit -m "feat(ui): wire DndContext + LayoutRenderer into App for draggable panels"
```

---

### Task 8: Visual polish — drag overlay animation and drop zone styling

**Files:**
- Modify: `ui_web/src/components/DropZoneOverlay.tsx`
- Modify: `ui_web/src/components/DraggableTab.tsx`
- Modify: `ui_web/src/index.css`

Add CSS transitions for drop zone highlighting, improve the drag overlay appearance, and add a subtle animation when panels rearrange.

**Step 1: Enhance drop zone visual feedback**

In `ui_web/src/components/DropZoneOverlay.tsx`, update the `Zone` component to show a more visible indicator. Add a label text when hovered:

Update the `Zone` return JSX to:

```tsx
  return (
    <div
      ref={setNodeRef}
      data-drop-zone={dropId}
      className={`absolute ${positionClasses[position]} transition-all duration-150 z-50 flex items-center justify-center ${
        isOver
          ? 'bg-accent/15 border-2 border-accent/40 rounded-sm'
          : 'hover:bg-edge/5'
      }`}
    >
      {isOver && (
        <span className="text-[9px] uppercase tracking-widest text-accent/70 pointer-events-none">
          {position}
        </span>
      )}
    </div>
  )
```

**Step 2: Add cursor style and grab indicator to DraggableTab**

In `ui_web/src/components/DraggableTab.tsx`, add a hover state and transition:

```tsx
      className={`flex items-center text-[11px] uppercase tracking-widest text-label pb-1.5 border-b border-edge shrink-0 cursor-grab active:cursor-grabbing select-none transition-opacity ${
        isDragging ? 'opacity-50' : 'hover:text-dim'
      }`}
```

**Step 3: Run tests**

```bash
cd ui_web && npm test
```

Expected: All tests pass (styling changes don't break behavior).

**Step 4: Commit**

```bash
git add ui_web/src/components/DropZoneOverlay.tsx ui_web/src/components/DraggableTab.tsx ui_web/src/index.css
git commit -m "feat(ui): polish drop zone indicators and drag overlay animations"
```

---

### Task 9: Manual integration testing and edge case fixes

**Files:**
- Possibly modify any of the above files

**Step 1: Start the dev server**

```bash
cd ui_web && npm run dev
```

Also start the daemon in another terminal:

```bash
cargo run -p sim_daemon -- run --seed 42
```

**Step 2: Test these scenarios manually**

1. **Drag reorder:** Drag the "Events" tab to the right of "Fleet" — panels should swap positions
2. **Vertical stack:** Drag "Events" onto the bottom edge of "Map" — they should stack vertically with a horizontal resize handle between them
3. **Undo stack:** Drag "Events" off the vertical stack onto the right edge of "Fleet" — it should leave the stack and become a standalone panel
4. **Toggle visibility:** Click "Map" in the sidebar to hide it, then click again to show it — it should reappear at the end
5. **Persist:** Rearrange panels, refresh the page — layout should be preserved
6. **Resize:** Drag resize handles between panels — should still work in both horizontal and vertical directions
7. **Single panel:** Hide all panels except one — verify it fills the space, drag zones don't appear

**Step 3: Fix any issues found during manual testing**

Common issues to watch for:
- Drop zones not appearing (check `isDragging` state flows correctly)
- Panel sizes resetting after drag (may need to set `autoSaveId` on `PanelGroup`)
- Vertical stacks getting unnecessarily nested (collapse single-child groups)
- Animation jank (adjust `PointerSensor` distance threshold)

**Step 4: Commit fixes**

```bash
git add -A
git commit -m "fix(ui): edge case fixes from integration testing"
```

---

## Summary

| Task | What | New Files |
|------|------|-----------|
| 1 | Install @dnd-kit | — |
| 2 | Layout tree types + helpers | `layout.ts`, `layout.test.ts` |
| 3 | useLayoutState hook | `useLayoutState.ts`, `useLayoutState.test.ts` |
| 4 | DraggableTab component | `DraggableTab.tsx`, `DraggableTab.test.tsx` |
| 5 | DropZoneOverlay component | `DropZoneOverlay.tsx`, `DropZoneOverlay.test.tsx` |
| 6 | LayoutRenderer component | `LayoutRenderer.tsx`, `LayoutRenderer.test.tsx` |
| 7 | Wire into App.tsx | — (modify existing) |
| 8 | Visual polish | — (modify existing) |
| 9 | Integration testing + fixes | — |
