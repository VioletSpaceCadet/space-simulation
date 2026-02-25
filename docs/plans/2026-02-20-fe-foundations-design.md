# FE Foundations: Sorting + Collapsible Panels

**Date:** 2026-02-20
**Status:** Approved
**Scope:** Shared FE component infrastructure only — no content/data changes

## Goals

Add foundational interactivity to the mission control UI:
1. Column sorting in tables (starting with AsteroidTable)
2. Collapsible/expandable panels with persistent state
3. Hover-highlighted panel headers as a foundational UX pattern

## Constraints

- Another session is making significant backend/content changes — keep changes isolated to shared FE components
- No new npm dependencies
- Use react-resizable-panels v2 native `collapsible`/`collapsedSize` props

## New Files

### `ui_web/src/hooks/useSortableData.ts`

Generic sorting hook:
```ts
type SortDirection = 'asc' | 'desc' | null;
type SortConfig<T> = { key: keyof T; direction: SortDirection };

function useSortableData<T>(data: T[]): {
  sortedData: T[];
  sortConfig: SortConfig<T> | null;
  requestSort: (key: keyof T) => void;
}
```

- Click toggles: null → asc → desc → null
- Handles string and number comparison
- Returns original order when direction is null

### `ui_web/src/components/PanelHeader.tsx`

Shared clickable header:
- Props: `title`, `collapsed`, `onToggle`
- Visual: panel name + chevron indicator (`▸` collapsed, `▾` expanded)
- Hover: `hover:bg-edge transition-colors cursor-pointer`
- Compact: single-line, no padding bloat

## Changes to Existing Files

### `App.tsx`

- Add `useState` per panel for collapsed state, initialized from `localStorage`
- Write to `localStorage` on toggle (keys: `panel:{name}:collapsed`)
- Pass `collapsible` + `collapsedSize` + `onCollapse`/`onExpand` to `<Panel>` components
- Pass `collapsed` + `onToggle` to each panel component

### `AsteroidTable.tsx`

- Wire `useSortableData` with asteroid array
- Clickable `<th>` headers with sort indicators (▲ asc, ▼ desc, dim ⇅ unsorted)
- Sortable columns: ID, Node, Mass, primary composition fraction
- Wrap content in `<PanelHeader>` at top; hide table body when collapsed

### `EventsFeed.tsx`, `FleetPanel.tsx`, `ResearchPanel.tsx`

- Add `<PanelHeader>` at top
- Accept `collapsed`/`onToggle` props
- When collapsed, render only the header

## Sorting Behavior

- Cycle: unsorted → ascending → descending → unsorted
- Sort indicators: `▲` asc, `▼` desc, dim `⇅` unsorted
- Numeric sort for Mass; string sort for ID, Node; custom sort for composition (by highest fraction)

## Collapse Behavior

- Click header → collapse to slim bar (title + ▸)
- Click again → expand
- Uses react-resizable-panels native `collapsible` + `collapsedSize` props
- State persisted to `localStorage` per panel

## Testing

- Unit test for `useSortableData` hook (sort cycling, numeric/string sorting)
- Update existing component tests to account for PanelHeader
