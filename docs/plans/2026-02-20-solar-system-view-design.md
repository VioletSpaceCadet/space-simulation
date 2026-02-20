# Solar System View — Design

**Date:** 2026-02-20
**Status:** Approved

## Overview

Add a toggleable solar system visualization to the React UI. Stylized orbital map rendered as SVG with d3-zoom for pan/zoom, showing all game entities at their graph nodes with transit animation.

## View Toggle

- State in `App.tsx`: `view: 'dashboard' | 'map'`
- Toggle button in `StatusBar` (top-left area)
- `dashboard` renders existing `PanelGroup`; `map` renders `<SolarSystemMap>` full-bleed
- `StatusBar` remains visible in both views

## Orbital Map Layout

SVG coordinate system centered at (0, 0).

| Node | Ring radius | Style |
|------|-------------|-------|
| Earth Orbit | 100 | Thin solid ring |
| Inner Belt | 200 | Dashed ring, subtle fill band |
| Mid Belt | 300 | Dashed ring, subtle fill band |
| Outer Belt | 400 | Dashed ring, subtle fill band |

- Decorative sun glyph at center (no game entity)
- Node labels placed along their ring
- Ring radii are hardcoded for the current 4-node graph; can be computed later

## Entity Placement

### Angular position
- **Stations:** Fixed angle (0° for the first station at a node)
- **Asteroids:** Angle derived from hash of asteroid ID → deterministic spread around ring
- **Ships (idle/working):** At their `location_node` ring, offset slightly from station/asteroids
- **Scan sites:** Faint markers at their node ring, angle from site ID hash

### Transit animation
- Ships with `Transit` task: origin = `ship.location_node` (stays at departure during transit), destination = `task.kind.Transit.destination`
- Progress: `(currentTick - task.started_tick) / (task.eta_tick - task.started_tick)`, clamped [0, 1]
- Position: lerp between origin ring/angle and destination ring/angle (polar interpolation)

## Entity Rendering (SVG)

| Entity | Shape | Color | Size |
|--------|-------|-------|------|
| Station | Diamond/square | `--color-accent` | Fixed |
| Ship | Triangle/arrow | Task-dependent (idle=dim, survey=blue, mining=amber, transit=accent) | Fixed |
| Asteroid | Circle | Tag-dependent (IronRich=reddish) | log(mass_kg), clamped |
| Scan site | Faint dot / `?` | `--color-faint` | Small fixed |

## Interactivity

### Zoom & Pan (d3-zoom)
- `d3-zoom` applied to root `<g>` inside SVG
- Mouse wheel = zoom, drag = pan
- Smooth transitions via d3
- Dependencies: `d3-zoom`, `d3-selection` (~30KB)

### Hover tooltips
- HTML `<div>` overlaid on SVG, positioned via SVG→screen coordinate transform
- Ship: ID, location, current task, cargo summary
- Station: ID, location, cargo summary
- Asteroid: ID, mass, tags, composition (if known)
- Scan site: ID, template

### Click to select
- Clicking an entity outlines/highlights it and shows a detail card
- Detail card: HTML overlay with expanded info (reuses data from Fleet/Asteroid panels)
- Clicking empty space deselects

## Component Structure

```
App.tsx
├── StatusBar (+ view toggle button)
├── if 'dashboard': PanelGroup (existing, unchanged)
└── if 'map': SolarSystemMap
    ├── <svg> with d3-zoom on root <g>
    │   ├── OrbitalRings
    │   ├── NodeLabels
    │   ├── ScanSiteMarkers
    │   ├── AsteroidMarkers
    │   ├── StationMarkers
    │   └── ShipMarkers
    ├── Tooltip (HTML overlay)
    └── DetailCard (HTML overlay, selected entity)
```

## Props

`SolarSystemMap` receives from `App`:
- `snapshot: SimSnapshot | null`
- `currentTick: number`
- `oreCompositions: OreCompositions`

## New Files

- `ui_web/src/components/SolarSystemMap.tsx` — main map component
- `ui_web/src/components/solar-system/` — sub-components (rings, entities, tooltip) if needed
- `ui_web/src/hooks/useSvgZoomPan.ts` — d3-zoom React hook

## Dependencies

- `d3-zoom` + `d3-selection` (npm)
- No other new dependencies

## Non-Goals (for initial version)

- Real spatial physics or orbital mechanics
- Editable node positions
- Route planning or command input from the map
- 3D rendering
