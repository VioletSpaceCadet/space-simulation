---
date: 2026-04-11
topic: sim-optimization-variance
---

# Sim Optimization Variance — Creating Real Strategy Space

## Problem Frame

VIO-614 validated the Bayesian optimization pipeline and surfaced a diagnosis: at 50k ticks across 50 trials, composite score spread was only **2.5%** (672–689). The best config delivered +0.9% with 70% lower variance — technically a win, but well within noise for practical purposes.

The root cause (captured in `docs/solutions/logic-errors/scoring-sensitivity-and-optimization-landscape.md`) is not the optimizer. It's the sim:

> "The pipeline's bottlenecks are structural (ore availability, refinery throughput, tech tree gates) rather than priority-driven. Strategy parameters modulate allocation within a narrow band, not the fundamental production rate."

Concretely: the tech tree is mostly linear, the production DAG is narrow (Fe-dominated), starting balance ($1B) removes budget pressure, the event system is cosmetic, fleet sizing is a monotonic dial, and scoring is a single-optimum composite. Most reasonable strategies produce similar outcomes because **resources aren't scarce, time isn't pressured, and nothing forces commitment**. The sim is a queue, not a game.

This brainstorm scopes a package that injects real strategy space — starting with the cheapest diagnostic and moving to the smallest content changes that create adaptive pressure.

The deeper goal that emerged in discussion: **the sim should surface phase-specific play styles where early commitments compound forward into later phases.** Early game is home-base manufacturing, research, and key launch decisions (rocket size, human-rating, recovery investment). Those decisions should durably shape what the mid and late game look like — not as mutually exclusive branches, but as capital lock-in, switching costs, and opportunity costs that make "who you became in the early game" matter when you're running an industrial belt empire. Full branching tech tradeoffs (strict mutually-exclusive paths) remain deferred to a separate brainstorm; this project adds the lighter, naturally emerging commitment pressure at phase transitions.

**Forward compatibility with the LLM co-pilot vision.** The [Tambo UI & LLM Game Interface](https://linear.app/violetspacecadet/project/tambo-ui-and-llm-game-interface-ed183dd0ce6d) project (likely implemented via Copilot Kit, starting with OpenRouter, moving to local inference on a Mac Mini M4 Pro) describes the primary interaction layer as an LLM co-pilot that reasons about game state and proposes commands through approval cards. That project is not in scope to *build* here, but every decision we make in this brainstorm should be legible enough for it to reason about later. The Tambo project explicitly mandates "track moments where an assistant would add value" during P1–P5 — the launch reliability / reusability / commitment subsystem is a rich source of such moments, and this brainstorm catalogs them (see *LLM Co-Pilot Value Moments* below). Concretely: new state must flow through the existing `sim_daemon` metrics / events / alerts pipeline and the `balance-advisor` MCP surface. The LLM is advisory, not authoritative — it generates commands that go through the same validation as everything else — so our job is to make sure the state it'll eventually reason about is queryable, not hidden inside `sim_core`.

## Requirements

### Phase 1 — Diagnostic scenarios (scenario-only, no code)

- **R1.** A `strategy_stress_ground` sim_bench scenario runs `ground_start`, 50k ticks, ≥20 seeds, with starting balance reduced materially below the current $1B (target range to tune during planning: $50M–$200M), seed-varied starting element prices (±20–30%), and seed-varied asteroid distance distributions (±20%).
- **R2.** A validation script reports composite score spread (min/max/stddev) and compares against the `strategy_default` baseline. Spread must be reproducible across runs.
- **R3.** Results of R1/R2 decide how much of Phase 2 is needed. If spread widens to ≥8% with scenario changes alone, Phase 2 content scope can shrink.

### Phase 2 — Unified Launch Reliability + Reusability Subsystem

This absorbs **VIO-560** (currently in `P2: Ground Operations & Telescopes`, re-parent into this project). Reliability (new) and reusability (existing ticket) share state and tech gates.

- **R4.** Rockets become per-instance inventory items with metadata: `build_quality` (0–1), `wear` / `reuse_count`, and reusability tier (expendable / partial / full). Confirm alignment with ADR-4 during planning.
- **R5.** Ground launch pads carry a `readiness` state (0–1) that decays from weather/time and rises from maintenance. Readiness is observable to the autopilot and to the UI.
- **R6.** Launch success is a deterministic RNG roll at launch time against `readiness × build_quality × tech_bonus`, seeded per-flight.
- **R7.** Launch failure destroys the rocket and its payload, damages the pad (wear spike), and emits a `LaunchFailed` event + alert. The autopilot is expected to respond to sustained failures by scrapping worn boosters or delaying launches.
- **R8.** Reusable flights (partial / full recovery tiers) make a **second** independent reliability roll at recovery time. Failed recovery loses the booster but the **payload still arrives** (stages separated successfully). This is the design choice that makes reusability a real risk/reward curve rather than a free cost reduction.
- **R9.** `build_quality` degrades as a function of `reuse_count` along a tuned curve (specific shape deferred to planning; real-world reference is in VIO-560: 45% savings by flight 2, 80% by flight 5, 85–90% by flight 10+). Once quality drops below a scrap threshold, the booster cannot launch and must be decommissioned.
- **R10.** Tech unlocks on the existing rocketry chain bump reliability: baseline bumps from `tech_basic_rocketry` / `tech_medium_rocketry` / `tech_heavy_rocketry`; recovery probability bumps from `tech_partial_recovery` and a new `tech_refurbishment` (or equivalent); weather-related readiness volatility reduction from a new `tech_weather_forecasting` (or equivalent). Concrete tech list and effect magnitudes are a planning-phase decision, but the **hook points** must exist.
- **R11.** Reusability tiers and cost progression match VIO-560 acceptance criteria: expendable (100% cost), partial recovery (~40% cost after tech, per VIO-560), full recovery (~30% cost after tech). Cost reductions compose multiplicatively with per-flight reliability rolls — a worn reusable booster is cheap but risky.
- **R12.** The autopilot can observe pad readiness and rocket quality and has at least two new decision levers: (a) delay a launch when readiness is below a strategy-configured threshold, (b) scrap a booster when quality falls below a strategy-configured threshold. These are exposed via `strategy.json` so the optimizer can tune them.

### Phase 2.5 — Phase-specific commitment decisions

These requirements make the early-game decision space distinct from the mid and late game, and give the player / autopilot commitments that matter downstream. They extend the reliability/reusability subsystem rather than being a separate layer.

- **R13.** **Rocket tier commitment has lasting implications.** Choosing `rocket_light` vs `rocket_medium` vs `rocket_heavy` as the *primary* launcher shapes pad infrastructure, fuel logistics, per-launch reliability curves, and payload economics. Switching primary tier later should incur a real cost — e.g. a new pad build-out, propellant system changes, or a period of reduced launch cadence — not be free. Exact mechanism is a planning decision; the requirement is that **"I committed to light rockets and now I need to lift a station module" is a materially different situation than "I committed to heavy from the start."**
- **R14.** **Human-rating is an optional early-game tech investment** that unlocks a distinct downstream path (crewed launches → crewed station construction → on-orbit research bonuses) while adding reliability floors, cost overhead, and failure-consequence severity (human-rated failures are catastrophic events, not just material losses). The autopilot can choose to skip it entirely and pursue an all-uncrewed strategy. Concrete tech id and effect magnitudes are a planning decision.
- **R15.** **Recovery investment timing is itself an optimizable commitment.** Unlocking `tech_partial_recovery` / `tech_refurbishment` early reduces per-launch cost long-term but diverts research and capital away from early expansion. Unlocking late preserves early capital for growth but burns more cash on expendable flights. The optimizer should be able to find "aggressive early recovery" and "late-pivot recovery" as distinct strategies with different winning conditions.
- **R16.** The strategy config exposes **phase-flavored parameters** so the optimizer surfaces different configs for different play styles: primary rocket tier preference, human-rating pursuit (bool + priority), recovery-unlock timing preference. These are the concrete optimization surface for the Phase 2.5 decision space.
- **R17.** Success criteria for Phase 2.5 specifically: across a set of seeds, the optimizer finds **at least two materially different winning configs** (e.g. "light-tier frequent launches, no human-rating, late recovery" vs "medium-tier human-rated, early recovery"), and both beat a naive baseline. This is the direct evidence that phase-specific play styles exist.

### Phase 3 — Event system enrichment (early-game entropy)

- **R18.** At least 4 new events target the ground / early-orbital phase with asymmetric impact on readiness or build_quality. Examples (final list is a planning decision): favorable weather window (readiness bump), storm front (readiness decay + launch scrub risk), bad propellant batch (next launch's build_quality penalty), successful pathfinder test (quality bump to rocket in construction).
- **R19.** At least 1 new event offers an asymmetric upside opportunity tied to commitment — e.g. a research grant contingent on a launch within a deadline, or a short-duration market window that rewards fast export.
- **R20.** Events are tunable via scenario overrides so the Phase 1 diagnostic scenarios can compare `events_enabled: true` vs `false` at the same seed set.
- **R21.** No event may silently mutate state the autopilot cannot observe — all event outcomes flow through the existing Event/alert pipeline and appear in metrics snapshots.

### Cross-cutting — Observability for LLM co-pilot forward-compatibility

- **R22.** All new state introduced by Phase 2 and Phase 2.5 (per-rocket `build_quality`, `reuse_count`, reusability tier; per-pad `readiness`; current launch success probability for a given rocket+pad pair) is exposed through the existing `sim_daemon` metrics snapshot / state endpoints. No reliability-relevant value lives only inside `sim_core` internals.
- **R23.** New failure / launch events emit fully-structured alerts with human-readable `description_template` fields populated (rocket id, tier, cause, consequences). Tambo will later render these as narrative callouts and approval cards; today, they just populate alerts + metrics.
- **R24.** Launch-time computed probabilities (`P(success) = readiness × build_quality × tech_bonus` or similar) are exposed as a *previewable* value through the daemon before the player / autopilot commits to the launch — i.e. "given this rocket and this pad right now, what's my success probability?" is a query the API can answer. This is the state that will eventually populate the tambo approval card.
- **R25.** The `balance-advisor` MCP tool surface gains (or extends existing tools with) access to the new state: metrics digest includes readiness / per-tier rocket inventory; suggest_parameter_change / suggest_strategy_change can target the new strategy fields. Exact tool changes deferred to planning.

## Success Criteria

- Composite score spread across 20 seeds at 50k ticks **widens from the current 2.5% baseline to ≥8%** (target) and the optimizer finds configs that beat the default by ≥5% on the harder scenario. Stretch: ≥15% spread.
- In high-entropy scenarios (events on, launches can fail), the Bayesian optimizer surfaces a **materially different** winning config than it surfaces in clean-seed scenarios. This is the direct evidence of adaptive strategy space.
- At least one strategy exists that beats the default in seeds with early launch failures *and* underperforms it in clean-start seeds — proving that robustness and aggression are now different strategies, not the same strategy.
- **Phase 2.5 specifically:** the optimizer finds ≥2 materially different winning configs that reflect different early-game play styles (e.g. light-tier/expendable/no-crew vs medium-tier/recovery/human-rated). Both beat a naive baseline. This is the direct evidence that early commitments compound forward meaningfully.
- Phase 1 completes and produces publishable spread numbers **before any Phase 2 code is written**.

## Scope Boundaries

- **In (clarified):** **Light commitment pressure at phase transitions** — switching costs, capital lock-in, opportunity cost on early rocket-tier / human-rating / recovery-timing decisions. These are the natural consequence of phase-specific play styles and belong in this project.
- **Out:** **Strict mutually-exclusive branching tech paths** (where unlocking A forecloses B entirely). Deferred to a separate brainstorm once Phase 1–3 results show whether it's still needed. The line: R13–R16 add *cost to switch*, not *inability to switch*.
- **Out:** Multi-objective / Pareto-front scoring. Wait for Phase 1 data.
- **Out:** Any changes to `dev_advanced_state`. It is explicitly a perf profiling harness, not a gameplay target.
- **Out:** ML-based autopilot reactions to events (e.g. trained models). Out of scope for this project; rule-based reactions only.
- **Out:** Refinery / assembly DAG content expansion (more elements, recipes, alloys). Not the diagnosed problem.
- **Out:** "Run the optimizer longer" as a standalone fix. Diminishing returns per the existing solutions doc.
- **Out:** UI work beyond the minimum needed to surface readiness and per-rocket quality during testing.

## Key Decisions

- **Launch reliability mechanic: B-lite.** Per-pad readiness state + per-rocket build_quality + simple RNG roll against their product. Not a per-launch pure RNG (A) and not a full staged-outcome distribution (C). Reason: gives the autopilot observable state to plan around, lets events modulate a durable variable instead of ambushing individual launches, and leaves headroom to enrich the failure distribution later without rewriting the system.
- **Recovery is its own roll.** Reusable flights face two independent reliability checks (launch, recovery). Failed recovery loses the booster but the payload arrives. Reason: makes reusability a real risk/reward curve instead of a free cost reduction once tech is unlocked.
- **VIO-560 rolls into this project** from `P2: Ground Operations & Telescopes`. Reusability and reliability share state (`build_quality`, `reuse_count`, per-rocket inventory) and tech gates, so implementing them together is strictly cheaper than sequentially.
- **Diagnostic first.** Phase 1 runs before any Phase 2 code. If scenario-only changes already widen the spread dramatically, Phase 2 scope shrinks accordingly.
- **Event system is the vehicle for entropy.** Rather than a parallel "world state" layer, new readiness/quality modulation flows through the existing event pipeline. Keeps the architecture coherent and makes events finally load-bearing.
- **Early commitments compound forward.** The design principle behind Phase 2.5. The early game is about home-base manufacturing, research, and key launch decisions. Those decisions should durably shape the mid and late game via switching costs and opportunity cost — not by forbidding alternatives, but by making "who you became in the early game" a real question with a real answer by the time you're running an industrial empire.
- **Forward-compatible with the LLM co-pilot.** Design the decision surface so a future Tambo/Copilot-Kit layer can reason about it. Means: all new state is queryable through the daemon and MCP surface (not buried in `sim_core`), launch probabilities are previewable before commit, events emit structured human-readable alerts, and the strategy fields the optimizer tunes are the same fields a Tier-2+ assistant will eventually suggest adjusting. This is not a build mandate — it's a non-hostility constraint on the design.

## Dependencies / Assumptions

- **VIO-614's Bayesian optimization pipeline is functional and reusable as-is.** This brainstorm is not changing the optimizer; it's giving it something meaningful to optimize against.
- **ADR-4 defines rockets as inventory items with metadata.** Planning must confirm the existing contract and extend it (don't parallel-table reliability state).
- **Existing rocketry tech chain stays intact.** `tech_basic/medium/heavy_rocketry` and `tech_partial_recovery` remain the backbone; new techs plug in as effects on reliability rather than replacements.
- **`strategy.json` is the right surface for new autopilot thresholds.** New fields (e.g. `launch_readiness_min`, `rocket_scrap_quality_threshold`) are the contract the optimizer tunes.
- **The existing event infrastructure (weight modifiers, cooldowns, effects) is expressive enough to represent the new events with at most a small number of new effect types** (e.g. `modify_pad_readiness`, `modify_next_build_quality`). Confirm during planning.
- **The Tambo / LLM co-pilot project is deferred but anchored.** We assume the Tambo project description is the authoritative vision for the eventual interaction layer, and that today's Copilot Kit vs Tambo decision doesn't change the shape of the state surface we're designing against. Starting stack: OpenRouter for minimal-cost experimentation, switching to local inference (Mac Mini M4 Pro, 48GB unified memory, quantized 7–13B) when hardware arrives. None of this is in scope for this project — it's context.

## LLM Co-Pilot Value Moments (catalog for future Tambo work)

Per the Tambo project's mandate to track "moments where an assistant would add value" during P1–P5, this brainstorm produces the following catalog. These are **not requirements for this project** — they're anchors for a future Tambo prototype after P1 lands, captured now because the design is fresh.

- **"What's my current launch success probability for the Medium Launcher?"** — Tier-1 query against R24. Tambo reads daemon state, formats a one-line answer.
- **"Should I launch now or wait for the weather?"** — Tier-2 recommendation. Tambo reads pad readiness, recent weather events, forecast (if tech unlocked), and suggests a delay or a go. Renders as approval card with probability + wait estimate.
- **"This booster has flown 8 times — should I scrap it?"** — Tier-2 recommendation. Tambo reads `reuse_count` + `build_quality` + the scrap threshold from strategy config, and proposes scrap-now vs one-more-flight.
- **"Why did my Heavy launch fail?"** — Tier-1 diagnosis. Tambo reads the `LaunchFailed` event, explains the probability roll, and points at the contributing factors (low readiness, worn booster, missing weather tech).
- **"Plan my early-game rocket investment."** — Tier-3 strategic planning. Tambo reasons about budget, current tech, expected payloads, and proposes a rocket tier commitment (light vs medium) with a justified recommendation the player can approve or modify.
- **"Should I invest in human-rating or recovery first?"** — Tier-3 strategic tradeoff. Tambo weighs current cash position, research progress, and expected game horizon, proposes one path with reasoning.
- **"A storm front is moving in — what should I do about my Medium Launcher scheduled in 200 ticks?"** — Tier-2 proactive recommendation. Tambo correlates the incoming event, the scheduled launch, and proposes a scrub / delay / accept-risk card.

These moments also serve as an informal UX sanity check: if any requirement in Phase 2 / 2.5 makes one of these moments *harder* to support later, it's probably wrong.

## Outstanding Questions

### Resolve Before Planning

*(none — all blocking product decisions are nailed down above)*

### Deferred to Planning

- **[Affects R1][Needs research]** What starting balance value for the Phase 1 diagnostic scenario creates meaningful pressure without causing universal collapse? Needs a pilot run to tune.
- **[Affects R4][Technical]** Data model for per-rocket state — extend the inventory item shape (ADR-4) vs a parallel `RocketState` map on `GameState`?
- **[Affects R5][Technical]** Data model for per-pad readiness — extend `FacilityCore` or add a `LaunchPadState` struct?
- **[Affects R6][Needs research]** What base launch probabilities by rocket tier produce good gameplay? A few candidate curves (e.g. sounding 0.95 / light 0.85 / medium 0.75 / heavy 0.65) need to be tuned against the diagnostic scenario until early-game difficulty feels right.
- **[Affects R9][Needs research]** What decay curve for `build_quality` as a function of `reuse_count` produces the VIO-560 real-world cost curve (45% by flight 2, 80% by 5, 85–90% by 10)?
- **[Affects R8][Technical]** Does recovery RNG use the same seed stream as launch RNG, or a separate one? Determinism implications.
- **[Affects R12][Technical]** Does the autopilot need net-new decision code, or can the existing strategy-priority-driven path absorb the new thresholds? Likely the latter for scrap-threshold; possibly the former for "wait for good weather."
- **[Affects R18–R21][Technical]** Do new event effects (`modify_pad_readiness`, `modify_next_build_quality`) need first-class enum variants, or can they be expressed with existing effects?
- **[Affects R18][Needs research]** What cooldowns / weight modifiers / conditions produce a healthy rate of consequential early-game events without becoming oppressive?
- **[Affects R13][Technical]** Concrete mechanism for rocket tier switching cost. Candidates: (a) pad retrofit recipe that consumes materials and takes real time; (b) separate pad modules per tier so switching means building a new pad; (c) fuel system specialization that forces re-tooling. Need to pick one before planning so the data model supports it.
- **[Affects R14][Needs research]** What does "human-rated" actually do mechanically beyond "required for crewed launches"? Candidates: unlocks crewed-only station construction, enables on-orbit research bonus multipliers, gates specific late-game techs. Need a concrete list of downstream effects.
- **[Affects R15][Technical]** Does recovery-investment-timing need new content, or is it already a knob the optimizer can turn via research priority weights? Likely the latter — confirm during planning.
- **[Affects R17][Needs research]** Do the Phase 2.5 winning-config differences show up at 50k ticks, or do we need longer scenarios to let commitments compound enough? Likely needs a pilot bench run during planning to size the validation scenario correctly.

## Next Steps

→ `/ce:plan` for structured implementation planning
