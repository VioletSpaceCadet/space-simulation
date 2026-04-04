---
date: 2026-03-31
topic: game-ui-architecture
---

# Game UI Architecture — Full Game View with Tambo Integration

## Problem Frame

The current React + Vite + Tailwind dashboard UI is a functional development tool but not the final product experience. The Tambo project envisions the primary UI as a full game view — solar system as the main canvas, camera following action, entity rendering, route visualizations, approval card overlays — with an LLM co-pilot layer on top. The current panel-based dashboard can't deliver this experience. A planned rebuild is needed.

Multi-solar-system support adds interstellar zoom, system collapse, and warp visualization — all spatial rendering problems that need a proper game view canvas.

## Requirements

- R1. **Full game view** — Solar system as the main canvas, not one panel among many. Camera control (pan, zoom, focus on entities). Stellaris/Supreme Commander strategic view aesthetic.
- R2. **React + Canvas/WebGL hybrid architecture** — Canvas/WebGL for the game view (solar system, entities, routes, annotations, transit lines). React for HUD overlays (panels, Tambo chat, approval cards, menus, tooltips). This is the standard browser strategy game pattern.
- R3. **AI-developable stack** — React + TypeScript + Canvas API or PixiJS/Three.js. Standard web tech that AI coding tools handle well. No exotic engines (Godot, Bevy) that limit AI-assisted development.
- R4. **Tambo-ready layout** — The UI layout must accommodate Tambo as a primary interaction element: chat panel, approval cards overlaid on the game view, camera control from Tambo commands ("focus on the belt outpost"), annotation rendering from Tambo proposals.
- R5. **Entity rendering** — Stations, ships, satellites, asteroids, scan sites rendered as interactive objects on the canvas. Click/hover for details. Visual state (operational, constructing, worn out) visible.
- R6. **Route and logistics visualization** — Ship transit paths, supply chain flows between stations, launch trajectories drawn on the canvas. Material flow direction and volume indicated.
- R7. **Multi-system support** — Interstellar zoom (galaxy view → system view → local view). System collapse (distant systems as single objects). Warp lane rendering between systems.
- R8. **Panel overlay system** — React panels (score, economy, research, fleet, manufacturing) float over the game view. Toggleable, repositionable. Don't obscure critical game view elements.
- R9. **SSE streaming continues** — Real-time data from sim_daemon via SSE. The rendering layer consumes the same events/metrics the current UI does.

## Success Criteria

- The game view is the default experience (not a dashboard with a map widget)
- A new player (observer) immediately understands "this is a space simulation" from the visual
- Tambo approval cards render spatially (attached to relevant map locations, not just in a chat panel)
- Adding a new entity type (ground facility, satellite) to the game view is a rendering component addition, not an architecture change
- Performance: 60fps with 10 stations + 50 ships + 100 asteroids rendered

## Scope Boundaries

- Current React dashboard stays AS-IS through P0-P6 (it's the dev tool)
- UI rebuild happens AFTER P6, BEFORE Tambo integration
- No Electron/Tauri wrapping needed initially (browser-first, Tambo local inference handled separately)
- No 3D — 2D top-down/strategic view. Canvas2D or PixiJS, not Three.js/WebGL 3D.
- No mobile/responsive — desktop browser only

## Key Decisions

- **React + Canvas hybrid over full canvas:** React handles UI widgets (panels, chat, cards, menus) where its ecosystem excels. Canvas handles spatial rendering where DOM can't perform. This avoids reimplementing text layout, scrolling, forms in canvas.
- **After P6, before Tambo:** The sim foundation (P0-P6) must be complete before the UI rebuild. This ensures the UI is designed for the actual game systems, not hypothetical ones. The current dashboard serves development fine.
- **Browser over native:** Keeps the AI-developable web stack. Tambo's local inference (Mac Mini M4) can connect via WebSocket/HTTP — doesn't require Electron for file system access.
- **2D over 3D:** The simulation is fundamentally 2D (orbital positions in a plane). 3D adds visual complexity without gameplay value. Stellaris proves 2D strategic view works for space 4X.

## Dependencies / Assumptions

- P0-P6 complete (sim foundation stable before UI rebuild)
- API contract (sim_daemon endpoints) stable — the new UI consumes the same API
- Tambo interaction patterns documented during P0-P6 development (per Tambo project: "keep a running list of moments where tambo would add value")
- Canvas solar system map (already completed in Solar System Map Redesign project) serves as proof-of-concept for the rendering approach

## Outstanding Questions

### Deferred to Planning

- [Affects R2][Needs research] PixiJS vs raw Canvas2D vs Konva for the game view renderer? PixiJS gives GPU acceleration and sprite batching. Canvas2D is simpler. Research during planning.
- [Affects R4][Needs research] Tambo UI component library? Does tambo (the npm package) have prescribed UI patterns, or is the approval card / chat interface custom?
- [Affects R6][Technical] How does supply chain visualization work at multi-system scale? LOD-based: show individual ship movements at local zoom, aggregate flow arrows at system zoom, nothing at galaxy zoom?
- [Affects R7][Technical] System collapse rendering — when zoomed out to galaxy view, how to represent an entire solar system as one object? Score-based color/size? Activity indicator?
- [Affects R8][Technical] Panel overlay positioning — CSS absolute over canvas, or canvas-aware React portal? The former is simpler, the latter allows panels to "dodge" important game view elements.

## Next Steps

All outstanding questions are deferred to planning (no blocking questions). When ready:

-> /ce:plan for structured implementation planning of the UI rebuild (post-P6)
