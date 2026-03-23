---
title: "Canvas+React Integration Patterns — Solar System Map Redesign"
category: integration-issues
date: 2026-03-22
tags: [canvas, react-19, css-variables, passive-events, jsdom, coverage, lod, theme-centralization, drag-guard]
severity: medium
components: [ui_web, SolarSystemMapCanvas, config/theme.ts]
related_issues: [VIO-388, VIO-389, VIO-390, VIO-391, VIO-392, VIO-393]
---

# Canvas+React Integration Patterns

Learnings from the Solar System Map Redesign project (6 tickets, PRs #193-#200), which replaced the SVG-based `SolarSystemMap.tsx` with a canvas+DOM overlay architecture.

## 1. CSS Variables Don't Work in Canvas 2D

**Problem:** Ships rendered with wrong colors. `SHIP_TASK_COLORS.Transit` was `'var(--color-accent)'` — Canvas `ctx.fillStyle` silently ignores CSS custom properties.

**Root Cause:** The HTML Canvas 2D rendering context has no connection to the CSSOM. When you assign `ctx.fillStyle = 'var(--color-accent)'`, the canvas API does not recognize CSS custom property syntax. It falls back to the previous fill color or transparent black.

**Fix:** Replace CSS variable references with resolved hex values. Added `IDLE_COLOR` and `MAP_COLORS` constants to `config/theme.ts`. Works in both CSS and canvas contexts.

```typescript
// BEFORE — broken in canvas context
const SHIP_TASK_COLORS = { Transit: 'var(--color-accent)' };

// AFTER — works in both canvas and DOM
const IDLE_COLOR = '#8a8e98';
const SHIP_TASK_COLORS = { Transit: '#5ca0c8' };
```

**Rule:** Any color used as `ctx.fillStyle`/`ctx.strokeStyle` must be a resolved hex/rgb value, never a CSS variable.

## 2. React 19 Passive Wheel Events

**Problem:** Page scrolled instead of map zooming. `e.preventDefault()` inside React's `onWheel` was a no-op.

**Root Cause:** React 19 registers wheel listeners as passive by default. Passive listeners cannot call `preventDefault()`.

**Fix:** Remove `onWheel` JSX prop. Use native `addEventListener('wheel', handler, { passive: false })` in a `useEffect`.

```typescript
// BEFORE — React 19 makes this passive, preventDefault is no-op
<div onWheel={handleWheel} />

// AFTER — native listener with explicit passive: false
useEffect(() => {
  const el = containerRef.current;
  if (!el) return;
  const onWheel = (e: WheelEvent) => {
    e.preventDefault();
    // ... zoom logic
  };
  el.addEventListener('wheel', onWheel, { passive: false });
  return () => el.removeEventListener('wheel', onWheel);
}, []);
```

**Rule:** For wheel/touch events that need `preventDefault()` in React 19, always use native `addEventListener` with `{ passive: false }`.

## 3. Click Fires After Drag

**Problem:** Entity selection triggered during pan gestures. Browser fires `click` after `mousedown`/`mouseup` regardless of mouse movement.

**Fix:** Track drag start position in a ref. Set `didDragRef = true` when cumulative movement exceeds `DRAG_THRESHOLD` (4px). Suppress click handler when flag is set.

```typescript
const DRAG_THRESHOLD = 4;
const dragStartRef = useRef({ x: 0, y: 0 });
const didDragRef = useRef(false);

const onMouseDown = (e) => {
  dragStartRef.current = { x: e.clientX, y: e.clientY };
  didDragRef.current = false;
};
const onMouseMove = (e) => {
  const dx = e.clientX - dragStartRef.current.x;
  const dy = e.clientY - dragStartRef.current.y;
  if (Math.hypot(dx, dy) > DRAG_THRESHOLD) didDragRef.current = true;
};
const handleClick = () => {
  if (didDragRef.current) return; // was a drag, not a click
};
```

**Rule:** Any canvas with both click-to-select and drag-to-pan must implement a drag threshold guard.

## 4. Ref Access During Render

**Problem:** ESLint error for `draggingRef.current` in JSX style prop. React concurrent mode makes ref reads during render unreliable.

**Fix:** Mirror the ref to state (`[dragging, setDragging]`). Ref for high-frequency event handlers (no re-render), state for rendered output (cursor style).

```typescript
const draggingRef = useRef(false);
const [dragging, setDragging] = useState(false);

const onMouseDown = () => {
  draggingRef.current = true;
  setDragging(true);        // triggers re-render for cursor
};
const onMouseMove = () => {
  if (!draggingRef.current) return; // ref read in handler — OK
};
// JSX reads state, not ref
<div style={{ cursor: dragging ? 'grabbing' : 'grab' }} />
```

**Rule:** Refs for event handlers, state for render output. The "ref + state mirror" pattern is standard for drag interactions.

## 5. jsdom Lacks Canvas/ResizeObserver

**Problem:** Tests crashed — `getContext('2d')` returns null, `ResizeObserver` undefined in jsdom.

**Fix:** Guard `getContext('2d')` with null check (return empty string). Add `ResizeObserver` stub polyfill to `test-setup.ts`.

```typescript
// starfield.ts — guard against null context
const ctx = canvas.getContext('2d');
if (!ctx) return '';  // jsdom — no canvas support

// test-setup.ts — ResizeObserver stub
if (typeof globalThis.ResizeObserver === 'undefined') {
  globalThis.ResizeObserver = class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
}
```

**Rule:** Canvas code must always null-check `getContext()`. Add stubs for missing browser APIs in test setup.

## 6. Coverage Threshold Drop

**Problem:** ~400 lines of Canvas 2D draw calls dropped statement coverage below 61% threshold.

**Fix:** Exclude `renderer.ts` and `starfield.ts` from coverage with explanatory comment. These are tested visually via Chrome agent.

```typescript
// vite.config.ts
coverage: {
  exclude: [
    // Canvas 2D draw-call modules — untestable in jsdom.
    // Tested via Chrome agent visual inspection.
    'src/components/solar-system/canvas/renderer.ts',
    'src/components/solar-system/canvas/starfield.ts',
  ],
}
```

**Rule:** Exclude pure draw-call files from jsdom coverage. Test rendering via browser automation, not unit tests.

## 7. LOD Hit-Test vs Render Threshold Mismatch

**Problem:** Entities clickable at zoom levels where they were invisible. Hit-test `smoothStep` fadeIn values (0.2/0.25) were lower than renderer's (0.25/0.3).

**Fix:** Align hit-test thresholds to exactly match renderer values. Share constants between both systems.

**Rule:** Visibility and hit-test thresholds must share constants — duplicated magic numbers guarantee "invisible but clickable" bugs.

## 8. Centralized Map Colors

**Problem:** Hardcoded hex color literals throughout canvas renderer, bypassing `config/theme.ts`.

**Fix:** Created `MAP_COLORS` constant in `theme.ts`. Replaced all inline literals in renderer. Satisfies PR review checklist item 11.

**Rule:** Canvas colors go in the theme — no inline hex for game concepts.

---

## Canvas+React Checklist

Use this when building or reviewing canvas-based React components.

### Colors
- [ ] All game-concept colors from `config/theme.ts` as resolved hex
- [ ] No `var(--` in canvas code
- [ ] CI grep catches new hex literals in canvas files

### Events
- [ ] Wheel/zoom via native `addEventListener({ passive: false })`
- [ ] Drag guard (threshold-based click suppression)
- [ ] Coordinate transform is a shared utility

### React Integration
- [ ] Refs read only in effects/handlers, never during render
- [ ] High-frequency refs mirrored to state only for rendered output
- [ ] `requestAnimationFrame` canceled in effect cleanup
- [ ] rAF loop reads refs, never calls setState

### Testing
- [ ] `getContext('2d')` null-checked for jsdom
- [ ] ResizeObserver polyfill in test-setup.ts
- [ ] Logic layer (transforms, hit-test, zoom math) has unit tests
- [ ] Draw-call files excluded from coverage with comment
- [ ] Visual correctness via Chrome agent

### Visibility
- [ ] Single source of truth for LOD thresholds
- [ ] Hit-test checks same visibility predicate as renderer

---

## SVG-to-Canvas Migration Checklist

1. Audit all CSS variable references — replace with resolved hex values
2. Move all color literals to a centralized theme constant (e.g., `MAP_COLORS`)
3. Switch wheel/touch handlers from React synthetic events to native `addEventListener`
4. Implement drag-vs-click discrimination with a pixel threshold
5. Mirror high-frequency refs to state only when they affect rendered output
6. Guard all `getContext('2d')` calls with null checks for jsdom
7. Add ResizeObserver polyfill stub to test setup
8. Exclude pure draw-call files from coverage; test visually via browser automation
9. Align hit-test thresholds with render thresholds — share constants
