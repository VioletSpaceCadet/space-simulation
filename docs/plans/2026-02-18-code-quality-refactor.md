# Code Quality Refactor Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Split Rust mega-files into logical modules and migrate the React UI from global CSS to Tailwind CSS v4.

**Architecture:** `sim_core/src/lib.rs` (1861 lines) splits into `types`, `graph`, `tasks`, `research`, and `engine` modules with `lib.rs` as a thin coordinator. `sim_daemon/src/main.rs` (472 lines) splits into `state`, `world`, `routes`, and `tick_loop` modules. The React UI replaces all `className="..."` BEM strings + `App.css` with Tailwind v4 utility classes and a space-game color theme.

**Tech Stack:** Rust workspace (sim_core, sim_daemon), Tailwind CSS v4 (`@tailwindcss/vite` plugin), Vite 7, React 19, TypeScript.

---

## Phase 1: sim_core Module Split

### Task 1: Extract `types.rs` from sim_core

**Files:**
- Create: `crates/sim_core/src/types.rs`
- Modify: `crates/sim_core/src/lib.rs`

**Context:** `lib.rs` currently starts with ~310 lines of type definitions before hitting any logic. Extract all types into a separate module. The tests remain in `lib.rs` and continue to work via `pub use types::*`.

**Step 1: Create `crates/sim_core/src/types.rs`** with everything from line 1 to the end of the `Constants` struct (just before the public tick entry point section). The file needs its own `use` block:

```rust
use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
```

Then copy verbatim every `pub` type, struct, and enum from `lib.rs` up to and including `Constants`. This includes:
- All newtype ID structs (`ShipId`, `SiteId`, `AsteroidId`, `NodeId`, `StationId`, `TechId`, `PrincipalId`, `CommandId`, `EventId`, `ElementId`)
- `DataKind`, `AnomalyTag`, `CompositionVec` type alias
- `GameState`, `MetaState`, `ScanSite`, `ShipState`, `TaskState`, `TaskKind`, `StationState`, `FacilitiesState`, `AsteroidState`, `AsteroidKnowledge`, `ResearchState`, `Counters`
- `EventEnvelope`, `Event`
- `GameContent`, `TechDef`, `TechEffect`, `SolarSystemDef`, `NodeDef`, `AsteroidTemplateDef`
- `Constants`
- `CommandEnvelope`, `Command`, `EventLevel`

**Step 2: Replace the type section in `lib.rs`** with just:

```rust
mod types;
pub use types::*;
```

The `use` statements that were at the top of `lib.rs` (`use std::collections::HashMap`, `use serde::...`, etc.) should be removed from `lib.rs` — they now live in `types.rs`. `lib.rs` will need to keep any `use` statements still needed for the remaining code.

**Step 3: Run tests**

```bash
~/.cargo/bin/cargo test -p sim_core 2>&1
```

Expected: All 28 tests pass.

**Step 4: Commit**

```bash
git add crates/sim_core/src/
git commit -m "refactor(sim_core): extract types into types.rs module"
```

---

### Task 2: Extract `graph.rs` from sim_core

**Files:**
- Create: `crates/sim_core/src/graph.rs`
- Modify: `crates/sim_core/src/lib.rs`

**Context:** `shortest_hop_count` is already a standalone `pub fn`. Move it to its own file.

**Step 1: Create `crates/sim_core/src/graph.rs`**:

```rust
use std::collections::{HashSet, VecDeque};
use crate::{NodeId, SolarSystemDef};

/// Returns the number of hops on the shortest undirected path between two nodes,
/// or `None` if no path exists. Returns `Some(0)` when `from == to`.
pub fn shortest_hop_count(from: &NodeId, to: &NodeId, solar_system: &SolarSystemDef) -> Option<u64> {
    if from == to {
        return Some(0);
    }
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back((from.clone(), 0u64));
    visited.insert(from.clone());
    while let Some((node, dist)) = queue.pop_front() {
        for (a, b) in &solar_system.edges {
            let neighbor = if a == &node {
                Some(b)
            } else if b == &node {
                Some(a)
            } else {
                None
            };
            if let Some(neighbor) = neighbor {
                if neighbor == to {
                    return Some(dist + 1);
                }
                if visited.insert(neighbor.clone()) {
                    queue.push_back((neighbor.clone(), dist + 1));
                }
            }
        }
    }
    None
}
```

**Step 2: In `lib.rs`**, remove the `shortest_hop_count` function body and replace with:

```rust
mod graph;
pub use graph::shortest_hop_count;
```

**Step 3: Run tests**

```bash
~/.cargo/bin/cargo test -p sim_core 2>&1
```

Expected: All 28 tests pass.

**Step 4: Commit**

```bash
git add crates/sim_core/src/
git commit -m "refactor(sim_core): extract graph utilities into graph.rs"
```

---

### Task 3: Extract `tasks.rs` from sim_core

**Files:**
- Create: `crates/sim_core/src/tasks.rs`
- Modify: `crates/sim_core/src/lib.rs`

**Context:** Move all task-related private/crate functions. `emit` stays in `lib.rs` as `pub(crate)` — it is needed here and in research.rs.

**Step 1: Create `crates/sim_core/src/tasks.rs`**. This file gets these functions (adjust visibility as noted):

```rust
use rand::Rng;
use crate::{
    AsteroidId, AsteroidKnowledge, AsteroidState, CompositionVec, DataKind,
    Event, GameContent, GameState, NodeId, ResearchState, ShipId, SiteId,
    TaskKind, TaskState, TechEffect,
};

// -- Private helpers ---------------------------------------------------------

fn normalise(composition: &mut CompositionVec) {
    let total: f32 = composition.values().sum();
    if total > 0.0 {
        for value in composition.values_mut() {
            *value /= total;
        }
    }
}

fn composition_noise_sigma(research: &ResearchState, content: &GameContent) -> f32 {
    content
        .techs
        .iter()
        .filter(|tech| research.unlocked.contains(&tech.id))
        .flat_map(|tech| &tech.effects)
        .find_map(|effect| match effect {
            TechEffect::DeepScanCompositionNoise { sigma } => Some(*sigma),
            _ => None,
        })
        .unwrap_or(0.0)
}

// -- pub(crate) helpers used by engine.rs ------------------------------------

pub(crate) fn task_duration(kind: &TaskKind, constants: &crate::Constants) -> u64 {
    match kind {
        TaskKind::Transit { total_ticks, .. } => *total_ticks,
        TaskKind::Survey { .. } => constants.survey_scan_ticks,
        TaskKind::DeepScan { .. } => constants.deep_scan_ticks,
        TaskKind::Idle => 0,
    }
}

pub(crate) fn task_kind_label(kind: &TaskKind) -> &'static str {
    match kind {
        TaskKind::Idle => "Idle",
        TaskKind::Transit { .. } => "Transit",
        TaskKind::Survey { .. } => "Survey",
        TaskKind::DeepScan { .. } => "DeepScan",
    }
}

pub(crate) fn task_target(kind: &TaskKind) -> Option<String> {
    match kind {
        TaskKind::Idle => None,
        TaskKind::Transit { destination, .. } => Some(destination.0.clone()),
        TaskKind::Survey { site } => Some(site.0.clone()),
        TaskKind::DeepScan { asteroid } => Some(asteroid.0.clone()),
    }
}

/// True if any unlocked tech grants the EnableDeepScan effect.
pub(crate) fn deep_scan_enabled(research: &ResearchState, content: &GameContent) -> bool {
    content
        .techs
        .iter()
        .filter(|tech| research.unlocked.contains(&tech.id))
        .flat_map(|tech| &tech.effects)
        .any(|effect| matches!(effect, TechEffect::EnableDeepScan))
}

// -- Task resolution ---------------------------------------------------------

pub(crate) fn set_ship_idle(state: &mut GameState, ship_id: &ShipId, current_tick: u64) {
    if let Some(ship) = state.ships.get_mut(ship_id) {
        ship.task = Some(TaskState {
            kind: TaskKind::Idle,
            started_tick: current_tick,
            eta_tick: current_tick,
        });
    }
}

pub(crate) fn resolve_transit(
    state: &mut GameState,
    ship_id: &ShipId,
    destination: &NodeId,
    then: &TaskKind,
    content: &GameContent,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    if let Some(ship) = state.ships.get_mut(ship_id) {
        ship.location_node = destination.clone();
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::ShipArrived {
            ship_id: ship_id.clone(),
            node: destination.clone(),
        },
    ));

    let duration = task_duration(then, &content.constants);
    let label = task_kind_label(then).to_string();
    let target = task_target(then);

    if let Some(ship) = state.ships.get_mut(ship_id) {
        ship.task = Some(TaskState {
            kind: then.clone(),
            started_tick: current_tick,
            eta_tick: current_tick + duration,
        });
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::TaskStarted {
            ship_id: ship_id.clone(),
            task_kind: label,
            target,
        },
    ));
}

pub(crate) fn resolve_survey(
    state: &mut GameState,
    ship_id: &ShipId,
    site_id: &SiteId,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    let Some(site_pos) = state.scan_sites.iter().position(|s| &s.id == site_id) else {
        return;
    };
    let site = state.scan_sites.remove(site_pos);

    let Some(template) = content
        .asteroid_templates
        .iter()
        .find(|t| t.id == site.template_id)
    else {
        return;
    };

    let mut composition: CompositionVec = template
        .composition_ranges
        .iter()
        .map(|(element, &(min, max))| (element.clone(), rng.gen_range(min..=max)))
        .collect();
    normalise(&mut composition);

    let asteroid_id = crate::AsteroidId(format!("asteroid_{:04}", state.counters.next_asteroid_id));
    state.counters.next_asteroid_id += 1;

    let anomaly_tags = template.anomaly_tags.clone();
    state.asteroids.insert(
        asteroid_id.clone(),
        AsteroidState {
            id: asteroid_id.clone(),
            location_node: site.node.clone(),
            true_composition: composition,
            anomaly_tags: anomaly_tags.clone(),
            knowledge: AsteroidKnowledge {
                tag_beliefs: vec![],
                composition: None,
            },
        },
    );

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::AsteroidDiscovered {
            asteroid_id: asteroid_id.clone(),
            location_node: site.node.clone(),
        },
    ));

    let detection_prob = content.constants.survey_tag_detection_probability;
    let detected_tags: Vec<(crate::AnomalyTag, f32)> = anomaly_tags
        .iter()
        .filter(|_| rng.gen::<f32>() < detection_prob)
        .map(|tag| (tag.clone(), detection_prob))
        .collect();

    if let Some(asteroid) = state.asteroids.get_mut(&asteroid_id) {
        asteroid.knowledge.tag_beliefs = detected_tags.clone();
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::ScanResult {
            asteroid_id: asteroid_id.clone(),
            tags: detected_tags,
        },
    ));

    let amount = content.constants.survey_scan_data_amount;
    let quality = content.constants.survey_scan_data_quality;
    *state
        .research
        .data_pool
        .entry(DataKind::ScanData)
        .or_insert(0.0) += amount * quality;

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::DataGenerated {
            kind: DataKind::ScanData,
            amount,
            quality,
        },
    ));

    set_ship_idle(state, ship_id, current_tick);

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::TaskCompleted {
            ship_id: ship_id.clone(),
            task_kind: "Survey".to_string(),
            target: Some(site_id.0.clone()),
        },
    ));
}

pub(crate) fn resolve_deep_scan(
    state: &mut GameState,
    ship_id: &ShipId,
    asteroid_id: &AsteroidId,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<crate::EventEnvelope>,
) {
    let current_tick = state.meta.tick;

    let sigma = composition_noise_sigma(&state.research, content);

    let Some(true_composition) = state
        .asteroids
        .get(asteroid_id)
        .map(|a| a.true_composition.clone())
    else {
        return;
    };

    let mut mapped: CompositionVec = true_composition
        .iter()
        .map(|(element, &true_value)| {
            let noise = if sigma > 0.0 {
                rng.gen_range(-sigma..=sigma)
            } else {
                0.0
            };
            (element.clone(), (true_value + noise).clamp(0.0, 1.0))
        })
        .collect();
    normalise(&mut mapped);

    if let Some(asteroid) = state.asteroids.get_mut(asteroid_id) {
        asteroid.knowledge.composition = Some(mapped.clone());
    }

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::CompositionMapped {
            asteroid_id: asteroid_id.clone(),
            composition: mapped,
        },
    ));

    let amount = content.constants.deep_scan_data_amount;
    let quality = content.constants.deep_scan_data_quality;
    *state
        .research
        .data_pool
        .entry(DataKind::ScanData)
        .or_insert(0.0) += amount * quality;

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::DataGenerated {
            kind: DataKind::ScanData,
            amount,
            quality,
        },
    ));

    set_ship_idle(state, ship_id, current_tick);

    events.push(crate::emit(
        &mut state.counters,
        current_tick,
        Event::TaskCompleted {
            ship_id: ship_id.clone(),
            task_kind: "DeepScan".to_string(),
            target: Some(asteroid_id.0.clone()),
        },
    ));
}
```

**Step 2: In `lib.rs`**, remove the bodies of all the functions above and replace with:

```rust
mod tasks;
use tasks::{resolve_transit, resolve_survey, resolve_deep_scan};
pub(crate) use tasks::{task_duration, task_kind_label, task_target, deep_scan_enabled};
```

**Step 3: Run tests**

```bash
~/.cargo/bin/cargo test -p sim_core 2>&1
```

Expected: All 28 tests pass.

**Step 4: Commit**

```bash
git add crates/sim_core/src/
git commit -m "refactor(sim_core): extract task resolution into tasks.rs"
```

---

### Task 4: Extract `research.rs` and `engine.rs`, slim down `lib.rs`

**Files:**
- Create: `crates/sim_core/src/research.rs`
- Create: `crates/sim_core/src/engine.rs`
- Modify: `crates/sim_core/src/lib.rs`

**Context:** Move `advance_research` to `research.rs` and `tick`/`apply_commands`/`resolve_ship_tasks` to `engine.rs`. `lib.rs` becomes a thin coordinator with just: module declarations, re-exports, the `pub(crate) fn emit` helper, and the test module.

**Step 1: Create `crates/sim_core/src/research.rs`**:

```rust
use rand::Rng;
use crate::{DataKind, Event, EventLevel, GameContent, GameState, StationId, TechId};

pub(crate) fn advance_research(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    event_level: EventLevel,
    events: &mut Vec<crate::EventEnvelope>,
) {
    // Copy the full advance_research function body verbatim from lib.rs.
    // It uses crate::emit(...) for all event emission.
    // Replace all direct `emit(...)` calls with `crate::emit(...)`.
}
```

Copy the complete body of `advance_research` (including the `station_ids` sort, power computation, eligible tech collection, evidence accumulation, and tech unlock logic) verbatim. Replace bare `emit(...)` calls with `crate::emit(...)`.

**Step 2: Create `crates/sim_core/src/engine.rs`**:

```rust
use rand::Rng;
use crate::{
    Command, EventLevel, GameContent, GameState, ShipId, TaskKind,
};
use crate::tasks::{
    deep_scan_enabled, resolve_deep_scan, resolve_survey, resolve_transit,
    task_duration, task_kind_label, task_target,
};
use crate::research::advance_research;

pub fn tick(
    state: &mut GameState,
    commands: &[crate::CommandEnvelope],
    content: &GameContent,
    rng: &mut impl Rng,
    event_level: EventLevel,
) -> Vec<crate::EventEnvelope> {
    let mut events = Vec::new();
    apply_commands(state, commands, content, &mut events);
    resolve_ship_tasks(state, content, rng, &mut events);
    advance_research(state, content, rng, event_level, &mut events);
    state.meta.tick += 1;
    events
}

fn apply_commands(
    state: &mut GameState,
    commands: &[crate::CommandEnvelope],
    content: &GameContent,
    events: &mut Vec<crate::EventEnvelope>,
) {
    // Copy verbatim from lib.rs. Replace bare emit(...) with crate::emit(...).
    // Replace bare task_duration/task_kind_label/task_target calls — they're
    // already in scope via `use crate::tasks::...` above.
}

fn resolve_ship_tasks(
    state: &mut GameState,
    content: &GameContent,
    rng: &mut impl Rng,
    events: &mut Vec<crate::EventEnvelope>,
) {
    // Copy verbatim from lib.rs.
    // The match arms call resolve_transit, resolve_survey, resolve_deep_scan
    // which are in scope via `use crate::tasks::...` above.
}
```

**Step 3: Slim down `lib.rs`** to just this:

```rust
mod types;
mod graph;
mod tasks;
mod research;
mod engine;

pub use types::*;
pub use graph::shortest_hop_count;
pub use engine::tick;

pub(crate) fn emit(counters: &mut Counters, tick: u64, event: Event) -> EventEnvelope {
    let id = EventId(format!("evt_{:06}", counters.next_event_id));
    counters.next_event_id += 1;
    EventEnvelope { id, tick, event }
}

// All existing tests stay here, unchanged.
#[cfg(test)]
mod tests {
    use super::*;
    // ... (keep all existing test code verbatim)
}
```

**Step 4: Run tests**

```bash
~/.cargo/bin/cargo test -p sim_core 2>&1
```

Expected: All 28 tests pass. Fix any `use` path errors the compiler reports.

**Step 5: Commit**

```bash
git add crates/sim_core/src/
git commit -m "refactor(sim_core): extract research and engine modules, slim lib.rs to coordinator"
```

---

## Phase 2: sim_daemon Module Split

### Task 5: Extract `state.rs` and `world.rs` from sim_daemon

**Files:**
- Create: `crates/sim_daemon/src/state.rs`
- Create: `crates/sim_daemon/src/world.rs`
- Modify: `crates/sim_daemon/src/main.rs`

**Context:** `SimState`, `SharedSim`, `EventTx`, `AppState` are pure data. `load_content` and `build_initial_state` are pure functions with no HTTP concerns. Moving them cleans up `main.rs` significantly.

**Step 1: Create `crates/sim_daemon/src/state.rs`**:

```rust
use std::sync::{Arc, Mutex};
use sim_control::AutopilotController;
use sim_core::{GameContent, GameState};
use rand_chacha::ChaCha8Rng;
use tokio::sync::broadcast;
use sim_core::EventEnvelope;

pub struct SimState {
    pub game_state: GameState,
    pub content: GameContent,
    pub rng: ChaCha8Rng,
    pub autopilot: AutopilotController,
    pub next_command_id: u64,
}

pub type SharedSim = Arc<Mutex<SimState>>;
pub type EventTx = broadcast::Sender<Vec<EventEnvelope>>;

#[derive(Clone)]
pub struct AppState {
    pub sim: SharedSim,
    pub event_tx: EventTx,
}
```

**Step 2: Create `crates/sim_daemon/src/world.rs`**:

```rust
use std::path::Path;
use anyhow::{Context, Result};
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use serde::Deserialize;
use sim_core::{
    AsteroidTemplateDef, Constants, Counters, FacilitiesState, GameContent, GameState,
    MetaState, NodeId, PrincipalId, ResearchState, ScanSite, ShipId, ShipState,
    SiteId, SolarSystemDef, StationId, StationState, TechDef,
};

#[derive(Deserialize)]
pub struct TechsFile {
    pub content_version: String,
    pub techs: Vec<TechDef>,
}

#[derive(Deserialize)]
pub struct AsteroidTemplatesFile {
    pub templates: Vec<AsteroidTemplateDef>,
}

pub fn load_content(content_dir: &str) -> Result<GameContent> {
    // Copy verbatim from main.rs
}

pub fn build_initial_state(content: &GameContent, seed: u64, rng: &mut impl Rng) -> GameState {
    // Copy verbatim from main.rs
}
```

**Step 3: In `main.rs`**, remove the four struct/type definitions and the two functions, and add at the top of the file:

```rust
mod state;
mod world;
use state::{AppState, SharedSim, SimState};
use world::{load_content, build_initial_state};
```

**Step 4: Run tests**

```bash
~/.cargo/bin/cargo test -p sim_daemon 2>&1
```

Expected: All 4 tests pass.

**Step 5: Commit**

```bash
git add crates/sim_daemon/src/
git commit -m "refactor(sim_daemon): extract state types and world-gen into own modules"
```

---

### Task 6: Extract `routes.rs` and `tick_loop.rs` from sim_daemon

**Files:**
- Create: `crates/sim_daemon/src/routes.rs`
- Create: `crates/sim_daemon/src/tick_loop.rs`
- Modify: `crates/sim_daemon/src/main.rs`

**Step 1: Create `crates/sim_daemon/src/routes.rs`**:

```rust
use std::convert::Infallible;
use std::time::Duration;
use axum::{
    extract::State,
    http::{header, Method, StatusCode},
    response::{
        sse::{Event, Sse},
        Json,
    },
    routing::get,
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use crate::state::AppState;

pub fn make_router(state: AppState) -> Router {
    // Copy verbatim from main.rs
}

pub async fn meta_handler(State(app_state): State<AppState>) -> Json<serde_json::Value> {
    // Copy verbatim
}

pub async fn snapshot_handler(
    State(app_state): State<AppState>,
) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    // Copy verbatim
}

pub async fn stream_handler(
    State(app_state): State<AppState>,
) -> Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>> {
    // Copy verbatim
}
```

**Step 2: Create `crates/sim_daemon/src/tick_loop.rs`**:

```rust
use std::time::Duration;
use sim_control::CommandSource;
use sim_core::EventLevel;
use crate::state::{SharedSim, EventTx};

pub async fn run_tick_loop(
    sim: SharedSim,
    event_tx: EventTx,
    ticks_per_sec: f64,
    max_ticks: Option<u64>,
) {
    // Copy verbatim from main.rs
}
```

**Step 3: In `main.rs`**, add:

```rust
mod routes;
mod tick_loop;
use routes::make_router;
use tick_loop::run_tick_loop;
```

And remove the function bodies that moved.

**Step 4: Run tests**

```bash
~/.cargo/bin/cargo test -p sim_daemon 2>&1
```

Expected: All 4 tests pass.

**Step 5: Commit**

```bash
git add crates/sim_daemon/src/
git commit -m "refactor(sim_daemon): extract HTTP routes and tick loop into own modules"
```

---

## Phase 3: Tailwind CSS Migration

### Task 7: Install Tailwind v4 and configure Vite

**Files:**
- Modify: `ui_web/vite.config.ts`
- Modify: `ui_web/src/index.css`
- Delete: `ui_web/src/App.css`

**Context:** Tailwind v4 uses a Vite plugin instead of a PostCSS plugin. Theme tokens are defined in CSS via `@theme`. There is no `tailwind.config.js` needed.

**Step 1: Install Tailwind v4**

```bash
cd ui_web && npm install -D tailwindcss @tailwindcss/vite
```

**Step 2: Update `ui_web/vite.config.ts`**:

```typescript
/// <reference types="vitest/config" />
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: './src/test-setup.ts',
  },
  server: {
    proxy: {
      '/api': 'http://localhost:3001',
    },
  },
})
```

**Step 3: Replace `ui_web/src/index.css`** with just:

```css
@import "tailwindcss";
```

**Step 4: Replace `ui_web/src/App.css`** with the space color theme tokens (these replace all the BEM CSS classes):

```css
@layer base {
  * {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
  }

  body {
    background-color: #0a0e1a;
    color: #c8d8f0;
    font-family: 'Courier New', Courier, monospace;
    font-size: 13px;
  }
}
```

Note: `App.css` no longer contains component-level styles. Those move inline into each component as Tailwind utilities. The base layer here sets the global body defaults.

**Step 5: Update `ui_web/src/main.tsx`** to still import both (order matters — Tailwind base first):

```typescript
import './index.css'
import './App.css'
```

If `main.tsx` currently only imports one of these, add the other.

**Step 6: Run tests**

```bash
cd ui_web && npm test -- --run
```

Expected: All 29 tests pass (styling changes don't affect test assertions).

**Step 7: Commit**

```bash
git add ui_web/
git commit -m "refactor(ui): install Tailwind v4, configure Vite plugin, define base styles"
```

---

### Task 8: Migrate App.tsx to Tailwind

**Files:**
- Modify: `ui_web/src/App.tsx`

**Context:** App.tsx currently uses `className="app"`, `className="panels"`, and passes className props to panels. Replace with Tailwind utilities.

**Current `App.tsx`** (read the file before editing to see current structure). The key changes:

- `.app` → `flex flex-col h-screen overflow-hidden`
- `.panels` → `flex flex-1 overflow-hidden gap-px bg-[#1e2d50]`
- `.panel.panel-events` → `flex flex-col overflow-hidden bg-[#0a0e1a] p-3 flex-1 min-w-[220px]`
- `.panel.panel-asteroids` → `flex flex-col overflow-hidden bg-[#0a0e1a] p-3 flex-[2]`
- `.panel.panel-research` → `flex flex-col overflow-hidden bg-[#0a0e1a] p-3 flex-1 min-w-[220px]`

Each panel's `<h2>` gets: `text-[11px] uppercase tracking-widest text-[#4a6a9a] mb-2 pb-1.5 border-b border-[#1e2d50] shrink-0`

**Step 1: Update `App.tsx`** replacing all `className="..."` strings with Tailwind utilities per the mapping above.

**Step 2: Run tests**

```bash
cd ui_web && npm test -- --run
```

Expected: All 29 tests pass.

**Step 3: Commit**

```bash
git add ui_web/src/App.tsx
git commit -m "refactor(ui): migrate App layout to Tailwind utilities"
```

---

### Task 9: Migrate StatusBar to Tailwind

**Files:**
- Modify: `ui_web/src/components/StatusBar.tsx`

**CSS mapping:**
- `.status-bar` → `flex gap-6 items-center px-4 py-1.5 bg-[#0d1226] border-b border-[#1e2d50] text-xs shrink-0`
- `.status-tick` → `text-[#a8c4e8] font-bold`
- `.status-time` → `text-[#7a9cc8]`
- connected state → `text-[#4caf7d]`
- disconnected state → `text-[#e05555]`

**Step 1: Rewrite `StatusBar.tsx`** without any `className` strings referencing the old CSS classes:

```tsx
interface Props {
  tick: number
  connected: boolean
}

export function StatusBar({ tick, connected }: Props) {
  const day = Math.floor(tick / 1440)
  const hour = Math.floor((tick % 1440) / 60)
  const minute = tick % 60

  return (
    <div className="flex gap-6 items-center px-4 py-1.5 bg-[#0d1226] border-b border-[#1e2d50] text-xs shrink-0">
      <span className="text-[#a8c4e8] font-bold">tick {tick}</span>
      <span className="text-[#7a9cc8]">
        day {day} | {String(hour).padStart(2, '0')}:{String(minute).padStart(2, '0')}
      </span>
      <span className={connected ? 'text-[#4caf7d]' : 'text-[#e05555]'}>
        {connected ? '● connected' : '○ reconnecting...'}
      </span>
    </div>
  )
}
```

**Step 2: Run tests**

```bash
cd ui_web && npm test -- --run
```

Expected: All 29 tests pass. The StatusBar tests check text content ("connected", "reconnecting") not class names.

**Step 3: Commit**

```bash
git add ui_web/src/components/StatusBar.tsx
git commit -m "refactor(ui): migrate StatusBar to Tailwind utilities"
```

---

### Task 10: Migrate EventsFeed to Tailwind

**Files:**
- Modify: `ui_web/src/components/EventsFeed.tsx`

**CSS mapping:**
- `.events-feed` → `overflow-y-auto flex-1`
- `.events-empty` → `text-[#3a5070] italic`
- `.event-row` → `flex gap-1.5 py-0.5 border-b border-[#0d1226] text-[11px] overflow-hidden`
- `.event-id` → `text-[#3a6090] min-w-[90px] shrink-0`
- `.event-tick` → `text-[#2a4060] min-w-[44px] shrink-0`
- `.event-type` → `text-[#70a0d0] min-w-[120px] shrink-0`
- `.event-detail` → `text-[#607090] overflow-hidden text-ellipsis whitespace-nowrap`

**Step 1: Rewrite `EventsFeed.tsx`** applying the mapping above to every `className` attribute.

**Step 2: Run tests**

```bash
cd ui_web && npm test -- --run
```

Expected: All 29 tests pass.

**Step 3: Commit**

```bash
git add ui_web/src/components/EventsFeed.tsx
git commit -m "refactor(ui): migrate EventsFeed to Tailwind utilities"
```

---

### Task 11: Migrate AsteroidTable to Tailwind

**Files:**
- Modify: `ui_web/src/components/AsteroidTable.tsx`

**CSS mapping:**
- `.asteroid-table` → `overflow-y-auto flex-1`
- `.table-empty` → `text-[#3a5070] italic`
- `table` → `w-full border-collapse text-[11px]`
- `th` → `text-left text-[#4a6a9a] px-2 py-1 border-b border-[#1e2d50] font-normal`
- `td` → `px-2 py-0.5 border-b border-[#0d1226]`

**Step 1: Rewrite `AsteroidTable.tsx`** with Tailwind utilities.

**Step 2: Run tests**

```bash
cd ui_web && npm test -- --run
```

Expected: All 29 tests pass.

**Step 3: Commit**

```bash
git add ui_web/src/components/AsteroidTable.tsx
git commit -m "refactor(ui): migrate AsteroidTable to Tailwind utilities"
```

---

### Task 12: Migrate ResearchPanel to Tailwind

**Files:**
- Modify: `ui_web/src/components/ResearchPanel.tsx`

**CSS mapping:**
- `.research-panel` → `overflow-y-auto flex-1`
- `.data-pool` → `flex flex-wrap gap-1.5 mb-2.5 text-[11px] text-[#7a9cc8]`
- `.data-pool .label` → `text-[#4a6a9a]`
- `.data-item.empty` → `text-[#3a5070]`
- `.tech-row` → `py-1.5 border-b border-[#0d1226] text-[11px]`
- `.tech-id` → `text-[#70a0d0] mb-0.5`
- `.tech-evidence` → `text-[#506080]`
- `.tech-status` → `text-[#506080] mt-0.5`
- `.tech-status.unlocked` → `text-[#4caf7d]`
- `.tech-empty` → `text-[#3a5070] italic`

**Step 1: Rewrite `ResearchPanel.tsx`** with Tailwind utilities. Use a ternary for the tech status color:

```tsx
<div className={`mt-0.5 ${isUnlocked ? 'text-[#4caf7d]' : 'text-[#506080]'}`}>
```

**Step 2: Run tests**

```bash
cd ui_web && npm test -- --run
```

Expected: All 29 tests pass.

**Step 3: Commit**

```bash
git add ui_web/src/components/ResearchPanel.tsx
git commit -m "refactor(ui): migrate ResearchPanel to Tailwind utilities"
```

---

### Task 13: Final cleanup — delete App.css CSS classes, verify all tests

**Files:**
- Modify: `ui_web/src/App.css` (remove all component CSS classes, keep only base layer)
- Verify: all test files still pass

**Context:** By this point all component CSS has moved inline as Tailwind utilities. The old BEM class names in `App.css` are unused dead code.

**Step 1: Edit `ui_web/src/App.css`** to remove everything except the `@layer base` block with `*` and `body` rules (which we set in Task 7). It should look like:

```css
@layer base {
  * {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
  }

  body {
    background-color: #0a0e1a;
    color: #c8d8f0;
    font-family: 'Courier New', Courier, monospace;
    font-size: 13px;
  }
}
```

**Step 2: Run the full test suite — both Rust and React**

```bash
~/.cargo/bin/cargo test 2>&1
```

Expected: 28 sim_core + 4 sim_daemon tests pass.

```bash
cd ui_web && npm test -- --run 2>&1
```

Expected: All 29 tests pass.

**Step 3: Final commit**

```bash
git add ui_web/src/App.css
git commit -m "refactor(ui): remove dead CSS classes now that all components use Tailwind"
```

---

## Completion

After all 13 tasks, the codebase should have:
- `sim_core`: 5 focused modules (`types`, `graph`, `tasks`, `research`, `engine`) + thin `lib.rs` coordinator
- `sim_daemon`: 4 focused modules (`state`, `world`, `routes`, `tick_loop`) + thin `main.rs` entry point
- React UI: zero global BEM CSS classes, all styling via Tailwind v4 utilities
- All 28 Rust + 29 React tests still passing
