---
date: 2026-03-21
topic: random-events-narrator
---

# Random Events / Narrator System

## Problem Frame

The simulation is fully predictable — same seed produces identical outcomes every time. While determinism is a core design constraint, the game lacks unpredictability that forces adaptation. Random events (natural hazards, discoveries, equipment failures, cosmic phenomena) create pressure, narrative, and replayability. The goal is to build the **event engine primitives** — the generic system for defining, selecting, targeting, and applying game events — not a large content library. Future systems (exploration, warfare, diplomacy) will expand the event pool.

## Requirements

### Event Engine Core

- R1. **Event definitions in content JSON.** Each event has: id, name, description template, category, weight, cooldown, conditions (game state predicates), effects (list of effect operations), and targeting rules.
- R2. **Deterministic event selection.** Events roll from the simulation's `ChaCha8Rng`. Event pool evaluated at a configurable interval (e.g., every N ticks). Events sorted by ID before evaluation to maintain determinism. Same seed + same game state = same events.
- R3. **Weighted random selection.** Each event has a base weight. Weights can be modified by game state conditions (e.g., "equipment_failure" weight increases when avg module wear > 0.6). Weight modifiers defined in the event's content JSON, not in code.
- R4. **Event targeting.** Events target specific entities: a station, a ship, a zone, a module, or global (affects everything). Targeting rules defined per event (e.g., "target a random station" or "target the station with highest population" or "target a random zone with VolatileRich asteroids").
- R5. **Event cooldowns.** Per-event-type cooldown prevents the same event from firing repeatedly. Global cooldown between any events prevents event spam. Both configurable in content.
- R6. **Event conditions (prerequisites).** Events only enter the selection pool when conditions are met. Example: "meteorite_strike" requires at least one station. "Comet_flyby" requires tick > 10,000. "Crew_accident" requires station with crew > 0. Conditions evaluated as simple predicates on game state.

### Effect System

- R7. **Generic effect operations** that events can apply:
  - `DamageModule { target, wear_amount }` — add wear to a module
  - `DamageStation { target, storage_damage_pct }` — reduce station storage/capacity temporarily
  - `KillCrew { target, role, count }` — reduce crew count (ties into crew system)
  - `AddInventory { target, item, quantity }` — deposit resources (comet drops volatiles)
  - `AddResearchData { domain, amount }` — inject research data (supernova observation)
  - `SpawnScanSite { zone, template_override }` — create new scan site with specific properties
  - `ApplyModifier { target, stat, op, value, duration_ticks }` — temporary stat modifier via StatModifier system
  - `TriggerAlert { severity, message }` — fire an alert to the UI
- R8. **Effects are composable.** A single event can apply multiple effects. "Meteorite strike" = DamageModule + DamageStation + KillCrew + TriggerAlert. Effects are a list in the event definition.
- R9. **Temporary effects with duration.** Some effects last N ticks then revert (e.g., solar flare gives +50% research data for 500 ticks). Uses the StatModifier system's temporal modifier support.

### Event Categories (Initial Content)

- R10. **Natural hazards (Phase 1, ~3-5 events):**
  - Meteorite strike — damages random module + station, potential crew casualties
  - Solar flare — temporary power surge (bonus) then brownout (penalty), increased wear
  - Micrometeorite shower — minor wear increase across all modules for N ticks
- R11. **Cosmic phenomena (Phase 1, ~2-3 events):**
  - Comet flyby — temporary scan site with rare volatiles in a specific zone
  - Supernova observation — burst of research data across all domains
  - Asteroid cluster — multiple new scan sites appear temporarily
- R12. **Equipment/operational (Phase 1, ~2-3 events):**
  - Critical equipment failure — random module takes heavy wear spike
  - Supply disruption — import costs increased for N ticks (temporary pricing modifier)
  - Efficiency breakthrough — random module gets temporary efficiency bonus (representing crew innovation)

### Event Rarity & Gating

- R10a. **Rarity tiers** on event definitions: Common, Uncommon, Rare, Legendary. Rarity maps to base weight ranges (e.g., Common: 100, Uncommon: 25, Rare: 5, Legendary: 1). Content-defined per event.
- R10b. **Gates (prerequisites beyond conditions).** Events can require: minimum tick count, specific tech unlocked, minimum station/fleet size, specific resource in inventory, prior event having fired. Gates are evaluated before the event enters the weighted pool. Distinct from conditions: gates are hard requirements, conditions modify weights.
- R10c. **Event tags for filtering.** Events tagged with categories (natural, operational, discovery, crew, economic). Future systems can add event pools filtered by tag.

### Event Choices

- R10d. **Choice events** present 2-4 options with different outcomes. Each option has: label, description, effects list, and optional conditions (some choices only available if you have the right tech/crew/resources). Example: "Meteorite detected on collision course" → options: "Evacuate section (lose production, save crew)" vs "Emergency repair (costs materials, risk of failure)" vs "Do nothing (full damage)."
- R10e. **Autopilot choice selection.** When running autonomously, autopilot selects the option with the best expected outcome based on a simple heuristic (minimize crew loss > minimize damage > minimize cost). Choice heuristic is configurable or improvable via learning.
- R10f. **Choice timeout.** If no selection is made within N ticks, a default option fires (typically the worst outcome — forces engagement). Timeout duration per event definition.

### Event Chains

- R10g. **Chain events** can trigger follow-up events after a delay. Each event definition has an optional `follow_up` list: `{ event_id, delay_ticks, probability, condition }`. Example: "Meteorite strike" has follow-up "Aftershock debris" at 50% probability after 100 ticks.
- R10h. **Chain depth limit** (configurable, default 3) prevents infinite cascades.
- R10i. **Chain context passing.** Follow-up events inherit the parent's target (same station that got hit) unless overridden. Allows coherent multi-part narratives.

### Integration

- R13. **Events emitted as SSE events** to the UI, using the existing Event/EventEnvelope system. New event type: `GameEvent { event_def_id, target, effects_applied, description }`. UI displays in events feed with distinct styling.
- R14. **Event history tracked** in game state for cooldown evaluation and future narrative features. Capped ring buffer (last N events).
- R15. **Event system runs as a new tick phase.** After research (phase 4), before replenish (phase 5). Or as a separate phase 4.5. Events evaluated, selected, applied, emitted — all within the deterministic tick pipeline.

## Success Criteria

- Same seed produces identical event sequence (deterministic)
- Adding a new event type = JSON content only, no code changes
- Events create visible narrative moments in the event feed ("Meteorite strike damages Refinery on Station Alpha!")
- Events force adaptation (crew loss, module damage, resource windfalls change plans)
- Event frequency feels natural, not spammy (cooldowns work)

## Scope Boundaries

- **Not in scope:** Storyteller/difficulty curve pacing (Phase 2 — layer on top of weighted random)
- **Not in scope:** Player event responses/choices (Phase 2 — "do you evacuate or repair?")
- **Not in scope:** Chain events / event sequences (Phase 2 — "aftershock following meteorite")
- **Not in scope:** Combat events, pirate raids, alien contact (future systems)
- **Not in scope:** Event modding/scripting language (content JSON is sufficient)
- **Design for future:** Event definition format should be extensible enough that exploration, warfare, and diplomacy systems can add events without changing the engine.

## Key Decisions

- **Content-driven everything:** Event defs, weights, conditions, effects, targeting — all JSON. Engine is generic.
- **Deterministic via existing RNG:** Uses ChaCha8Rng, sorted evaluation, same seed = same events. Non-negotiable.
- **Composable effects:** Events are bags of generic effects. No per-event-type code. This is what makes the system extensible.
- **Primitives first, pacing later:** Simple weighted random with cooldowns in Phase 1. Storyteller difficulty curve is a Phase 2 enhancement that doesn't change the event format.

## Phasing

### Phase 1: Engine + Initial Content
- Event definition format in content JSON
- Rarity tiers (Common/Uncommon/Rare/Legendary) with base weight mapping
- Gates and conditions (prerequisites, state-based weight modifiers)
- Deterministic weighted random selection via ChaCha8Rng
- Event targeting (station, ship, zone, module, global)
- Cooldown system (per-event + global)
- Choice events with 2-4 options + autopilot heuristic selection + timeout
- Chain events with follow-ups (delay, probability, context passing, depth limit)
- Event tags for category filtering
- 8-10 initial events (hazards, phenomena, operational) — including at least 1 choice event and 1 chain event
- Generic effect operations (damage, add inventory, add research data, temporary modifiers)
- SSE event emission + UI display (choice events show options in UI)
- Event history ring buffer

### Phase 2: Pacing & Narrative Depth
- Storyteller system (tracks colony wealth/threat, adjusts event weights for dramatic pacing)
- More complex choice trees (choices that branch into sub-choices)
- Longer chain narratives (3-4 event sequences telling a coherent story)
- More content: 20+ events across categories

### Phase 3: System Integration
- Exploration events (anomaly signals, derelict discoveries → ties into artifact system)
- Crew events (morale incidents, skill discoveries, leader emergence)
- Economic events (market fluctuations, trade opportunities)
- Environmental events (zone resource depletion, new zone discovery)

## Dependencies / Assumptions

- **StatModifier system** (VIO-332) — temporary event effects flow through it
- **Crew system** — crew casualties require population tracking to exist
- **Existing tick pipeline** — new phase fits between research and replenish
- **Existing SSE/Event system** — game events emit as new Event variant
- **Event sync CI** — new Event variant requires FE handler (existing enforcement)

## Outstanding Questions

### Resolve Before Planning

(None — all blocking questions resolved)

### Deferred to Planning
- [Affects R15][Technical] Exact tick phase placement for event evaluation — between research and replenish, or elsewhere?
- [Affects R4][Needs research] How complex should targeting rules be? Simple random vs weighted by entity properties?
- [Affects R3][Needs research] What weight values and cooldowns produce good pacing? Needs playtesting/sim_bench tuning.
- [Affects R7][Technical] How do temporary effects integrate with StatModifier — auto-expiry via tick counter?

## Next Steps

→ `/ce:plan` for Phase 1 implementation.
