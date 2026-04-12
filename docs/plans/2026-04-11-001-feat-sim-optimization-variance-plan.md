---
title: sim optimization variance + copilotkit foundation
type: feat
status: active
date: 2026-04-11
origin: docs/brainstorms/2026-04-11-sim-optimization-variance-requirements.md
---

# Sim Optimization Variance + CopilotKit Foundation

## Overview

Two adjacent projects, planned as one document because the architecture decisions for the LLM co-pilot layer shape the observability requirements of the game-loop work, and both need to be decided up front.

- **Project A — Sim Optimization Variance** (Rust-heavy): inject real strategy space into the sim so the Bayesian optimizer finds meaningful variance. Implements the brainstormed requirements R1–R25 across diagnostic scenarios, a unified launch-reliability + reusability subsystem (absorbing VIO-560), phase-specific commitment decisions, event-system enrichment, and LLM-forward-compat observability.
- **Project B — CopilotKit Foundation** (TypeScript-heavy): stand up the LLM co-pilot architecture against the live sim. One framework (CopilotKit), local LLM routing (OpenRouter now → Ollama on Mac Mini M4 Pro later), approval-card UX, reusing the existing `mcp_advisor` MCP server as the tool surface.
- **Project C — Tier Progression + Advanced Interactions** (DEFERRED): AI-research-gated co-pilot tier progression, sophisticated planning flows, multi-civ support. Not planned here. Picked up in a separate brainstorm after P5, per the Tambo project description.

Origin document: [`docs/brainstorms/2026-04-11-sim-optimization-variance-requirements.md`](../brainstorms/2026-04-11-sim-optimization-variance-requirements.md). Every requirement below maps to an origin R-number.

## Project Structure Decision

**Two separate projects, one plan document, clear separation at the ticket level.**

Why two projects, not one:
- **Different languages, different rhythms.** Project A is Rust + Python + JSON content. Project B is TypeScript + React + Node sidecar. Bundling them into one project would create artificial context-switching and dilute review attention.
- **Different ship gates.** Project A ships when the optimizer finds ≥8% score spread on the new scenarios (R17 success criterion). Project B ships when the co-pilot can round-trip one real command through an approval card end-to-end. These are independent success conditions.
- **Different risk profiles.** Project A is sim-internals work — high-confidence, template-driven (there are solid precedents in P3/P4). Project B is new-stack integration — higher uncertainty, more room for dead ends. Bundling would drag Project A's velocity down.
- **Project B can start before Project A ships.** It stands up CopilotKit against the *current* sim surface. The new observability hooks from Project A (R22–R25) make Project B better, but don't block it. The parallelism is the whole point.

Why one plan document:
- The user explicitly asked for architecture decisions to be made up-front, including for CopilotKit/Tambo.
- Project A's observability requirements (R22–R25) are written to make Project B possible. Those decisions must be aligned across both tracks.
- Deferring Project B to a separate plan doc would lose the architectural rationale this doc captures.

Sequencing recommendation:
- **Start Project A immediately** along the critical path: M0 → (M1 ∥ M2) → M3 → M4 → M6 → M10. This is the Rust-heavy backbone.
- **Start Project B in parallel after M0 lands** (M0 is tiny — scenario files + validator). Project B's Mb1 (runtime sidecar + OpenRouter adapter) depends on nothing from Project A.
- **Converge at M9 / Mb4**: when Project A ships the launch-preview endpoint and updated metrics digest, Project B's Mb4 consumes them via CopilotKit's MCP integration.

## High-Level Architecture Decisions

Decisions made up-front so the plan has ground to stand on. Each has a short rationale. Later sections reference these.

### Decision 1: CopilotKit, not Tambo

**User confirmed one-or-the-other.** Choosing **CopilotKit**.

Rationale:
- **Mature local-LLM integration path.** `BuiltInAgent` + `@ai-sdk/openai-compatible` is the canonical 2026 pattern for pointing at Ollama or OpenRouter. Proven with Qwen2.5/Qwen3 in production.
- **First-class MCP support shipped January 2026** (via `CopilotRuntime({ createMCPClient })`). This is decisive: we can expose the existing `mcp_advisor` stdio server directly as a CopilotKit tool source without re-implementing `get_metrics_digest`, `suggest_strategy_change`, etc. as parallel `useCopilotAction` definitions. Single source of truth.
- **Generative UI / approval cards** via `renderAndWaitForResponse` is exactly the approval-card pattern the Tambo project description called out. Idiomatic, not a workaround.
- **`useCopilotReadable` with memoized snapshot on pause** maps cleanly onto the existing SSE-driven Zustand-ish store pattern in `ui_web`. Research confirmed this works on Vite 7 + React 19 + TS 5 without special config.
- **Community + docs depth.** Research surfaced multiple working Ollama integration gists, issue threads, and late-2025/early-2026 blog posts. Tambo's docs are thinner and the patterns for state-rich apps are less proven.
- **Risk**: CopilotKit has known tool-schema simplification issues (#2220, #2061) with complex nested params. Mitigation: keep action params flat (one level of nesting max), validate on the daemon side regardless.

### Decision 2: Local LLM stack — Ollama + Qwen2.5-14B primary, Qwen3-30B-A3B strategic

- **Phase A (now, user's current machine)**: OpenRouter. Default model `qwen/qwen-2.5-72b-instruct` for reasoning quality, fallback to cheaper tiers for routine calls. Controlled by a single env var (`LLM_PROVIDER=openrouter`).
- **Phase B (when Mac Mini M4 Pro arrives, 48 GB unified memory)**: Ollama, default `qwen2.5:14b-instruct` Q4_K_M (~8.4 GB resident, strong tool-calling, proven with CopilotKit). Strategic queries optionally route to `qwen3:30b-a3b` (~18 GB, fast MoE with ~3B active params). `OLLAMA_KEEP_ALIVE=30m` to avoid cold starts.
- **Swap mechanism**: env-var-driven factory in the runtime sidecar. No abstraction layer — KISS.
- **Temperature**: `0.2` across the board. The sim is deterministic; we want the LLM to be as near-deterministic as sampling allows, particularly for preview/query actions that should be idempotent.
- **Fallback path if Ollama disappoints**: MLX via `mlx-lm.server` (also OpenAI-compatible, 20–50% faster prompt processing on Apple Silicon). Only switch if we measure a latency problem against the ≤3 s player-time budget.

### Decision 3: Sidecar Node runtime, not Vite middleware

CopilotKit runtime lives in a **new sibling crate / directory** `copilot_runtime/` (TypeScript Express sidecar), bound to `127.0.0.1` on a dedicated port (4000). Not embedded in Vite dev middleware — Vite HMR + middleware is fragile and not the canonical 2026 deployment pattern.

Process topology for local development:
- `sim_daemon` on `:3001`
- `ui_web` Vite dev server on `:5173`
- `copilot_runtime` sidecar on `:4000`
- `ollama` on `:11434` (local inference, Phase B only)
- `mcp_advisor` spawned as a child stdio process by `copilot_runtime` via CopilotKit's `createMCPClient`

`ui_web` wraps its root with `<CopilotKit runtimeUrl="http://localhost:4000/api/copilotkit" agent="default">`. No daemon traffic goes through the runtime — actions that hit `sim_daemon` do so directly from the browser via `fetch`.

### Decision 4: CopilotKit consumes the existing `mcp_advisor` via MCP

The canonical tool surface for game analysis already exists in `mcp_advisor/src/index.ts` (14 tools). Research confirmed CopilotKit can mount this directly via `createMCPClient` — no re-implementation. Benefits:
- One source of truth for tool definitions.
- The same tools Claude Code uses during dev balance sessions are the tools the player-facing co-pilot uses in Phase B. Symmetry reduces drift.
- `mcp_advisor` already knows how to hit `sim_daemon` HTTP endpoints and format results.

`useCopilotAction` is reserved for **player-only command-executing actions** that need UI (approval cards): `proposeLaunch`, `scrapRocket`, `confirmBuildOrder`. Read-only queries go through MCP.

### Decision 5: Rocket state model — side table, not `InventoryItem` extension

**Most consequential data-model decision in Project A.** Research uncovered that ADR-4 (the reusability system ADR buried in `docs/plans/2026-03-30-003-feat-p2-ground-operations-launch-system-plan.md:269`) says "rockets are inventory items with metadata" — but the current `InventoryItem::Component` variant has no per-instance metadata slot. Components are count-fungible.

With reliability + reusability, rockets **must** have identity: a 5×-reused booster with `build_quality = 0.7` is fundamentally different from a fresh one. They cannot share inventory counts.

**Decision**: introduce `state.rockets: BTreeMap<RocketInstanceId, RocketState>` as a side table on `GameState`. The inventory item contract stays untouched. `LaunchExecution` scans the side table instead of scanning inventory.

Why not extend `InventoryItem::Rocket { instance_id, … }`:
- The inventory system is optimized around fungible counts. Breaking that contract ripples across UI, metrics, recipes, and trade.
- A side table matches how other identity-bearing entities work (stations, ships, satellites all live in dedicated maps on `GameState`).
- `BTreeMap` iteration is sorted by key, satisfying the determinism gotcha from the past-learnings research.

**Migration**: on save-file load, any `InventoryItem::Component` whose `id` is also in `content.rocket_defs` is promoted to a new `RocketState` entry in the side table (with default `build_quality = 1.0`, `reuse_count = 0`, derived `tier` from content). Fully backward-compatible via `#[serde(default)]` on the new state field.

**Lifecycle**:
- Rocket is created in the side table when (a) an assembler produces one (quality from assembler wear + RNG + tech), or (b) one is imported via trade (quality = content default).
- Rocket is removed from the side table when (a) it is launched as expendable and succeeds, (b) it is launched as reusable and recovery fails, (c) it is scrapped by the autopilot/player at quality below the scrap threshold.
- Rocket returns to the side table with `reuse_count += 1` and decayed `build_quality` after successful recovery.
- In-flight rockets move their `RocketInstanceId` onto the existing `LaunchTransitState` (which today lacks an instance field — this is the one addition).

### Decision 6: Launch reliability mechanic — B-lite (from brainstorm)

Confirmed in brainstorm. **Per-pad readiness + per-rocket build_quality + simple RNG roll against the product**, with a **separate RNG roll for recovery** on reusable flights. Not option A (pure per-launch RNG), not option C (staged distribution). This is Key Decision #1 in the origin brainstorm — see origin for full reasoning.

### Decision 7: Event effects — closed enum additions, not content-driven

Research confirmed the `EffectDef` enum in `crates/sim_core/src/sim_events.rs:184` is closed and matched exhaustively. New effects (`ModifyPadReadiness`, `ModifyNextBuildQuality`) WILL require new Rust enum variants plus match arms in `apply_single_effect` (line 618). **CLAUDE.md's "content-driven types" rule does not apply here** — effects are engine mechanics, not content categories.

Additionally, `ResolvedTarget` (sim_events.rs:143) currently doesn't cover ground facilities or pads. A new `ResolvedTarget::LaunchPad { pad_id }` variant is needed for pad-targeted effects.

### Decision 8: Tech effects — `StatModifier` wherever possible

Per the P3 tech tree expansion patterns learning: prefer adding new `StatId` entries (`LaunchSuccessRate`, `RecoveryChance`, `BuildQualityBonus`, `PadTurnaroundSpeed`, `ReadinessDecayRate`) wired through the existing `resolve_with_f32` path. Zero Rust match arms, zero effect-type additions. Only add a new `TechEffect` variant if the effect is *qualitatively new* (e.g. `EnableHumanRating`).

### Decision 9: RNG determinism — one stream, sorted iteration

All new RNG rolls (launch outcome, recovery outcome, weather volatility, build-quality variance at construction time) use the existing `ChaCha8Rng` that flows through `sim_core::tick`. No new RNG streams. Every iteration over `rockets`, `pads`, or `launches` sorts by ID (`BTreeMap` is sorted; `HashMap` is banned for RNG-consuming iteration per the past-learnings research). Rolls happen inside the existing `&mut impl Rng` borrow — no plumbing of a second RNG handle.

### Decision 10: Strategy config extension — serde-default + behavioral-equivalence test

All new `StrategyConfig` fields (listed in Technical Approach § Strategy Config) ship with `#[serde(default)]` and a `fn default_<field>()` function. A regression test asserts all new defaults match the current hardcoded autopilot behavior, so "strategy-v2 → strategy-v3" migration is non-breaking for existing save files and scenarios.

### Decision 11: Reusability tiers — content-driven newtype strings, not Rust enum

Per VIO-544 (`DataKind`) and VIO-616 (`ResearchDomain`) migration patterns: reusability tier is a content-defined category, loaded as strings from `content/rockets.json`, wrapped in a newtype for type safety. No Rust enum. Tiers today: `"expendable"`, `"partial_recovery"`, `"full_recovery"`. Future tiers added in content.

### Decision 12: Pad readiness — integer milli-units, float API

Past-learnings research flagged: prefer integer durable state over floats to avoid determinism issues from float drift. `LaunchPadState.readiness_milli: u16` stored (0–1000). Float `readiness` exposed through getter for the roll math. Same pattern for `build_quality_milli: u16` on `RocketState`.

### Decision 13: Credentials via macOS Keychain, not environment variables

**Cloud-provider API keys (OpenRouter in Phase A, any future frontier-model API) live in macOS Keychain, not `.env` files.** Reason: no leakage into shell history, `ps` output, process environment listings, or accidental git commits. Storage:

```bash
security add-generic-password -a "copilot_runtime" -s "OPENROUTER_API_KEY" -w "sk-or-..."
```

Retrieval at `copilot_runtime` startup (not per-request):

```ts
// copilot_runtime/src/credentials.ts
import { execSync } from "child_process";

export function readKeychainSecret(account: string, service: string): string {
  try {
    return execSync(
      `security find-generic-password -a "${account}" -s "${service}" -w`,
      { encoding: "utf8" }
    ).trim();
  } catch (err) {
    throw new Error(
      `Missing keychain entry: account="${account}" service="${service}". ` +
      `Install with: security add-generic-password -a "${account}" -s "${service}" -w "<value>"`
    );
  }
}

export const getOpenRouterKey = () =>
  readKeychainSecret("copilot_runtime", "OPENROUTER_API_KEY");
```

Rules:
- Read once at startup, cache in module scope. Never re-read per request.
- Fail loudly with install instructions if the keychain entry is missing.
- Ollama path skips the keychain call entirely — Ollama ignores the API key field, pass the literal string `"ollama"`.
- This is **macOS-only** by design. Project B runs exclusively on a macOS dev machine (current machine now, Mac Mini M4 Pro later). No Linux fallback needed; if portability becomes relevant later, wrap with platform detection at that point — don't prematurely abstract.

## Problem Statement

VIO-614 validated the Bayesian optimizer and surfaced a flat scoring landscape: at 50k ticks across 50 trials, composite score spread was 2.5% (672–689). The root cause, per [`docs/solutions/logic-errors/scoring-sensitivity-and-optimization-landscape.md`](../solutions/logic-errors/scoring-sensitivity-and-optimization-landscape.md), is structural:

> The pipeline's bottlenecks are structural (ore availability, refinery throughput, tech tree gates) rather than priority-driven. Strategy parameters modulate allocation within a narrow band, not the fundamental production rate.

The sim is a queue, not a game. The brainstorm also surfaced a second, larger goal: phase-specific play styles where early commitments compound forward into later phases. And the Tambo project description laid out a vision where the primary interaction layer is an LLM co-pilot that reasons about game state and proposes commands — which requires the game loop to have enough decision surface for the co-pilot to reason about in the first place.

See origin document for full problem framing.

## Proposed Solution

Project A implements the four-phase structure from the brainstorm:

- **Phase 1** — Diagnostic scenarios (R1–R3): scenario-only stress tests that decide whether Phase 2 scope can shrink.
- **Phase 2** — Unified Launch Reliability + Reusability subsystem (R4–R12): per-rocket state, per-pad readiness, launch + recovery RNG rolls, reusability tier cost progression, tech hook points, strategy config + autopilot integration. Absorbs VIO-560.
- **Phase 2.5** — Phase-specific commitment decisions (R13–R17): rocket tier commitment with switching cost, human-rating optional path, recovery investment timing as a strategy parameter.
- **Phase 3** — Event system enrichment (R18–R21): new effect variants, 4+ early-game events, 1+ asymmetric upside event.
- **Cross-cutting** — LLM co-pilot observability (R22–R25): state exposure through `sim_daemon` endpoints and `mcp_advisor` tools.

Project B implements the CopilotKit foundation:
- Sidecar Node runtime with OpenRouter → Ollama adapter swap
- Readable state selector (snapshot on pause, <4 KB JSON summary)
- Initial action set (preview launch, propose command, query state, diagnose alert)
- Approval card components for `renderAndWaitForResponse`
- MCP integration pointing at `mcp_advisor`

## Technical Approach

### Architecture Overview

```
Player <─┬─> ui_web (React 19 + Vite 7)
         │    ├─ <CopilotKit runtimeUrl="http://localhost:4000/api/copilotkit">
         │    ├─ useCopilotReadable(memoized snapshot, available=paused)
         │    ├─ useCopilotAction (proposeLaunch, scrapRocket, confirmBuildOrder)
         │    └─ approval card components (renderAndWaitForResponse)
         │           │
         │           └─ direct fetch ─> sim_daemon :3001
         │
         └─> copilot_runtime (Node + Express, :4000)
              ├─ BuiltInAgent + @ai-sdk/openai-compatible
              ├─ Phase A: OpenRouter adapter
              ├─ Phase B: Ollama adapter (localhost:11434/v1)
              └─ createMCPClient ──spawn──> mcp_advisor (stdio child process)
                                                 │
                                                 └─ HTTP ─> sim_daemon :3001
                                                              │
                                                              └─ sim_core (deterministic)
```

Key properties:
- **LLM is advisory, never authoritative.** Every command the LLM proposes goes through the same `sim_daemon` command pipeline (and therefore the same `sim_core` validation) that direct UI commands use.
- **No new transport protocol.** `sim_daemon` HTTP is the single tool surface. MCP is a transport over it. CopilotKit consumes MCP. Both Claude Code (dev) and the player co-pilot (runtime) share the same underlying tools.
- **Localhost-only by default.** No internet traffic in Phase B (except optional strategic-model API calls). Phase A traffic goes to OpenRouter, tunneled through the sidecar, not the browser.

### Project A: Rocket state model

```rust
// crates/sim_core/src/types/state.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RocketState {
    pub instance_id: RocketInstanceId,
    pub def_id: String,                     // looks up RocketDef + ReusabilityTier in content
    pub tier: ReusabilityTierId,            // content-driven newtype String
    pub build_quality_milli: u16,           // 0..=1000, durable integer state
    pub reuse_count: u16,
    pub human_rated: bool,
    pub pad_id: Option<FacilityId>,         // current pad location, or None if in transit
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RocketsState {
    #[serde(default)]
    pub instances: BTreeMap<RocketInstanceId, RocketState>,
    #[serde(default)]
    pub next_instance_id: u64,
}
```

On `GameState`: add `#[serde(default)] pub rockets: RocketsState`.

**Migration on load** (in `sim_world::load_state` or equivalent): for each `InventoryItem::Component` in any facility whose `id` matches `content.rocket_defs`, remove from inventory and insert a fresh `RocketState` into `state.rockets.instances` with `build_quality_milli: 1000` and `reuse_count: 0`. Increment `next_instance_id`. This makes existing saves load cleanly.

### Project A: Pad readiness state

```rust
// crates/sim_core/src/types/state.rs (extends existing LaunchPadState at :258-279)
pub struct LaunchPadState {
    pub available: bool,
    pub recovery_ticks_remaining: u64,
    pub launches_count: u64,
    #[serde(default = "default_readiness_milli")]
    pub readiness_milli: u16,              // 0..=1000
    #[serde(default)]
    pub readiness_modifier_ticks: u16,     // temporary event-driven modifier countdown
}

fn default_readiness_milli() -> u16 { 1000 }
```

**Readiness decay model** (runs in `tick_ground_facilities` at `station/mod.rs:280` BEFORE the proxy-station swap):
- Base passive decay per tick: driven by `content.constants.readiness_decay_per_tick_milli` (e.g. 1 milli-unit per tick ≈ full decay over ~1000 hours).
- Maintenance-driven recovery: pads with low readiness that have an operational `LaunchPad` module + sufficient crew/power apply a `readiness_maintenance_bonus` per tick.
- Event modifiers: `ModifyPadReadiness` effect applies a temporary delta + duration.
- Tech bonus: `tech_weather_forecasting` (new) reduces volatility by scaling the event-modifier amplitude.

### Project A: Launch reliability pipeline

Extends `handle_launch` in `crates/sim_core/src/commands.rs:1430`. Because `handle_launch` is already long (per past-learnings research, 5 helpers already extracted), **decomposition comes first**. Extract `resolve_launch_outcome(&mut RocketState, &LaunchPadState, &GameContent, &mut Rng) -> LaunchOutcome` as a `pub(crate)` helper callable by both the transit handler and unit tests.

**Cost-of-failure calibration** (critical for R17 — see Risk Analysis). The 8% score-spread target requires that launch failures cascade into meaningful economic and progression impact. If a failed launch costs 2% of treasury, it's noise and the optimizer will ignore it. Target: **a failed launch of the currently-most-relevant rocket tier should cost between 10% and 25% of treasury at the time the player is likely to launch it** — enough to force strategic adaptation without being unrecoverable. The real cost components on a failed launch are:

1. Rocket destroyed (full build cost — materials + cash)
2. Payload destroyed (satellites, station kits, or supplies — often more valuable than the rocket)
3. Pad wear damage (scales the next launch's readiness + maintenance burden)
4. Opportunity cost: the next launch can't happen until the pad recovers + is repaired, blocking progression
5. Optional cascade: a Critical alert that triggers a temporary `ReadinessDecayRate` bump from investigation/grounding (modeled via existing `ApplyModifier` event effect)

M3 ends with a calibration pass: pilot a few seeds, measure the treasury impact of a single failure at characteristic points in the game (first Light launch, first Medium launch, first Heavy launch). If impact is under 10%, scale up one or more of: rocket build cost, pad wear damage, or the next-launch readiness penalty. Record the final cost model in the M10 validation doc.

Command flow:
1. `Command::Launch { pad_id, rocket_instance_id, payload, destination, reusable }` received.
2. Validate pad available + rocket exists in `state.rockets` + rocket is on the named pad + tech for the rocket tier is unlocked + tech for reusable mode is unlocked if `reusable=true`.
3. Compute cost (base * reusability tier multiplier), validate balance + fuel.
4. Compute `p_success = readiness × build_quality × tech_bonus` (in milli-unit math, final value clamped to 0..=1000).
5. **Roll**: `rng.gen_range(0..1000) < p_success_milli`.
6. On failure:
   - Rocket removed from `state.rockets.instances`.
   - Payload lost (no `PayloadDelivered` event fired later).
   - Pad takes wear damage + readiness penalty.
   - Emit `LaunchFailed {}` event (empty struct form — FE dispatcher gotcha from past-learnings research).
   - Emit alert at `Severity::Critical`.
7. On success:
   - `state.counters.rockets_launched += 1`.
   - Push `LaunchTransitState { rocket_instance_id, reusable, destination, arrival_tick, ... }` onto `GroundFacilityState.launch_transits`. **New field**: `rocket_instance_id` on `LaunchTransitState`.
   - Deduct balance + fuel, mark pad recovering.
   - Emit `LaunchSucceeded {}` event (formerly `PayloadLaunched`, kept for back-compat).

### Project A: Recovery pipeline

Extends `resolve_launch_transits` at `crates/sim_core/src/engine.rs:559`. When a reusable transit resolves:
1. Deliver the payload normally (emit `PayloadDelivered`).
2. **Second roll**: `p_recovery = recovery_base × tech_bonus × pad_readiness_at_return × build_quality`.
3. On recovery success: rocket returns to `state.rockets.instances` with `reuse_count += 1` and `build_quality_milli = decay_curve(reuse_count)`. The decay curve is tuned during M10 pilot runs; starting shape is a piecewise-linear approximation of the VIO-560 real-world curve.
4. On recovery failure: rocket removed from `state.rockets.instances`. Payload already delivered — this is the design decision in the brainstorm (R8). Emit `RocketLost {}` event.
5. If `build_quality_milli < scrap_threshold_milli` after decay, the rocket is auto-scrapped (removed from side table, `RocketScrapped {}` event).

### Project A: Tech integration

New `StatId` entries in `crates/sim_core/src/stats.rs`:
- `LaunchSuccessRate` — applied as `StatModifier::PctAdditive` to the reliability roll base.
- `RecoveryChance` — same pattern for recovery roll.
- `BuildQualityBonus` — applied at rocket construction time.
- `PadTurnaroundSpeed` — scales pad recovery ticks.
- `ReadinessDecayRate` — scales base decay.

New techs added to `content/techs.json`:
- `tech_flight_computer` — `LaunchSuccessRate +10%`
- `tech_weather_forecasting` — `ReadinessDecayRate -25%`, reduces event-modifier amplitude
- `tech_refurbishment` — `RecoveryChance +15%`, reduces `build_quality` decay per reuse
- `tech_human_rating` — new `TechEffect::EnableHumanRating` variant (qualitatively new, needs enum addition)

Existing techs `tech_partial_recovery` and `tech_heavy_rocketry` extended with reliability bumps.

**Starting state audit**: grep `content/*_start.json` for any rocket whose new `required_tech` isn't in `research.unlocked`, update as needed. Extend `validate_techs()` in `sim_world/src/lib.rs` to assert every rocket's `required_tech` resolves to a real tech ID.

### Project A: Event system extensions

New `EffectDef` variants in `crates/sim_core/src/sim_events.rs:184`:
- `ModifyPadReadiness { delta_milli: i16, duration_ticks: u16 }` — pad-targeted.
- `ModifyNextBuildQuality { delta_milli: i16, expires_ticks: u16 }` — station-targeted (affects next rocket produced at station's assembler).

New `ResolvedTarget::LaunchPad { pad_id: FacilityId }` variant and `resolve_pad_target()` function.

New events in `content/events.json`:
- `evt_favorable_weather` — ground phase, targets random pad, readiness delta `+200 milli`, duration 500 ticks.
- `evt_storm_front` — ground phase, targets random pad, readiness delta `-400 milli`, duration 1000 ticks. Weight multiplier if `has_tech('tech_weather_forecasting')` is false.
- `evt_bad_propellant_batch` — ground phase, targets random station, `ModifyNextBuildQuality -150 milli`, expires 200 ticks.
- `evt_pathfinder_success` — ground phase, targets random station, `ModifyNextBuildQuality +100 milli`, expires 500 ticks.
- `evt_launch_contract` — asymmetric upside event, targets global. Fires only when conditions are already met (`has_rocket_tier_at_least: medium` AND `pad_available: true` AND `not has_active_contract`). Effect: immediate `AddInventory { item: Cash, amount: 10_000_000 }` + `ApplyModifier { stat: LaunchPayoutBonus, op: pct_additive, value: 0.5, duration_ticks: 2000 }` — a lump-sum advance plus a 50% launch payout bonus for the next window. Rewards commitment that was already made (player invested in a medium pad + rocket), without needing a new "pending objectives" / deadline-tracking system. **This is a deliberate simplification of the brainstorm's "grant with deadline" idea** — the general-purpose pending-objectives system is out of scope for this project.

All events emit via the existing alert pipeline with populated `description_template` strings so the future LLM co-pilot can narrate them.

### Project A: Strategy config additions

New fields on `StrategyConfig` in `crates/sim_core/src/types/strategy.rs:251`:
- `launch_readiness_min: f32` (default 0.5) — autopilot delays launch when pad readiness is below this threshold
- `rocket_scrap_quality_threshold: f32` (default 0.3) — autopilot scraps rockets below this quality
- `preferred_rocket_tier: String` (default `"light"`) — content ID of the primary rocket tier for fleet planning
- `human_rating_priority: f32` (default 0.0) — weight for investing in human-rating research
- `recovery_investment_priority: f32` (default 0.5) — weight for investing in recovery tech
- `weather_delay_tolerance_ticks: u16` (default 200) — max ticks the autopilot will wait for a weather window

Schema bump: `"strategy-v2"` → `"strategy-v3"`. Backward-compat deserialization test.

`StrategyInterpreter` in `sim_control/src/strategy_interpreter.rs:1` gains corresponding priority outputs. `LaunchExecution` concern (`sim_control/src/agents/ground_facility_agent/concerns/launch_execution.rs`) consumes `launch_readiness_min` and `rocket_scrap_quality_threshold` directly. A new `RocketScrap` concern is added for the scrap-decision (small, ~40 LoC).

### Project A: Observability (R22–R25)

New `sim_daemon` endpoints in `crates/sim_daemon/src/routes.rs`:
- `GET /api/v1/launch/preview?pad_id=X&rocket_instance_id=Y` → returns `{ p_success, readiness, build_quality, tech_bonus, blockers: [...] }`. This is R24: "previewable launch probability."
- Extends `GET /api/v1/snapshot` to include `state.rockets` (automatic via serde — no plumbing).
- Extends `GET /api/v1/advisor/digest` with `rocket_inventory_by_tier`, `avg_build_quality`, `launch_success_rate_recent`, `pad_readiness_by_id`.

New `MetricsSnapshot` fields (following the "8 construction sites" pattern from past-learnings research — MetricsSnapshot struct, `fixed_field_values`, `fixed_field_descriptors`, finalizer, `SummaryMetrics` struct, `SummaryMetrics::from_snapshot`, Parquet writer schema, test fixtures):
- `launches_attempted: u32`
- `launches_failed: u32`
- `rockets_recovered: u32`
- `rockets_lost: u32`
- `rockets_scrapped: u32`
- `avg_build_quality: f32` — default neutral (1.0) when no rockets exist
- `avg_pad_readiness: f32` — default neutral (1.0) when no pads exist
- `recovery_rate: f32` — `rockets_recovered / launches_attempted.max(1)`, default neutral (1.0)

Scoring extension: the `efficiency` dimension gains a `recovery_rate` blend and the `industrial_output` dimension gains a `launches_attempted`-based blend. Replaces the existing `"reusable_landings"` placeholder (per past-learnings research — this placeholder was left for VIO-560).

New MCP advisor tools in `mcp_advisor/src/index.ts`:
- `preview_launch_success` — wraps the new `/api/v1/launch/preview` endpoint.
- `list_rockets` — wraps a new `/api/v1/rockets` endpoint or reads from snapshot.
- `get_pad_readiness` — reads from snapshot.
- `suggest_reliability_config` — wraps `suggest_strategy_change` with reliability-specific field hints.

### Project B: CopilotKit Foundation

New directory `copilot_runtime/` (TypeScript Express sidecar):

```
copilot_runtime/
├── package.json
├── tsconfig.json
├── src/
│   ├── index.ts           # Express boot, bind 127.0.0.1:4000
│   ├── runtime.ts         # CopilotRuntime + BuiltInAgent setup
│   ├── adapter.ts         # env-driven factory: OpenRouter | Ollama
│   ├── mcp.ts             # createMCPClient → mcp_advisor stdio spawn
│   └── auth.ts            # shared-secret header check for localhost hardening
```

`ui_web/src/copilot/` (new):
- `CopilotProvider.tsx` — wraps app root with `<CopilotKit runtimeUrl=...>` and `<CopilotSidebar>`.
- `readables.ts` — `summarizeForLLM(state)` selector + `useSnapshotReadable` hook. **Hierarchical, not flat.** See "Readable architecture" below.
- `actions/launch.ts` — `useCopilotAction({ name: "proposeLaunch", ... })` with `renderAndWaitForResponse` approval card. Calls `GET /api/v1/launch/preview` for the stats, calls `POST /api/v1/command` to execute on approve. **Availability: disabled when !isPaused** — we don't let the LLM commit commands against stale state.
- `actions/scrap.ts` — similar pattern for scrap decisions. Also disabled when unpaused.
- `actions/query.ts` — read-only state queries that fall through to MCP tools. **Availability: enabled always** — the LLM can always answer questions from the top-level snapshot + fresh MCP tool calls.
- `ApprovalCard.tsx` — shared approval card component consumed by all `renderAndWaitForResponse` actions.

**Readable architecture — hierarchical, not flat** (revised after adversarial review):

The original "target <4 KB JSON" for one flat readable doesn't scale. A realistic late-game state with 3 stations, 15 ships, 50+ modules, 20+ rockets, a dozen active alerts, a full research tree, and strategy config will blow past 4 KB on a flat serialization. The corrected pattern is a **top-level compressed snapshot + drill-down via MCP tool calls for details**:

```ts
// Top-level snapshot (target <4 KB, always on)
useCopilotReadable({
  description: "Current game state summary. Call MCP tools for details.",
  value: {
    snapshot_tick: state.tick,
    snapshot_age_label: isPaused ? "current (paused)" : `stale as of tick ${state.tick}`,
    treasury: state.balance_usd,
    active_alerts_count: state.alerts.filter(a => a.severity !== "Info").length,
    recent_critical_alerts: state.alerts.filter(a => a.severity === "Critical").slice(-3),
    strategy: { mode, primary_rocket_tier, human_rating, recovery_investment },
    research: { unlocked_count, in_progress_domains, recent_unlock },
    stations: state.stations.map(s => ({ id: s.id, name: s.name, module_count: s.modules.length, readiness: avgReadiness(s) })),
    fleet: { ships_total: state.ships.length, in_transit: countInTransit(state), idle: countIdle(state) },
    rockets: { by_tier: tierCounts(state.rockets), avg_build_quality: avgQuality(state.rockets) },
    pads: { by_status: padStatusCounts(state) },
  },
  available: "enabled", // ALWAYS available — see below
});
```

Drill-down tool calls (via existing `mcp_advisor` tools, auto-exposed by `createMCPClient`):
- `list_stations(station_id?)` — full module inventory for a station
- `list_rockets(filter?)` — individual rockets with quality/reuse_count/tier
- `list_ships(filter?)` — fleet with full detail
- `list_alerts(severity?)` — alerts paginated
- `get_pad_readiness(pad_id?)` — readiness + decay rate
- `preview_launch_success(pad_id, rocket_instance_id)` — R24 endpoint

**Co-pilot UX when unpaused** (addresses the "available: disabled" confusion):

- Top-level readable stays `available: "enabled"` always. The payload carries an explicit `snapshot_tick` and `snapshot_age_label`.
- Query-only actions (`query_game_state`, `diagnose_alert`, `list_rockets` via MCP) are always enabled. The LLM answers questions from cached snapshot + fresh MCP calls. The system prompt says: *"This data may be stale — always cite the snapshot_tick. If the user needs live data, recommend pausing first."*
- **Command-executing actions** (`proposeLaunch`, `scrapRocket`, `retrofitPad`) are `available: "disabled"` when unpaused, with a disabled-state tooltip: *"Pause to propose commands — the co-pilot needs a stable state."*
- UI surfaces a "paused: ✓" / "running: ⟳" indicator in the sidebar header so the player can see the co-pilot mode at a glance.

**Snapshot size target (revised)**: top-level readable ≤4 KB JSON **for a realistic late-game state** (3 stations, 15 ships, 50+ modules). This is the discipline line. If late-game state blows past 4 KB in practice, aggressive-compress by moving more fields to drill-down MCP calls. M-series success criterion: a scripted session with a late-game save produces a top-level readable ≤4 KB and the LLM can still answer questions about any station/ship/rocket via MCP calls in ≤2 round-trips.

**Localhost hardening**:
- Bind everything to `127.0.0.1`, never `0.0.0.0`.
- Shared-secret header check in `copilot_runtime/src/auth.ts` before CopilotKit runtime dispatches.
- CORS: `Access-Control-Allow-Origin: http://localhost:5173` explicit.
- No CopilotKit runtime port exposed externally.

**Determinism bounds for LLM interaction**: `temperature: 0.2`, preview actions are idempotent on the daemon side, action names are short and `snake_case` to reduce tool-call hallucination rate on 7–14B models.

## Implementation Phases

### Project A — Milestones

| M | Scope | Size | Dependencies |
|---|---|---|---|
| **M0** | Diagnostic scenarios: `strategy_stress_ground.json` with scarce cash + seed-varied prices + seed-varied asteroid distances. `validate_strategy_stress.py` script. Pilot optimizer run. Decision gate recorded — see M0 outcome handling below. | S | none |
| **M1** | Rocket state model: `RocketState` + `RocketsState` types, migration on load, `handle_launch` decomposition (extract `resolve_launch_outcome`), `LaunchTransitState.rocket_instance_id` field. | M | M0 |
| **M2** | Pad readiness state: `LaunchPadState` extension (integer milli-units), base decay model in `tick_ground_facilities`, snapshot exposure. | S–M | none (parallel with M1) |
| **M3** | Launch reliability roll: `p_success` math, roll site inside `handle_launch`, `LaunchFailed {}` event + alert, FE `applyEvents.ts` handler + `ci_event_sync.sh` check. Unit + integration tests. | M | M1 + M2 |
| **M4** | Recovery roll + reusability tiers (absorbs VIO-560): second roll in `resolve_launch_transits`, `RocketLost` / `RocketRecovered` / `RocketScrapped` events, `build_quality` decay curve, content tier definitions (`partial_recovery`, `full_recovery`), per-tier cost multipliers. Integration test: multi-flight reusable rocket across 10 seeds. | L | M3 |
| **M5** | Tech chain hooks: new `StatId` entries, `tech_flight_computer` / `tech_weather_forecasting` / `tech_refurbishment` / `tech_human_rating` techs, starting state audit, `validate_techs` extension. | S | M3 (parallel with M4) |
| **M6** | Strategy config + autopilot integration: new strategy fields with `#[serde(default)]`, `strategy-v3` schema bump + backward-compat test, `StrategyInterpreter` updates, `LaunchExecution` consumption, new `RocketScrap` concern, behavioral equivalence regression test. | M | M4 |
| **M7** | Phase 2.5 commitment mechanisms: pad retrofit recipe (design spec below), `tech_human_rating` downstream effects (crewed launches + gated station construction), `preferred_rocket_tier` autopilot usage. Strategy integration tests for multiple play styles. **Requires a mini design session before implementation** — see M7 Design Spec below. | L | M6 |
| **M8** | Phase 3 event enrichment: new `EffectDef` variants, `ResolvedTarget::LaunchPad`, new event handlers, 5 new events in content, `events_enabled` scenario override. | M | M3 (parallel with M6/M7) |
| **M9** | Observability / MCP surface: `/api/v1/launch/preview` endpoint, metrics snapshot "8 construction sites" field additions, advisor digest extension, new MCP tools in `mcp_advisor/src/index.ts`. | M | M1–M4 |
| **M10** | Validation: `strategy_optimized_v2` scenario, updated `validate_strategy_comparison.py`, 50-trial optimizer run at 50k ticks. Record spread. Must achieve ≥8% target (R17). Document findings in `docs/solutions/`. | S–M | M6 + M8 + M9 |

**Critical path**: M0 → (M1 ∥ M2) → M3 → M4 → M6 → M10. Parallel streams: M5 during M3/M4, M8 during M6/M7, M9 during M4–M8. M7 is the late-critical Phase 2.5 work.

#### M0 Outcome Handling

The M0 decision gate records one of three outcomes and informs downstream scope, but **does not cancel Phase 2 or beyond**:

1. **M0 spread < 5%** — scenario changes alone are insufficient. Full Phase 2 / 2.5 / 3 scope as planned. This is the expected outcome.
2. **M0 spread 5–8%** — scenario changes help but don't hit the target. Full Phase 2 / 2.5 / 3 scope. Update M10 expected target: brainstorm's stretch ≥15% becomes the likely outcome at the tail.
3. **M0 spread ≥8%** — scenario changes alone already hit the target. **Phase 2 / 2.5 / 3 continue in full** for gameplay / Tambo-value-moment reasons (launch reliability is a decision surface the co-pilot will reason about regardless of its variance contribution), but the M10 validation criterion becomes "Phase 2–3 must not *narrow* the spread below 8% while adding gameplay depth." The 8% spread is a floor once achieved, not a ceiling.

The rationale: launch reliability + reusability + phase-specific commitments are valuable for gameplay depth and for the LLM co-pilot's decision surface, not just for optimizer variance. Cancelling them because M0 widened the spread would regret in Project B + Project C time.

#### M7 Design Spec — Pad Retrofit Mechanism

Resolved up-front so M7 can start without a mini design session. Four sub-decisions:

**1. Pads are tier-specific.** A launch pad built as `light_pad` can only launch `light` rockets. Tier commitment is physical: the pad's fuel plumbing, flame trench, and structural supports are sized for a specific rocket class.

**2. Pads can coexist.** A ground facility can host a `light_pad` and a `medium_pad` simultaneously if it has the module slots and power. This is the "honest" answer to rocket tier commitment — nothing *prevents* you from having multiple tiers, but each pad has an independent capital cost and ongoing maintenance burden. The commitment pressure comes from (a) capital efficiency (running 3 pads at 30% utilization each is worse than 1 pad at 90%) and (b) operational complexity (crew + power + maintenance scale with pad count).

**3. Retrofit is a new recipe type, not a module replacement.** A `PadRetrofit` operation runs against an existing pad module, consuming materials + cash + time, and converts it to a different tier. During retrofit the pad is `available: false` and its `readiness_milli` is frozen. Retrofit cost is **30–50% of a new pad** (rationale: scavenged structural frame, new propellant handling + control systems). Retrofit duration is **2× the vanilla pad construction time** at the target tier (rationale: conversion is harder than greenfield). Specific numbers tuned during M10 pilot.

**4. Downgrade is cheaper than upgrade.** Light → Medium retrofit costs more than Medium → Light (downgrading is structurally easier). But downgrade is rarely the right call — the plan includes it for completeness, not as a primary optimization path.

**Mechanism implementation**:
- New `LaunchPadDef` field `retrofit_targets: Vec<{ target_tier, cost, duration_ticks }>` so retrofit options are content-defined per pad tier
- New `Command::RetrofitPad { pad_id, target_def_id }` variant
- New `LaunchPadState` enum discriminant: `PadStatus::{ Idle, Recovering, Retrofitting { target_def_id, ticks_remaining } }`
- `tick_ground_facilities` advances `Retrofitting` state; on completion, swaps the pad's `def_id` to the target
- Pad-construction path unchanged — players can also just build a second, third, fourth pad at different tiers

**Strategy surface**: the new strategy field `preferred_rocket_tier` drives autopilot pad construction at the start, and drives retrofit decisions later when the autopilot detects a tier mismatch between its current primary pad and its current strategic target. The autopilot prefers *building a second pad* when budget is abundant and *retrofitting* when budget is constrained — this is an emergent tradeoff the optimizer can explore.

**Acceptance criteria addendum** (M7):
- [ ] Multiple tier-specific pads coexist at the same ground facility with no interference
- [ ] Retrofit command validates + begins transition; pad becomes unavailable until complete
- [ ] Retrofit cost + duration are content-tunable per source→target pair
- [ ] Autopilot chooses between "build new pad" and "retrofit existing" based on budget pressure
- [ ] Integration test: 10-seed run at 50k ticks shows meaningful divergence between "retrofit early (narrow focus)" and "multi-pad (diversify)" strategy configs

### Project B — Milestones

| Mb | Scope | Size | Dependencies |
|---|---|---|---|
| **Mb1** | `copilot_runtime/` scaffolding: Express sidecar, `BuiltInAgent` + `@ai-sdk/openai-compatible`, OpenRouter adapter, localhost-only binding, shared-secret auth. `<CopilotKit>` provider wired into `ui_web/`. Smoke test: one round-trip "what's the current tick?" query against the current sim. | M | none (parallel with Project A) |
| **Mb2** | `useCopilotReadable` snapshot selector: `summarizeForLLM`, memoized on tick, `available` gated on pause. First read-only actions (`query_game_state`, `diagnose_alert`). | M | Mb1 |
| **Mb3** | First command-executing action with approval card: `proposeLaunch` via `useCopilotAction({ renderAndWaitForResponse })`. `ApprovalCard.tsx` shared component. Direct `fetch` to `sim_daemon`. Integration smoke test: LLM proposes → card renders → user confirms → `POST /api/v1/command` executes → state reflects. | M | Mb2 + M9 (launch preview endpoint) |
| **Mb4** | MCP integration: `createMCPClient` in `copilot_runtime/src/mcp.ts` spawns `mcp_advisor` as a stdio child. Verify `get_metrics_digest` and `suggest_strategy_change` are callable from the LLM. Ollama adapter added, swap tested once Mac Mini hardware arrives. Migration guide written. | M | Mb3 |

**Project B critical path**: Mb1 → Mb2 → Mb3 → Mb4. Mb3 has a soft dependency on Project A's M9 (the launch-preview endpoint) for the first end-to-end demo.

### Parallelism summary

Waves, not weeks — calendar time depends on iteration cycles at M10 and is not usefully estimated up-front.

- **Wave 1**: A-M0, B-Mb1 in parallel. Independent, no shared dependencies.
- **Wave 2**: A-M1 + A-M2 in parallel (both foundational), A-M5 stub tech entries, B-Mb2 readables + first actions.
- **Wave 3**: A-M3 (reliability roll), A-M8 scaffolding begins, B-Mb3 approval card pattern (can use current sim or wait for A-M9).
- **Wave 4**: A-M4 (the big one — recovery + reusability tiers), A-M9 (observability + daemon endpoints), B-Mb4 (MCP integration).
- **Wave 5**: A-M6 (strategy integration), A-M8 tail, A-M7 (Phase 2.5 commitment).
- **Wave 6 (gating)**: A-M10 validation. Multiple iteration cycles expected — if the first run doesn't hit the 8% spread target, tune cost-of-failure model, event rates, or decay curves and rerun. Each iteration is ~1 hour of sim + analysis time. Budget for ≥3 iteration cycles.

**Note on time estimates**: Per CLAUDE.md, no calendar estimates. Calendar time will exceed raw dev time because M10 gating requires iteration cycles between sim runs, and Project B's Phase B depends on external hardware arrival (Mac Mini M4 Pro).

## Alternative Approaches Considered

1. **Extend `InventoryItem::Component` with per-instance metadata** instead of a side table. Rejected because it breaks the inventory contract for every other consumer (UI, recipes, trade, metrics). See Decision 5.
2. **CopilotKit runtime embedded in Vite dev server middleware.** Rejected because Vite HMR plus CopilotKit's stateful runtime is fragile, and the 2026 canonical pattern is a separate Node process.
3. **Use Tambo instead of CopilotKit.** Rejected because (a) user clarified one-or-the-other, (b) CopilotKit has mature MCP support (decisive), (c) approval card UX is idiomatic in CopilotKit via `renderAndWaitForResponse`, (d) docs + community patterns for state-rich apps are deeper.
4. **Implement launch reliability as pure per-launch RNG (option A from brainstorm)** instead of B-lite with pad readiness state. Rejected in brainstorm — B-lite gives the autopilot observable state to plan against and lets events modulate a durable variable.
5. **Recovery as deterministic once tech unlocked (option 1 from brainstorm)** instead of a second RNG roll. Rejected in brainstorm — deterministic recovery makes reusability a free cost reduction.
6. **New RNG stream per mechanic** (one for launch, one for recovery, one for weather). Rejected — determinism is harder to audit with multiple streams. Single `ChaCha8Rng` with sorted iteration covers all.
7. **Run optimizer for much longer (100k+ ticks) as the primary fix** instead of adding mechanics. Rejected in origin brainstorm — diminishing returns confirmed by prior VIO-614 findings.
8. **Bundle Project B (CopilotKit) into Project A.** Rejected — different languages, different rhythms, false-dependency cost, see Project Structure Decision.

## System-Wide Impact

### Interaction Graph

A `Command::Launch` triggers:
1. `handle_launch` validates pad + rocket + tech + balance + fuel
2. Reads `state.rockets[instance_id]` for `build_quality`
3. Reads `pad.readiness` from `LaunchPadState`
4. Resolves `tech_bonus` via `StatModifier` stack
5. Calls `resolve_launch_outcome(&mut RocketState, &LaunchPadState, &GameContent, &mut rng)` → `LaunchOutcome::{Success, Failure}`
6. On Success: deducts balance + fuel, marks pad recovering, pushes `LaunchTransitState { rocket_instance_id, ... }`, emits `LaunchSucceeded` event
7. On Failure: removes rocket from side table, damages pad (wear + readiness penalty), emits `LaunchFailed` event + Critical alert
8. Alert flows through `sim_daemon` SSE stream → `ui_web` alerts panel
9. Alert is captured by `AlertEngine` for subsequent analytics digest
10. On Success, `resolve_launch_transits` fires at `arrival_tick`: delivers payload, then (if reusable) rolls recovery, which itself may return the rocket to the side table or remove it

At tick+1, `evaluate_events` may fire `evt_storm_front` which targets a pad via `ResolvedTarget::LaunchPad`, pushes a temporary readiness modifier, which affects the next `handle_launch` roll.

At every strategy evaluation interval, `StrategyInterpreter` recomputes `ConcernPriorities` including new reliability-aware priorities, which feeds `LaunchExecution` + `RocketScrap` concerns.

### Error Propagation

- `handle_launch` invalid inputs (missing rocket, insufficient balance, etc.) → return `CommandError` → `apply_commands` logs + emits `CommandFailed` event. Existing pattern.
- `LaunchFailed` is NOT an error — it's a successful command with a failed outcome. Distinguishing these matters for metrics: a validation failure doesn't count as `launches_attempted`, only a roll does.
- Recovery failures: `resolve_launch_transits` already runs after tick delivery; no error path change. Emit `RocketLost` as an ordinary event.
- CopilotKit action errors: return `"error: <reason>"` string rather than throwing, per research — the LLM can reason about the error on its next turn.

### State Lifecycle Risks

- **Orphaned rockets**: If `handle_launch` partially updates state before failing (deducted balance but didn't push transit), we could leave a phantom paid launch. Mitigation: all state mutations happen AFTER the roll + post-roll validation passes; any prior failure path uses `return Err(...)` before mutation.
- **Lost rockets**: a rocket in `LaunchTransitState` whose `rocket_instance_id` refers to a non-existent `RocketState` — can happen if save/load races. Mitigation: on load, validate every transit's rocket_instance_id resolves; warn + skip transit if not.
- **Readiness drift in edge cases**: passive decay runs every tick but event modifiers have their own duration timers. If an event modifier's duration is miscomputed, readiness could underflow or stick. Mitigation: clamp to 0..=1000 after every mutation, unit tests for boundary cases.
- **CopilotKit action mid-flight during state change**: the LLM might be reasoning against a stale snapshot because the sim was paused, then the player unpauses. Mitigation: `useCopilotReadable` has `available: "disabled"` when not paused, and the approval card sends a fresh `GET /api/v1/launch/preview` at render time rather than trusting the snapshot.

### API Surface Parity

Three consumer surfaces must be updated in lockstep:
1. **HTTP API** (`sim_daemon/src/routes.rs`) — new `/api/v1/launch/preview`, extended `/api/v1/snapshot` + `/api/v1/advisor/digest`
2. **MCP tools** (`mcp_advisor/src/index.ts`) — `preview_launch_success`, `list_rockets`, `get_pad_readiness`, `suggest_reliability_config`
3. **CopilotKit actions** (`ui_web/src/copilot/actions/*.ts`) — `proposeLaunch`, `scrapRocket`, action-level preview fetching

Tests must prove all three return consistent data for the same query.

### Integration Test Scenarios

1. **Full-arc reliability progression** — ground_start, 50k ticks, 10 seeds. Asserts: launches happen, some fail, some rockets reused, at least one scrapped, avg_build_quality in a sane range, score spread ≥8% across StrategyConfig variations (R17).
2. **Strategy-driven play-style differentiation** — fork on `preferred_rocket_tier` + `human_rating_priority` + `recovery_investment_priority`, run each fork 10 seeds, assert final inventory / research state diverges materially. This is the R17 test, direct evidence that early commitments compound forward.
3. **Event-modulated launch timing** — inject a sequence of `evt_storm_front` events, assert autopilot delays launches when readiness drops below `launch_readiness_min`. No deterministic-order assertion, just count: ≥X delays observed across 5 seeds.
4. **Reuse curve validation** — scripted launch sequence (fresh rocket → launch → recover → launch → recover → …) asserts `build_quality_milli` follows the decay curve and auto-scraps at the expected reuse count.
5. **Save/load round-trip with side-table rockets** — create state with 10 rockets in the side table (mix of tiers, reuse counts, qualities), serialize to JSON, parse, assert exact equality. Plus a migration test from a saved state without the side table.
6. **CopilotKit end-to-end** (Project B): scripted LLM call → `proposeLaunch` → approval card rendered with correct stats → simulated click → `POST /api/v1/command` → state reflects. E2E Playwright test in `e2e/`.

## Acceptance Criteria

Mapped to origin R-numbers. Every requirement from the brainstorm must land.

### Phase 1 — Diagnostic (R1–R3)

- [ ] `scenarios/strategy_stress_ground.json` exists, runs ground_start at 50k ticks × 20 seeds, with scarce starting balance and seed-varied prices + distances
- [ ] `scripts/analysis/validate_strategy_stress.py` reports composite score spread (min/max/stddev) and compares to `strategy_default` baseline
- [ ] Decision gate recorded: whether scenario changes alone widen spread to ≥8%

### Phase 2 — Launch reliability + reusability (R4–R12)

- [ ] `RocketState` + `RocketsState` types exist on `GameState`, with `#[serde(default)]`, backward-compat migration on load
- [ ] `LaunchPadState` extended with `readiness_milli` (integer 0..=1000) + decay model in `tick_ground_facilities`
- [ ] `handle_launch` decomposed with `resolve_launch_outcome` helper
- [ ] Launch roll: `p_success = readiness × build_quality × tech_bonus`, sorted-ID iteration, deterministic
- [ ] `LaunchFailed {}` / `LaunchSucceeded {}` events (empty struct form) with FE `applyEvents.ts` handlers
- [ ] Recovery roll on reusable flights, second independent roll, failed recovery keeps payload but loses booster
- [ ] `RocketLost {}` / `RocketRecovered {}` / `RocketScrapped {}` events with FE handlers
- [ ] `build_quality` decay curve tuned against VIO-560 real-world reference data
- [ ] Tech effects via `StatModifier`: `LaunchSuccessRate`, `RecoveryChance`, `BuildQualityBonus`, `PadTurnaroundSpeed`, `ReadinessDecayRate`
- [ ] Reusability tier cost progression: expendable 100% → partial ~40% → full ~30%
- [ ] Autopilot consumes `launch_readiness_min` + `rocket_scrap_quality_threshold`
- [ ] `RocketScrap` concern added

### Phase 2.5 — Commitment decisions (R13–R17)

- [ ] Rocket tier switching cost: pad retrofit recipe (chosen from 3 candidates during planning)
- [ ] `tech_human_rating` + downstream effects (crewed launches gated, station construction path)
- [ ] `recovery_investment_priority` strategy field drives recovery tech unlock timing
- [ ] New strategy fields: `preferred_rocket_tier`, `human_rating_priority`, `recovery_investment_priority`, `weather_delay_tolerance_ticks`
- [ ] Integration test: ≥2 materially different winning configs at 50k ticks / 10 seeds (R17)

### Phase 3 — Event enrichment (R18–R21)

- [ ] `EffectDef::ModifyPadReadiness` + `EffectDef::ModifyNextBuildQuality` variants with match arms in `apply_single_effect`
- [ ] `ResolvedTarget::LaunchPad { pad_id }` variant + resolver
- [ ] 4 new events: `evt_favorable_weather`, `evt_storm_front`, `evt_bad_propellant_batch`, `evt_pathfinder_success`
- [ ] 1 asymmetric upside event: `evt_launch_grant`
- [ ] `events_enabled: false` scenario override tested
- [ ] All event outcomes flow through existing Event/alert pipeline (R21)

### Cross-cutting — Observability (R22–R25)

- [ ] All new state exposed through `/api/v1/snapshot` (automatic via serde)
- [ ] `GET /api/v1/launch/preview` endpoint returns `{ p_success, readiness, build_quality, tech_bonus, blockers }` (R24)
- [ ] Metrics snapshot extended with reliability fields at all 8 construction sites
- [ ] Advisor digest includes `rocket_inventory_by_tier`, `avg_build_quality`, `launch_success_rate_recent`, `pad_readiness_by_id`
- [ ] MCP advisor tools: `preview_launch_success`, `list_rockets`, `get_pad_readiness`
- [ ] Launch failure events emit structured alerts with `description_template` populated (R23)

### Project B — CopilotKit Foundation

- [ ] `copilot_runtime/` sidecar exists, runs on `:4000`, bound to `127.0.0.1`
- [ ] OpenRouter adapter working (Phase A)
- [ ] Ollama adapter working (Phase B — gated on Mac Mini hardware arrival)
- [ ] Env-var adapter swap (`LLM_PROVIDER=openrouter|ollama`)
- [ ] `<CopilotKit>` provider in `ui_web` with `CopilotSidebar`
- [ ] `useCopilotReadable` with memoized `summarizeForLLM` selector, <4 KB JSON payload, `available: disabled` when unpaused
- [ ] At least 3 actions shipped: `query_game_state`, `proposeLaunch` (approval card), `scrapRocket`
- [ ] Shared `ApprovalCard.tsx` component
- [ ] `createMCPClient` integration — CopilotKit consumes `mcp_advisor` stdio tools
- [ ] Localhost-only binding + shared-secret auth in runtime
- [ ] E2E smoke test: LLM proposes → approval card renders → confirm → command executes → state reflects
- [ ] Migration guide from OpenRouter to Ollama documented

## Success Metrics

- **R17 (primary)**: Composite score spread across 20 seeds at 50k ticks widens from the 2.5% baseline to **≥8%** on the new scenarios. Stretch: ≥15%.
- **R17 (secondary)**: Bayesian optimizer surfaces **≥2 materially different winning configs** reflecting different play styles (e.g. light/expendable/no-crew vs medium/human-rated/early-recovery), both beating the naive baseline.
- **Adaptive pressure**: At least one strategy beats the default in seeds with early launch failures *and* underperforms it in clean-start seeds.
- **Phase 1 shipped before Phase 2 code**: Decision gate recorded before M3 begins.
- **Project B end-to-end**: LLM successfully round-trips a `proposeLaunch` with approval card in ≤3 seconds at paused player time, against OpenRouter in Phase A.
- **Phase B local latency** (when hardware arrives): same round-trip ≤3 seconds against local Ollama + Qwen2.5-14B Q4_K_M on Mac Mini M4 Pro.
- **Tool-call reliability**: across a scripted 10-interaction session, ≥8 actions fire correctly (name + parameter schema valid) on the primary local model.

## Dependencies & Prerequisites

- **VIO-560 rolled into this project** from P2: Ground Operations & Telescopes. Re-parent ticket.
- **ADR-4 is aspirational**: the current `InventoryItem::Component` variant has no metadata slot. This plan resolves the gap via a side table (Decision 5). Surface ADR-4 as a first-class ADR document in a follow-up ticket, or link it from the plan.
- **Existing rocketry tech chain** (`tech_basic_rocketry`, `tech_medium_rocketry`, `tech_heavy_rocketry`, `tech_partial_recovery`) stays intact and gets extended with reliability bumps.
- **VIO-614 Bayesian optimizer** is functional and reused as-is. No optimizer changes.
- **Mac Mini M4 Pro hardware** (~2 months out per Tambo project description) arrives during the project. If it slips, Phase B of Project B extends; Phase A (OpenRouter) remains functional.
- **Ollama + `qwen2.5:14b-instruct`** are the assumed local stack. Fallback path: MLX via `mlx-lm.server`.
- **CopilotKit 2026 packages**: `@copilotkit/react-core`, `@copilotkit/react-ui`, `@copilotkit/runtime`, plus `@ai-sdk/openai-compatible`. React 19 + Vite 7 + TS 5 compatibility confirmed.
- **OpenRouter API key** for Phase A. Store in `.env`, never commit.

## Risk Analysis & Mitigation

| Risk | Likelihood | Severity | Mitigation |
|---|---|---|---|
| **R17 8% spread target missed — launch-only variance insufficient** | **High** | **High** | **Primary risk surfaced in review.** Variance from this project concentrates in launch operations. If the rest of the sim stays structurally deterministic, spread may plateau at 5–6%. Mitigations: (a) cost-of-failure calibration pass at M3 to ensure failures cascade (10–25% of treasury target); (b) M10 iteration budget of ≥3 runs to tune the cost model + event rates; (c) if still muted after tuning, the 8% target is formally downgraded to "highest achievable with this scope" and a follow-up brainstorm considers branching tech tradeoffs (the #3 approach explicitly deferred from origin brainstorm) |
| Side-table migration breaks existing save files | Medium | High | Comprehensive backward-compat test suite, `#[serde(default)]` on every field, explicit migration function with logging |
| Launch reliability math produces score distributions the optimizer can't handle (NaN, divide-by-zero) | Low | High | Neutral defaults (1.0) when counters are zero; clamp every milli-unit to `0..=1000`; integration test with zero-launch seeds |
| `handle_launch` grows unmaintainable after reliability additions | Medium | Medium | Decomposition happens FIRST (M1), never use `#[allow(clippy::too_many_lines)]` — past-learnings discipline |
| 8-construction-sites pattern missed, Parquet writer breaks silently | Medium | Medium | Exhaustive `test_empty_state_all_zeros` test failure will catch it loudly per past solutions |
| **CopilotKit version churn — fast-moving OSS, MCP integration young (Jan 2026)** | **Medium** | **Medium** | **Surfaced in review.** Pin `@copilotkit/*` versions explicitly in `copilot_runtime/package.json` and `ui_web/package.json` — no `^`, no auto-upgrade. MCP integration specifically shipped January 2026; treat its API as unstable and wrap in a thin adapter so upstream changes only touch one file. Keep a fallback path: direct `useCopilotAction` wrappers for critical tools if MCP integration breaks |
| CopilotKit tool-schema simplification bug (#2220) strips action params | Medium | Medium | Keep action parameters flat, validate on daemon side regardless, add catch-all action during dev to surface bad calls |
| Local Ollama tool-call hallucinations on 14B model | Medium | Medium | Temperature 0.2, structured output mode via Ollama's native `format` field, short snake_case action names, `OLLAMA_KEEP_ALIVE=30m` |
| Mac Mini hardware slips → Phase B delayed | Medium | Low | Phase A (OpenRouter) is fully functional; Phase B is a flip of one env var |
| Phase 2.5 commitment mechanisms don't produce variance at 50k ticks | Medium | High | M10 pilot run on shorter horizons first; if variance is muted, escalate to 100k ticks; if still muted, flag to user as a design finding, not a plan failure |
| Weather event modifiers drift pad readiness outside `[0, 1000]` | Low | Medium | Clamp after every mutation, unit test for underflow/overflow |
| Reusability tier strings collide with existing content IDs | Low | Low | Prefix with `tier_` in content, validate uniqueness in `sim_world::load_content` |
| CopilotKit MCP integration immature, breaks intermittently | Medium | Medium | Build direct `useCopilotAction` wrappers for critical tools as a fallback path; MCP is preferred but not required for Mb3 |
| RNG determinism regression from new roll sites | Low | High | Determinism regression test at M3: same seed → identical `rockets_recovered` counter after 10k ticks |
| **Late-game `useCopilotReadable` payload exceeds 4 KB target** | Medium | Low | **Surfaced in review.** Hierarchical snapshot (top-level <4 KB + drill-down via MCP tool calls). If top-level still exceeds 4 KB at realistic late-game state, move per-station module details, per-ship fuel/cargo, and individual rocket records to MCP tools. See "Readable architecture" in Project B technical approach |

## Resource Requirements

- **Solo developer** (user) plus Claude Code assistance.
- **Time estimate — NOT provided** per user preference (CLAUDE.md: avoid time estimates).
- **Compute**:
  - Current machine for Phase A CopilotKit + OpenRouter API calls.
  - Mac Mini M4 Pro 48GB arriving ~2 months out for Phase B local inference.
- **External services**:
  - OpenRouter API (Phase A only, minimal spend).
  - Optional fallback to a frontier model API for strategic queries in Phase C.

## Future Considerations

- **Project C — Tier Progression**: the brainstorm explicitly deferred this. After P5 ships, build a new brainstorm + plan for AI-research-gated co-pilot tiers, sophisticated multi-step planning, autonomous operation of routine tasks, strategic-director-level reasoning.
- **Rename the Linear project**: the "Tambo UI & LLM Game Interface" project in Linear was named before the framework decision. Now that CopilotKit is committed, rename to "CopilotKit UI & LLM Game Interface" (or similar) to avoid future confusion. Low priority but worth a single edit.
- **Strict branching tech tradeoffs**: also deferred from the brainstorm. This project adds switching costs, not forbidden alternatives. If M10 validation shows the spread is still insufficient, escalate to a branching brainstorm.
- **Multi-civ / AI opponents**: per the Tambo project description, the same CopilotKit surface the player uses can eventually be driven by an opponent LLM agent. This project's decision to keep the LLM advisory-not-authoritative is what makes that possible. No code in this project, but a structural prerequisite.
- **MLX / llama.cpp swap**: if Ollama hits a wall on latency or reliability, MLX is the documented fallback. All of Project B's adapter code is env-var-driven so the swap is trivial.
- **Voice interface**: open question in the Tambo project description. Not in scope here but not blocked.
- **Approval card library**: `ApprovalCard.tsx` starts as a single shared component. If Project C adds 10+ interaction patterns, consider extracting a small approval-card design system.

## Documentation Plan

- **ADR write-up**: Surface ADR-4 from the P2 plan into a first-class `docs/adrs/0004-rockets-as-identity-bearing-state.md` explaining the side-table decision and its rationale.
- **docs/reference.md updates**: New `RocketState`, `LaunchPadState.readiness_milli`, new strategy fields, new event effects. Per CLAUDE.md rule: if this project changes a type or tick ordering, update `reference.md`.
- **docs/BALANCE.md updates**: Append an M10 section with the new score spread numbers and which strategies won.
- **docs/solutions/**: At M10, write a new solutions doc capturing the final score-spread outcome and any gotchas discovered during M3/M4/M9 (the high-risk milestones).
- **docs/workflow.md**: Update the CI gate list if any new scripts are added (validate_strategy_stress.py, validate_strategy_comparison.py v2).
- **CopilotKit migration guide**: A new `copilot_runtime/README.md` with the OpenRouter → Ollama swap steps, model selection rationale, and the localhost hardening checklist.
- **`content/knowledge/playbook.md`**: Append a "Reliability & Reusability" section after M10 with observed strategy patterns from the optimizer run.
- **Tambo value moments catalog**: the brainstorm's "LLM Co-Pilot Value Moments" section becomes the source for the initial `useCopilotAction` set in Mb3. Those moments are now concrete targets, not hypotheticals.

## Sources & References

### Origin

- **Origin document**: [`docs/brainstorms/2026-04-11-sim-optimization-variance-requirements.md`](../brainstorms/2026-04-11-sim-optimization-variance-requirements.md) — all requirements R1–R25, phase structure, key decisions (B-lite reliability mechanic, recovery as second roll, VIO-560 absorption, forward-compat observability), LLM co-pilot value moments catalog.

Key decisions carried forward from origin:
- B-lite reliability (readiness × build_quality × tech_bonus, single RNG roll)
- Recovery as separate roll, failed recovery keeps payload
- VIO-560 reusability absorbed into this project
- Forward-compat observability via sim_daemon HTTP + mcp_advisor
- Early commitments compound forward (Phase 2.5)

### Internal References (current code, from repo-research-analyst)

- `RocketDef` content-only: `crates/sim_core/src/types/content.rs:522-536`
- `Command::Launch` variant: `crates/sim_core/src/types/commands.rs:138-143`
- `handle_launch`: `crates/sim_core/src/commands.rs:1430-1547` (target for decomposition)
- `LaunchPadState` (current): `crates/sim_core/src/types/state.rs:258-279` (extension target)
- `LaunchTransitState`: `crates/sim_core/src/types/state.rs:614-624` (needs `rocket_instance_id` field)
- `LaunchExecution` concern: `crates/sim_control/src/agents/ground_facility_agent/concerns/launch_execution.rs`
- `StrategyConfig`: `crates/sim_core/src/types/strategy.rs:251-320`
- `EffectDef` (event effects, closed enum): `crates/sim_core/src/sim_events.rs:184-211`
- `apply_single_effect` (effect dispatcher): `crates/sim_core/src/sim_events.rs:618-688`
- `ResolvedTarget`: `crates/sim_core/src/sim_events.rs:143`
- `FacilityCore`: `crates/sim_core/src/types/state.rs:540-574`
- `tick_ground_facilities` (proxy station pattern): `crates/sim_core/src/station/mod.rs:280-310`
- `MetricsSnapshot`: `crates/sim_core/src/metrics.rs:89-182`
- `sim_daemon` routes: `crates/sim_daemon/src/routes.rs:42-65`
- `mcp_advisor` tool surface: `mcp_advisor/src/index.ts` (14 existing tools)
- ADR-4 (buried): `docs/plans/2026-03-30-003-feat-p2-ground-operations-launch-system-plan.md:269`

### Past Learnings (from learnings-researcher)

- **Scoring landscape flatness**: [`docs/solutions/logic-errors/scoring-sensitivity-and-optimization-landscape.md`](../solutions/logic-errors/scoring-sensitivity-and-optimization-landscape.md) — the direct motivator; the "8 construction sites" pattern; ground_start + 50k ticks + 10 seeds rule
- **RNG determinism**: `docs/solutions/logic-errors/deterministic-integer-arithmetic.md` — sorted-ID iteration, single RNG stream, integer-preferred durable state
- **Event sync enforcement**: `docs/solutions/integration-issues/event-sync-enforcement.md` — `LaunchFailed {}` empty struct form, NEVER unit variant, FE `applyEvents.ts` sync
- **Tech tree expansion patterns**: `docs/solutions/patterns/p3-tech-tree-expansion-patterns.md` — prefer `StatModifier` over new `TechEffect` variants; autopilot tech-gate filter; starting state audit; ghost tech validation
- **Strategy consumption wiring**: `docs/solutions/patterns/strategy-consumption-wiring-patterns.md` — numeric thresholds on `StrategyConfig`, content IDs on `content.autopilot`, behavioral equivalence regression test, causal integration tests
- **Multi-ticket satellite system**: `docs/solutions/patterns/multi-ticket-satellite-system-implementation.md` — pad availability check order, 3-file component registration, MetricsSnapshot field propagation (~6 constructor sites), `handle_launch` decomposition discipline, shared outcome helper, scoring-weight rebalance with neutral defaults
- **Backward-compatible type evolution**: `docs/solutions/integration-issues/backward-compatible-type-evolution.md` — `#[serde(default)]` on every new field, backward-compat deserialization test per field
- **Entity type coverage in metrics/milestones**: `docs/solutions/patterns/extending-entity-type-coverage-in-metrics-and-milestones.md` — event-driven counters (not `state.launches.len()`), `reusable_landings` placeholder exists and is replaced by this project, dynamic normalization with neutral defaults, content-aware counter resolution

### External References (from best-practices-researcher + framework-docs-researcher)

- **CopilotKit docs** — https://docs.copilotkit.ai (BuiltInAgent, useCopilotAction, useCopilotReadable, MCP integration, Node endpoint deployment)
- **CopilotKit generative UI guide 2026** — https://www.copilotkit.ai/blog/the-developer-s-guide-to-generative-ui-in-2026 (renderAndWaitForResponse pattern)
- **CopilotKit MCP Apps spec** — https://docs.copilotkit.ai/learn/generative-ui/specs/mcp-apps
- **CopilotKit + Ollama example** — https://github.com/supercjy009/CopilotKit-Ollama (canonical integration pattern)
- **CopilotKit + AG-UI MCP integration** — https://www.copilotkit.ai/blog/bring-mcp-apps-into-your-own-app-with-copilotkit-and-ag-ui
- **Ollama OpenAI compatibility** — https://docs.ollama.com/api/openai-compatibility
- **Ollama structured outputs** — https://ollama.com/blog/structured-outputs
- **Ollama model library + Qwen tool calling** — https://qwen.readthedocs.io/en/latest/framework/function_call.html
- **MLX vs Ollama on Apple Silicon** — https://insiderllm.com/guides/qwen35-mac-mlx-vs-ollama/
- **Best Ollama models 2026** — https://www.morphllm.com/best-ollama-models
- **Known CopilotKit issues**:
  - Tool schema simplification: https://github.com/CopilotKit/CopilotKit/issues/2220
  - Tool schema missing: https://github.com/CopilotKit/CopilotKit/issues/2061
  - Ollama feature request: https://github.com/CopilotKit/CopilotKit/issues/1797

### Related Work

- **VIO-560** (absorbed): Reusability system — expendable to full recovery progression. Re-parent from P2: Ground Operations & Telescopes to this project.
- **VIO-614** (validator): P6 sim_bench scenarios + strategy optimization validation — produced the flat-landscape finding that motivated this project.
- **Tambo UI & LLM Game Interface** project (Linear, Backlog) — provides the long-horizon vision for Project C (tier progression). Not implemented in this plan.
