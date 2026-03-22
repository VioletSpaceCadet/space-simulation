# Solar System Map Redesign — Design Document

## Overview

Interactive canvas-based map replacing the current SVG-based `SolarSystemMap.tsx`. Supports seamless zoom from planet surfaces to interstellar distances, LOD-based decluttering, rich tooltips, keyboard navigation, and multi-star-system rendering.

**Mockup location:** `http://localhost:8090/solar-map-mockup.html` (served from `/tmp/solar-map-mockup.html`)

---

## Technology Recommendation

### Rendering: Canvas + DOM Overlay (no new dependencies)

| Approach | Pros | Cons | Verdict |
|----------|------|------|---------|
| **SVG + d3-zoom (current)** | Simple, crisp text, existing code | Slow at 100+ entities, linear zoom only, no log projection | Not viable for multi-system |
| **Canvas only** | Full control, performant, no deps | Text rendering less crisp, hit detection is manual | Good but text suffers |
| **Canvas + DOM overlay** | Canvas for rendering, DOM for crisp labels/tooltips/HUD. Best of both. | Slightly more complex architecture | **Recommended** |
| **PixiJS** | WebGL-accelerated 2D, handles thousands of sprites, built-in hit detection | New dep (~200KB), learning curve, overkill until 1000+ entities | Future upgrade path |
| **Three.js** | Could enable 3D orbital view later, WebGL | Massive overkill for 2D, huge bundle | Not recommended |

**Decision: Canvas + DOM overlay.**
- **Canvas:** Bodies, zones, orbits, entity markers, transit lines, starfield tile
- **DOM:** Tooltips, entity name labels (crisp text), nav HUD, minimap, detail cards, breadcrumbs
- **Dependencies removed:** `d3-selection`, `d3-zoom` (no longer needed for map)
- **Dependencies added:** None
- **Upgrade path:** If we hit performance issues at 500+ entities, PixiJS is a drop-in replacement for the canvas layer — the DOM overlay and game logic remain unchanged

### Risk: Log Projection Seam

The transition from linear to logarithmic coordinate space creates a blend zone where positions are interpolated between two projection modes. This works visually for display, but needs careful testing for:
- **Hit detection accuracy** — clicking an entity at the seam must resolve to the correct world coordinate
- **Command targeting** — issuing a "transit to" command near the projection boundary must use real linear coordinates, not log-projected ones
- **Mitigation:** Always use linear world coordinates for game logic (commands, distances, ETAs). Log projection is view-only — applied in `worldToScreen()`, never stored or sent to the daemon.

---

## Architecture Decisions

### Canvas vs SVG
- **Current:** SVG with d3-zoom (`useSvgZoomPan` hook)
- **Proposed:** Canvas + DOM overlay (see Technology Recommendation above)
- **Why:** SVG DOM manipulation becomes expensive with 100+ entities. Canvas draws everything in a single pass. DOM overlay provides crisp text for labels and tooltips. Starfield is a CSS background (zero per-frame cost).
- **Keep SVG for:** Nothing. Full canvas + DOM approach.

### Coordinate System
- **Matches existing game model:** `Position { parent_body, radius_au_um, angle_mdeg }` → converted to absolute `{ x_au_um, y_au_um }` via `entityAbsolute()` using `body_absolutes` from the daemon
- **Canvas world units:** Direct mapping from `body_absolutes` values
- **No changes needed to sim_core or sim_daemon** — the map purely consumes existing `body_absolutes` + entity positions

### Logarithmic Projection (Interstellar Zoom)
- At zoom levels below a threshold (`LOG_THRESHOLD`), distances from camera center are log-compressed using `log1p(x * scale)`
- Enables smooth transition from 1 AU scale to 268,550 AU (Proxima Centauri) without infinite panning
- Camera zoom interpolation uses **log-space lerp** (`Math.exp(logCurrent + (logTarget - logCurrent) * lerp)`) so zooming from 10x to 0.0001x feels smooth and constant-rate

### Starfield Background
- **Parallax tiling:** A 1024x1024 star tile is generated once on a hidden canvas (deterministic seeded random, 3 density layers with warm/cool tinting), converted to `dataURL`, set as CSS `background-image` with `background-repeat`
- Each frame: `background-position` shifts by `camera * PARALLAX_FACTOR` (5% of camera speed)
- Zero per-frame rendering cost — pure CSS transform

---

## LOD System (Level of Detail)

Four tiers based on camera zoom:

| Tier | Zoom Range | Visible Elements |
|------|-----------|-----------------|
| **INTERSTELLAR** | < 0.001 | Star system markers only (Sol, Alpha Centauri). Individual stars/planets hidden. Distance line between systems. |
| **SYSTEM** | 0.001 - 0.15 | Stars, planets, orbit rings, zone arcs. No entity labels, ships as dots. |
| **REGION** | 0.15 - 0.8 | + Ship triangles, station diamonds, asteroids, scan sites. Transit lines with progress. Zone labels. |
| **LOCAL** | > 0.8 | + Entity name labels, ship task labels, transit ETA labels, full tooltips. |

All transitions use `smoothStep()` for opacity fading — no pop-in/pop-out.

### Star System Collapse
At interstellar zoom, individual stars (Rigil Kentaurus, Toliman, Proxima) collapse into a single "Alpha Centauri" system marker with glow + subtitle. Expanding to individual stars happens as you zoom into the system.

---

## Navigation System

### Quick-Nav Panel (top-right HUD)
- **Context-aware:** Shows different waypoints based on current camera position and zoom level
  - **Interstellar zoom:** Star Systems (Sol, Alpha Centauri, Proxima Centauri)
  - **Sol system zoom:** Sol System, Earth, Mars, Main Belt, Jupiter
  - **Proxima system zoom:** Proxima System, Alpha Centauri
- Each button shows a **number key badge** (1, 2, 3...) for keyboard shortcut
- **Shift held:** Panel swaps in-place to show Stations (filtered to current system)
- **Control/Cmd held:** Panel swaps to show Ships (filtered to current system)
- **Released:** Reverts to location waypoints
- At interstellar zoom, Shift/Control modifiers are disabled (no local entities to show)

### Keyboard Shortcuts
- `1-9`: Fly to location waypoint (context-dependent)
- `Shift + 1-9` (while holding): Fly to station by index
- `Ctrl/Cmd + 1-9` (while holding): Fly to ship by index

### Fly-To Animation
- Sets `targetCamera` position and zoom
- Camera lerps in log-space (zoom) and linear (position) each frame
- Position lerp uses adaptive speed — faster for interstellar jumps

### Double-Click
- Only responds on bodies and stations (not empty space)
- Zooms in 2x closer, centering on the entity
- Updates breadcrumb

### Minimap (bottom-right)
- 140x140 canvas showing full system overview
- **Clickable + draggable:** Click/drag repositions the camera
- Viewport rectangle shows current view bounds
- Uses world-to-minimap coordinate transform

### Breadcrumb (top-left)
- Shows "System > Earth" navigation path
- "System" is clickable to zoom back out
- Updates on fly-to navigation

---

## Entity Rendering

### Size Caps
All entity markers have capped screen sizes to prevent oversized icons at close zoom:

| Entity | Min Size | Max Size | Scale Factor |
|--------|---------|---------|-------------|
| Star | 5px | 14px | `radius * zoom * 0.5` |
| Planet | 2px | 8px | `radius * zoom * 0.5` |
| Moon | 2px | 4px | `radius * zoom * 0.5` |
| Station | 3px | 7px | `4 * zoom * 0.6` |
| Ship | 3px | 6px | `3.5 * zoom * 0.5` |
| Asteroid | 2px | 5px | `log10(mass) * zoom * 0.35` |
| Scan Site | 3px | 5px | `3.5 * zoom * 0.4` |

### Ship Task Colors
Ships are color-coded by task (from `config/theme.ts`):
- Survey: `#5b9bd5` (blue)
- Mine: `#d4a44c` (gold)
- Deposit: `#4caf7d` (green)
- Transit: `#5ca0c8` (cyan)
- DeepScan: `#7b68ee` (purple)

### Transit Lines
- **Dashed line** from ship to destination (dim)
- **Solid line** from ship origin to current position (brighter)
- **Progress pip** — small glowing dot at interpolated position
- **ETA label** — "45% · 2d 12h" at local zoom
- **Destination marker** — small hollow circle

### Label Collision
- Body labels (Earth, Luna) hide when a station is within 40px screen distance at close zoom
- Moon labels offset to the right to avoid parent planet
- Station labels positioned to the right of the diamond marker

### Asteroids
- Rendered as irregular 6-sided polygons (not circles) with seeded wobble per-asteroid
- Color from anomaly tags via `TAG_COLORS`

---

## Tooltips

Rich HTML tooltips with `backdrop-filter: blur(12px)`, positioned at cursor with arrow pointer.

### Station Tooltip
- Name, type badge
- Orbit (parent body)
- Power surplus/deficit (green/red)
- Cargo: `used / capacity m³`
- Modules: `active / total`
- Ore breakdown (Fe, Si, etc.) in separated section

### Ship Tooltip
- Name, type badge
- Task (color-coded)
- Location (parent body or "transit")
- For Transit: Destination, progress bar, ETA
- For Mine: Target asteroid, progress bar
- For Survey: Target site, progress bar
- Cargo: `used kg / capacity m³`

### Asteroid Tooltip
- Name, type badge
- Location (parent body)
- Mass in kg
- Anomaly tags (colored badges)
- Tag confidence percentages
- Composition breakdown (if known): "Fe 72% · Si 18% · Ni 10%"

### Scan Site Tooltip
- Name, type badge
- Location, template ID

### Body Tooltip (Planet/Star/Moon)
- Name, type badge only (type is already in subtitle)

---

## Multi-Star System Support

### Alpha Centauri System Data
- **Rigil Kentaurus** (Alpha Centauri A) — G2V yellow star
- **Toliman** (Alpha Centauri B) — K1V orange star, orbits A at ~23 AU
- **Proxima Centauri** (Alpha Centauri C) — M5.5V red dwarf, ~13,000 AU from A/B pair
  - **Proxima b** — 1.17 Earth masses, 0.049 AU, habitable zone
  - **Proxima d** — 0.26 Earth masses, 0.029 AU

### Orbit Rings
- Sol system: Earth orbit, Mars orbit, Jupiter orbit
- Alpha Centauri: Toliman orbit around Rigil (23 AU), Proxima orbit around A/B (13,000 AU)
- Proxima: Proxima b orbit (0.049 AU)

### Content Model Impact
Adding a new star system requires:
1. New entries in `solar_system.json` with `parent: null` (or a galaxy-level root)
2. New `body_absolutes` entries from daemon
3. Entities use existing `Position` type — `parent_body` references the new body IDs
4. Transit between systems would need new nav graph nodes/edges

---

## Camera & Controls

### Zoom
- **Scroll wheel:** Constant ratio per tick (0.82x out, 1.22x in) — ~5 ticks per order of magnitude
- **Zoom toward cursor:** Adjusts camera position so the point under the cursor stays fixed
- **Camera lerp:** Log-space interpolation for zoom, linear for position. Lerp rate 0.18 for zoom, 0.12 for position (adaptive for large jumps)

### Pan (Drag)
- Standard drag-to-pan
- **Speed scales with zoom:** `1 / camera.zoom` with extra boost below `LOG_THRESHOLD` using `pow(threshold/zoom, 0.4)` multiplier
- Prevents both "too fast at normal zoom" and "stuck at interstellar zoom"

### Zoom Sensitivity
- Previous issue: Scrolling from 100 AU to 10,000 AU happened in 2 ticks (overshooting)
- Fix: Constant multiplicative ratio means each tick is the same perceptual step regardless of zoom level
- The log-lerp camera smooths the visual transition

---

## Critical Design Constraint: Discovery-Driven Rendering

**The map must render based on discovered state from the backend, not static content.**

- The FE does NOT pre-know what star systems, bodies, or entities exist. Everything is revealed through gameplay (scanning, research, exploration).
- Bodies, zones, and star systems should only appear on the map once the player has discovered them.
- The `SolarSystemConfig` from the daemon already reflects discovered state — the map just needs to render what it receives, not hardcode content knowledge.
- When adding multi-system support, the daemon must gate which `body_absolutes` entries it sends based on discovery state.
- Fog-of-war or "unexplored" visual treatment for regions the player hasn't reached yet (future feature).

---

## Galactic Scale Architecture (Future)

The coordinate and navigation system needs to scale through multiple tiers:

### Zoom Hierarchy
| Scale | Distance Range | Coordinate System | LOD Content |
|-------|---------------|-------------------|-------------|
| **Local** | < 10 AU | Polar (parent_body + radius + angle) | Entities, asteroids, stations, ships |
| **System** | 10 - 1,000 AU | Absolute cartesian (body_absolutes) | Planets, zones, orbit rings |
| **Regional** | 1,000 - 100,000 AU | Star system relative positions | Star systems as markers, nearby stars |
| **Galactic** | > 100,000 AU | Galactic coordinates (kpc scale) | Spiral arms, sectors, star clusters |

### Positioning Model
Solar systems orbit galaxies, so the existing node/orbit pattern should extend upward:

```
Galaxy (root)
  └─ Galactic Arm / Sector (organizational)
       └─ Star System (node — like current OrbitalBodyDef with parent=null)
            └─ Star (body)
                 └─ Planet (body, orbits star)
                      └─ Moon (body, orbits planet)
```

- **Star systems** would use galactic polar coordinates (radius from galactic center + angle), same pattern as planets orbiting stars
- **Regional view** shows nearby star systems — positions derived from galactic coordinates, rendered with log projection
- **Galactic view** shows the full galaxy structure — likely a pre-rendered background image with system markers overlaid (rendering a full galaxy procedurally is impractical)

### Content Model Changes Needed
1. **New top-level in `solar_system.json`** (or separate `galaxy.json`): star system entries with galactic coordinates
2. **`body_absolutes` expansion:** daemon computes absolute positions at all scales — system-relative for local, galactic-absolute for regional/galactic
3. **Nav graph expansion:** new node types for star systems with transit edges (interstellar travel)
4. **Discovery gating:** star systems start hidden, revealed through long-range scanning or research

### What We're NOT Doing Yet
- Galaxy-scale simulation (ticking all systems) — Sol system is the active simulation; other systems are content/data, not ticked
- Real orbital mechanics at galactic scale — positions are fixed or slowly drifting, not simulated per-tick
- Procedural galaxy generation — manually placed systems for now, procedural later

**Decision: stick at galaxy scale ceiling for a long time.** The hierarchy above is the target architecture, but implementation starts with 2 star systems and adds regional/galactic views only when the game needs them.

---

## Known Issues / Future Iteration

1. **Label repulsion** — Ship labels near stations still crowd at close zoom. Need a simple label repulsion algorithm or priority-based hiding.
2. **Minimap at interstellar scale** — Current minimap shows Sol system only. Should switch to show both star systems at interstellar zoom.
3. **Scroll direction** — May need to swap (scroll up = zoom in) depending on platform/preference.
4. **Drag panning at interstellar** — Still needs tuning. The boost factor helps but the log projection makes screen-to-world conversion approximate during drag.
5. **Interstellar distance line** — Label overlaps with Alpha Centauri system marker. Needs offset.
6. **Font clarity** — Canvas text at 11px is readable but not as crisp as DOM text. Consider rendering labels as positioned DOM elements for sharpest text.
7. **Performance** — Not tested with 50+ ships, 100+ asteroids. May need spatial indexing (quadtree) for hit detection and culling.
8. **System filtering for Shift/Ctrl** — Currently shows all stations/ships regardless of system. Need to filter by proximity or parent_body system.
9. **Discovery-driven rendering** — Map currently renders all content. Needs to gate on player discovery state from backend.
10. **Galactic coordinate system** — Node/orbit model needs extension for star-system-orbits-galaxy positioning.

---

## Implementation Plan

### Phase 1: Canvas Map Core
- [ ] New `SolarSystemMapCanvas.tsx` component replacing `SolarSystemMap.tsx`
- [ ] Canvas rendering with `requestAnimationFrame`
- [ ] Camera system (zoom, pan, worldToScreen/screenToWorld)
- [ ] Consume `SolarSystemConfig` + `SimSnapshot` (same props as current)
- [ ] Render bodies, zones, orbit rings from config
- [ ] Render entities (stations, ships, asteroids, scan sites) from snapshot
- [ ] Starfield background tile (CSS)

### Phase 2: LOD & Interaction
- [ ] LOD tier system with smoothStep fading
- [ ] Zoom-to-cursor with log-space lerp
- [ ] Drag-to-pan with zoom-scaled speed
- [ ] Hover detection with tooltip (HTML overlay, not canvas)
- [ ] Click-to-select with DetailCard
- [ ] Double-click to zoom-to-entity

### Phase 3: Navigation HUD
- [ ] Quick-nav panel (HTML overlay)
- [ ] Keyboard shortcuts (1-9 for waypoints)
- [ ] Shift/Ctrl modifier for stations/ships
- [ ] Context-aware waypoints (per-system)
- [ ] Breadcrumb navigation
- [ ] Minimap with click-to-pan

### Phase 4: Transit & Data
- [ ] Transit line rendering with progress pip + ETA
- [ ] Rich tooltips matching game data types
- [ ] Ship task color coding from `config/theme.ts`
- [ ] Asteroid irregular shapes

### Phase 5: Multi-System (Future)
- [ ] Logarithmic projection for interstellar distances
- [ ] Star system markers with collapse/expand
- [ ] System-relative navigation
- [ ] Content model updates for new star systems

---

## Files to Modify

| File | Change |
|------|--------|
| `ui_web/src/components/SolarSystemMap.tsx` | Replace with canvas-based implementation |
| `ui_web/src/components/SolarSystemMap.test.tsx` | Update tests for canvas component |
| `ui_web/src/components/solar-system/Tooltip.tsx` | Keep but enhance with richer content |
| `ui_web/src/components/solar-system/DetailCard.tsx` | Keep, minimal changes |
| `ui_web/src/hooks/useSvgZoomPan.ts` | Remove (replaced by canvas camera) |
| `ui_web/src/config/theme.ts` | No changes (already centralized) |
| `content/solar_system.json` | Future: add Alpha Centauri bodies |

## Dependencies
- Remove: `d3-selection`, `d3-zoom` (no longer needed for map)
- Keep: `dagre` (used by TechTreeDAG, not map)
- Add: None (pure canvas + CSS)

---

## Design References

- **Mockup:** `/tmp/solar-map-mockup.html` (~800 lines, single-file HTML/CSS/JS)
- **Current implementation:** `ui_web/src/components/SolarSystemMap.tsx`
- **Game coordinate system:** `Position` type in `ui_web/src/types.ts`, spatial utils in `ui_web/src/utils/spatial.ts`
- **Solar system content:** `content/solar_system.json`
- **Theme colors:** `ui_web/src/config/theme.ts`

### Alpha Centauri Research Sources
- [Proxima Centauri - Wikipedia](https://en.wikipedia.org/wiki/Proxima_Centauri)
- [Alpha Centauri - Wikipedia](https://en.wikipedia.org/wiki/Alpha_Centauri)
- [Proxima Centauri b - Wikipedia](https://en.wikipedia.org/wiki/Proxima_Centauri_b)
- [ESO: New planet detected around star closest to the Sun](https://www.eso.org/public/news/eso2202/)
