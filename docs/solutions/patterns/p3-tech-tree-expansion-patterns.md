---
title: "P3 Tech Tree Expansion: Content-Driven Domains, Tiered Pacing, and Cross-Facility Research"
category: "patterns"
problem_type: "feature-implementation"
component: "sim_core/research, sim_world/content, sim_control/autopilot, ui_web/theme"
symptoms:
  - "ResearchDomain enum blocked content-only domain additions"
  - "No tier field on TechDef — tree was flat, all techs researched at uniform rate"
  - "No pacing knobs — tuning research speed required Rust code changes"
  - "module_engineering_lab was configured as Manufacturing domain (naming mismatch)"
  - "Ghost techs (tech_satellite_basics, tech_orbital_science) referenced in 11 places but never defined"
  - "Autopilot sensor_purchase ignored required_tech — silent $30M burn + inventory churn"
  - "Lab diminishing returns counted only station labs, excluded ground facility labs"
  - "Starting inventory contained modules whose gating tech was not pre-unlocked"
tags:
  - "p3"
  - "research"
  - "tech-tree"
  - "content-driven-types"
  - "autopilot-gating"
  - "cross-facility"
  - "multi-ticket"
  - "migration-pattern"
severity: "medium"
date: "2026-04-05"
related:
  - "docs/solutions/patterns/multi-ticket-satellite-system-implementation.md"
  - "docs/solutions/patterns/stat-modifier-tech-expansion.md"
  - "docs/solutions/patterns/cross-layer-enum-refactor-and-dag-ui.md"
  - "docs/solutions/patterns/progression-system-implementation.md"
  - "docs/solutions/patterns/multi-epic-project-execution.md"
---

# P3 Tech Tree Expansion: Patterns and Learnings

9 tickets merged end-to-end in a single session. Tech tree grew from 14 flat techs to 26 tiered techs across 3 tiers, with a new Engineering research domain, 4 configurable pacing multipliers, and content validation that would have prevented several latent bugs. This document captures the 7 reusable patterns and 12 prevention strategies that emerged from the work.

## Problem Statement

The research system had three compounding limitations blocking further balance work:

1. **`ResearchDomain` was a Rust enum**, so adding a new domain (Engineering) required code changes rather than a content edit. CLAUDE.md's "enums are for engine mechanics only, not content categories" rule was violated.
2. **`TechDef` had no tier field**, so the tech tree was flat and there were no knobs to pace early-vs-late-game research progression.
3. **Modules/recipes gated by unreleased techs silently broke starting states**, and the autopilot had no tech-filtering for sensor purchases — leading to $30M burn loops when gated modules appeared in starting inventory.

P3 expanded the tech tree from a flat list to a tiered DAG, introduced four pacing multipliers composed multiplicatively, added an Engineering domain + `EngineeringData` data kind, and closed the gating hole in both autopilot and starting state files. The migration preserved save compatibility by following the VIO-544 `DataKind` pattern (enum `"Survey"` and newtype `"Survey"` are wire-identical in serde JSON).

## Components Affected

- `crates/sim_core/src/types/mod.rs` — `ResearchDomain` newtype, `DataKind::ENGINEERING` constant
- `crates/sim_core/src/types/content.rs` — `TechDef.tier` field
- `crates/sim_core/src/types/constants.rs` — 4 new pacing constants
- `crates/sim_core/src/station/lab.rs` — pacing engine, cross-facility DR counting
- `crates/sim_core/src/station/assembler.rs` — dual data generation (Manufacturing + Engineering)
- `crates/sim_core/src/sim_events.rs` — `EffectDef::AddResearchData` takes `data_kind` not `domain`
- `crates/sim_world/src/lib.rs` — `validate_techs()` cross-reference validation
- `crates/sim_control/src/agents/ground_facility_agent/concerns/sensor_purchase.rs` — required_tech filter
- `content/techs.json` — 26 techs retiered/added across 3 tiers
- `content/module_defs.json` — repurposed `module_engineering_lab`, new `module_manufacturing_lab`, gated ground sensors
- `content/constants.json` — pacing constants (via serde defaults)
- `content/ground_start.json`, `content/satellite_start.json` — pre-unlock gating techs
- `scenarios/tech_tree_{baseline,fast,slow}.json` — pacing validation
- `ui_web/src/config/theme.ts` — Engineering DataKind + ResearchDomain entries

## Investigation Summary

- **Content-driven type migration is now a proven, repeatable pattern.** Converting `ResearchDomain` from enum to String newtype touched 60 references across 18 files but preserved save/wire compatibility because serde serializes unit enum variants and single-field tuple structs identically. This is the second successful application (after VIO-544 `DataKind`) and should be the default for any "category loaded from content" type.

- **Autopilot must filter by `required_tech` everywhere modules are acquired.** `sensor_purchase` was the last holdout — `satellite_management` and `launch_execution` already filtered, but sensor_purchase did not. Silent $30M burn + perpetual inventory churn when a gated sensor hit the starting state. Caught by pr-reviewer agent, not by tests.

- **Cross-reference validation catches ghost techs early.** `tech_satellite_basics` and `tech_orbital_science` were referenced in 11 places across content files but never defined — P4 review missed this entirely. Added `required_tech` cross-reference validation in `validate_techs()` that turns these into load-time panics.

- **Cross-facility counting is easy to miss.** Lab diminishing-returns initially counted only stations. Ground facilities use a proxy-station tick pattern, so their labs would see different counts than station labs during their separate tick. Caught by pr-reviewer.

- **Starting state and tech gates must move together.** Gating `module_optical_telescope` on `tech_ground_observation` broke both `ground_start.json` and `satellite_start.json` until their `research.unlocked` arrays were updated. Missing this in one file (satellite_start) caused the silent burn bug.

- **Reuse existing mechanisms before adding enum variants.** VIO-586's ticket proposed `TechEffect::ResearchSpeedBonus` but `StatModifier { stat: StatId::ResearchSpeed, ... }` already existed and was wired into the lab tick via VIO-582. Zero Rust changes needed — the ticket scope shrank to pure content.

- **pr-reviewer agent earned its keep.** Found blocking issues in 3 of 9 PRs (33% hit rate on latent defects). The categories of bug it caught were invisible to tests: UI rendering omissions, cross-facility asymmetry, silent autopilot burns.

---

## Pattern 1: Enum → Content-Driven String Migration

### Root Cause

Content-defined categories (research domains, data kinds, anomaly tags) started life as compile-time Rust enums, but the game is designed to be data-driven. Adding a new domain required a code change, recompile, and exhaustive match-arm updates — violating the "enums are for engine mechanics only, not content categories" rule in CLAUDE.md.

### Working Solution

In `crates/sim_core/src/types/mod.rs`, replace the enum with a newtype wrapping `String`, preserving the same serialization shape:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResearchDomain(pub String);

impl ResearchDomain {
    pub fn new(s: impl Into<String>) -> Self { Self(s.into()) }
    pub const SURVEY: &str = "Survey";
    pub const MATERIALS: &str = "Materials";
    pub const MANUFACTURING: &str = "Manufacturing";
    pub const PROPULSION: &str = "Propulsion";
    pub const ENGINEERING: &str = "Engineering";
}

impl std::fmt::Display for ResearchDomain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
```

Test fixtures migrate from `ResearchDomain::Survey` to `ResearchDomain::new(ResearchDomain::SURVEY)` — 60 references across 18 files. **Audit all `{:?}` formatters in logs** and switch to `{}`, because the struct's Debug output is `ResearchDomain("Survey")` rather than bare `Survey`.

Related cleanup: `EffectDef::AddResearchData { domain, amount }` became `{ data_kind, amount }`, deleting the `domain_to_data_kind()` mapping function entirely. The only consumer was the event pipeline, which can now carry `data_kind` directly from content JSON.

### Why This Works

Serde serializes unit enum variants as bare strings (`"Survey"`) and single-field tuple structs with `#[derive(Serialize, Deserialize)]` also as bare strings. Save files stay readable. The `SURVEY`/`MATERIALS`/... associated constants give call sites a lint-friendly, typo-resistant reference while still allowing content to introduce new domains without touching Rust.

### When to Apply

Any time an enum is branched on string IDs that originate in content JSON: research domains, data kinds, anomaly tags, element IDs, tech IDs, module categories. Reserve Rust enums for engine mechanics whose variants imply distinct code paths (`Command`, `Event`, `TaskKind`). If you catch yourself adding a new enum variant purely so content can reference a new name, migrate to a newtype-over-String instead.

---

## Pattern 2: Autopilot Tech-Gate Filtering Before Import

### Root Cause

In `crates/sim_control/src/agents/ground_facility_agent/concerns/sensor_purchase.rs`, the concern selected sensors to import without inspecting `required_tech`. Once `optical_telescope` was gated behind `tech_ground_observation` and `satellite_start.json` did not pre-unlock the tech, the loop would:

1. Pass the budget check and import $30M of telescopes
2. Fail the install step every subsequent tick because `required_tech` was not unlocked
3. Never retry the import because `in_inventory` still reported the telescopes as present

Net effect: silent $30M burn plus infinite inventory churn, invisible to bench smoke CI because nothing crashed and no alert fired.

### Working Solution

Mirror the guard already used in `satellite_management.rs` and `launch_execution.rs`, skipping any module whose required tech is not yet unlocked:

```rust
// Skip if the module's required_tech is not unlocked — otherwise
// we'd import it and fail to install every tick.
if let Some(ref required_tech) = def.required_tech {
    if !ctx.state.research.unlocked.contains(required_tech) {
        continue;
    }
}
```

### Why This Works

The guard moves the tech check upstream of the import decision, so the autopilot never spends money on a module it cannot install. The filter is inside the candidate loop, so it naturally re-evaluates each tick: once the tech unlocks, the module becomes an eligible candidate again without any special retry logic. Behavior stays symmetric with sibling concerns — reviewers can grep for a single shape across autopilot code.

### When to Apply

Any autopilot concern that (a) commits resources before installing/operating a content-defined thing and (b) the thing has a `required_tech` field. When adding a new `required_tech` field on a content type, search concerns that reference that type and confirm each enforces the guard before spending. New autopilot concerns should use the same three-line filter as boilerplate.

---

## Pattern 3: Cross-Reference Validation for Content Graph

### Root Cause

Before P3, `tech_satellite_basics` was referenced 11 times across `satellite_defs.json`, `recipes.json`, `autopilot.json`, and `satellite_start.json`, but it was never defined in `techs.json`. Content validation only checked prereq edges **inside** `techs.json`, leaving all outbound `required_tech` references from rockets, satellites, modules, recipes, and hulls unchecked. Ghost techs accumulated silently.

### Working Solution

Extend `validate_techs()` in `crates/sim_world/src/lib.rs` to assert that every `required_tech` reference resolves into the tech ID set:

```rust
for rocket in content.rocket_defs.values() {
    if let Some(ref tech) = rocket.required_tech {
        assert!(
            tech_ids.contains(tech),
            "rocket '{}' requires unknown tech '{}'",
            rocket.id, tech.0,
        );
    }
}
// + similar loops for satellite_defs, module_defs, recipes, hulls
```

### Why This Works

The validator now treats the content corpus as a directed graph and fails fast during `load_content()` when any edge dangles. Because validation runs before the sim starts (both in tests and prod), a ghost tech surfaces as a loud panic at load time rather than subtle runtime skipping. The assertion message includes the referencing entity ID and the missing tech name — debug time cut to seconds.

### When to Apply

Any time content JSON carries a cross-file reference (tech IDs, recipe IDs, element IDs, module def IDs, hull IDs). Every `required_*` or `*_id` field that points into another content collection deserves a matching validation loop. Add the check alongside the collection's existing structural validation, so the content graph is verified in one pass before any game state is constructed.

---

## Pattern 4: Counting Modules Across Stations AND Ground Facilities

### Root Cause

The first lab-diminishing-returns implementation in `crates/sim_core/src/station/lab.rs` iterated only `state.stations.values()`. Ground facility labs were excluded from the per-domain count, so building an orbital lab affected DR scaling but an identical ground lab did not. The asymmetry was surprising, hard to observe in a single tick, and violated the proxy-station tick model that treats ground facilities as equivalent to stations for module-pipeline purposes.

### Working Solution

Chain station cores with ground facility cores and iterate once over the combined stream:

```rust
pub(crate) fn count_labs_per_domain(
    state: &GameState,
    content: &GameContent,
) -> HashMap<ResearchDomain, u32> {
    let mut counts: HashMap<ResearchDomain, u32> = HashMap::new();
    let station_cores = state.stations.values().map(|s| &s.core);
    let facility_cores = state.ground_facilities.values().map(|g| &g.core);
    for core in station_cores.chain(facility_cores) {
        for module in &core.modules {
            if !module.enabled { continue; }
            let Some(def) = content.module_defs.get(&module.def_id) else { continue; };
            if let ModuleBehaviorDef::Lab(lab_def) = &def.behavior {
                *counts.entry(lab_def.domain.clone()).or_insert(0) += 1;
            }
        }
    }
    counts
}
```

### Why This Works

Both `StationState` and `GroundFacilityState` expose the same `FacilityCore` substructure, so once you collapse them to a `&FacilityCore` iterator the rest of the counting logic is identical. `Iterator::chain` keeps allocation out of the hot path. The `enabled` check ensures wear-disabled modules don't inflate DR, and the `if let ModuleBehaviorDef::Lab` destructure avoids miscounting as new behaviors are added.

### When to Apply

Any aggregation or scan that asks "how many modules of kind X exist in this game?" — thermal groups, maintenance targets, sensor coverage, crew capacity, power draw. Whenever you see a loop over only `state.stations.values()` that reads modules, ask whether ground facilities should participate. The proxy-station pattern means the answer is almost always yes unless the concept is explicitly orbit-only (docking, boiloff, microgravity).

---

## Pattern 5: Prefer Existing StatModifier Over New TechEffect Variants

### Root Cause

VIO-586's ticket proposed adding `TechEffect::ResearchSpeedBonus { value: f64 }` for a new "research acceleration" tech. But `StatId::ResearchSpeed` already existed and had been wired into the lab tick through `resolve_with_f32` during VIO-582. A new TechEffect variant would have duplicated plumbing (serde, match arms, event emission) for a behavior already expressible through the generic stat modifier.

### Working Solution

Implement the tech as a content-only change — zero Rust, one JSON edit:

```json
{
  "id": "tech_research_acceleration",
  "tier": 3,
  "prereqs": ["tech_advanced_manufacturing"],
  "domain_requirements": {
    "Manufacturing": 100.0,
    "Materials": 60.0,
    "Engineering": 60.0
  },
  "effects": [{
    "type": "StatModifier",
    "stat": "research_speed",
    "op": "pct_additive",
    "value": 0.25
  }]
}
```

### Why This Works

`StatModifier` is the generic extension point for any numeric stat the engine resolves through `resolve_with_f32`. Because the lab tick already consults `ResearchSpeed`, the 25% bonus composes with other modifiers (pacing multipliers from VIO-582, wear efficiency, future techs) using the shared `pct_additive`/`mult`/`flat` math. Adding a bespoke `ResearchSpeedBonus` variant would have created a second code path that a future contributor would have to remember to stack with `StatModifier` — a classic scalability trap.

### When to Apply

Before adding any new `TechEffect` variant, grep `StatId` for existing entries and check whether the effect can be expressed as a stat modifier. The same rule applies to `ModuleBehaviorDef` — prefer extending shared fields over new variants. If a proposed effect fits the shape "multiply/add to a number the engine already resolves per-tick," it belongs in `StatModifier`. Only add a new `TechEffect` variant when the effect unlocks qualitatively new behavior (`EnableDeepScan`, `UnlockRecipe`) that no stat can express.

See also: `docs/solutions/patterns/stat-modifier-tech-expansion.md` for the Epic 5 origin of this pattern.

---

## Pattern 6: Starting-State Audit When Gating Existing Content

### Root Cause

VIO-587 added `tech_ground_observation` as a gate on `optical_telescope`. Multiple start-state files (`ground_start.json` and `satellite_start.json`) already shipped with telescopes in their starting inventory. The ground start file was updated to pre-unlock the gating tech in `research.unlocked`, but the satellite start file was missed — causing the silent $30M burn described in Pattern 2.

### Working Solution

When gating a previously-ungated content item, search every `*_start.json` for references to the item and append the required techs (and their transitive prereqs) to each file's `research.unlocked` set:

```json
"research": {
  "unlocked": [
    "tech_satellite_basics",
    "tech_orbital_science",
    "tech_ground_observation",
    "tech_radio_astronomy"
  ]
}
```

Every start state that seeds the gated item now also seeds the techs, so the autopilot's tech-gate filter (Pattern 2) finds the item immediately eligible.

### Why This Works

Start states are hand-authored snapshots designed to let players (or scenarios) skip early-game bootstrap. They implicitly encode the assumption "whatever is in this inventory must also be operable immediately." When a new gate is introduced, that assumption silently breaks unless the starting `research.unlocked` set is updated in lockstep. Making the audit a mandatory step of any gating ticket keeps the invariant enforceable by review.

### When to Apply

Any ticket that adds or tightens a `required_tech` on a module, recipe, rocket, satellite, or hull. Before merging:

1. Grep `content/*_start.json` for the gated ID
2. For every hit, confirm the gating tech (and any transitive prereqs) appear in that file's `research.unlocked`
3. Ideally, add a validation test that asserts every starting-inventory module's `required_tech` is in the scenario's unlocked set

Pair this with Pattern 3's cross-reference validator — an even stronger version would assert at load time that every item in a start state's inventory has its required techs unlocked.

---

## Pattern 7: PR Reviewer Catches Invisible-to-Test Failure Modes

### Root Cause

Across the 9 P3 PRs, three had blocking issues that the full test suite and CI pipeline did not catch:

- **VIO-582:** clippy `map_unwrap_or` failure (CI-visible only under `-D warnings`) + lab DR ground-facility exclusion (Pattern 4, only visible across a full sim run comparing station-only vs mixed fleets)
- **VIO-583:** missing Engineering-domain entries in `ui_web/src/config/theme.ts` (only visible in the rendered UI)
- **VIO-587:** the `satellite_start.json` regression (Patterns 2 + 6, silent runtime burn invisible to bench smoke)

Common thread: failure modes were runtime-visible, in rarely-exercised code paths, or silent (no panic, no alert, no test assertion).

### Working Solution

Dispatch the `pr-reviewer` agent on every non-trivial PR after CI goes green, and treat its findings as merge-blocking. Fix should-fix items in the same PR, file tickets for larger follow-ups, never merge with unresolved should-fix comments. The review checklist in `MEMORY.md` items 11–18 explicitly covers content scalability, data-driven types, module extensibility, and test realism — exactly the categories of bug P3 kept producing.

### Why This Works

Unit tests assert what you thought to check. The pr-reviewer agent reads the whole diff with a checklist focused on patterns that *don't* produce test failures: UI mapping omissions, content-scalability violations, autopilot guards, silent fund burns, starting-state drift. The review happens post-CI, so it's not redundant with mechanical checks — it catches categories of bug where tests are structurally incapable of catching them. A ~33% hit rate on latent defects that would otherwise ship to production.

### When to Apply

Always, for any non-trivial PR, before merging. Even trivial-looking content PRs should get a reviewer pass when they touch gates, starting states, or cross-file references. The reviewer is especially valuable for PRs that:

- Add `required_tech` fields
- Change autopilot concerns
- Add theme/UI mappings for new content categories
- Modify starting states
- Restructure shared data structures used by both stations and ground facilities

---

## Prevention Strategies

Twelve specific, actionable rules distilled from the P3 bugs:

1. **Validate all cross-content references, not just same-type refs.** Every `required_*` field on any content type deserves a validation loop in `validate_techs()` / `validate_content()`.

2. **Starting inventory must be a closed system w.r.t. tech gates.** Adding `required_tech` to a module requires grep-auditing every `*_start.json` and updating `research.unlocked`.

3. **Autopilot concerns must never retry a failing install indefinitely.** Check install feasibility (tech, prereqs, resources) BEFORE spending money, back off on repeated failure.

4. **Ground facilities are not second-class citizens.** Any function iterating `state.stations` for a global property must also iterate `state.ground_facilities` via the proxy-station pattern.

5. **Local clippy must match CI clippy exactly.** Run `cargo clippy -- -D warnings`, matching `scripts/ci_rust.sh`.

6. **FE theme is a contract.** Every content-driven enum value (DataKind, ResearchDomain, AnomalyTag) must have entries in `ui_web/src/config/theme.ts`.

7. **Repurposing existing content is higher risk than adding new content.** Prefer adding a new module and deprecating the old one. If repurposing is unavoidable, map the full dependency graph first.

8. **Balance bench scenarios must measure every facility type.** Add ground facility KPIs (`gf_balance_delta`, `gf_autopilot_stuck_ticks`) to default bench output.

9. **Autopilot silent failures must emit telemetry.** Every concern failure path should emit an `AutopilotConcernBlocked` event with reason + tick count.

10. **Content migrations need a PR checklist.** Any ticket adding a new content-driven type or `required_*` field must document the files touched: content JSON, theme.ts, validation tests, scenarios, docs/reference.md.

11. **Proxy-station pattern needs symmetric test harness.** Every station-level system should have a parallel test exercising the same code via a GF proxy-station with equivalent assertions.

12. **pr-reviewer should grep diffs for tech gate additions.** Add to the reviewer's checklist: "Does this diff add a `required_tech`? If yes, verify all starting scenarios unlock that tech."

---

## Testing Recommendations

Concrete tests that would have caught the P3 bugs:

**sim_world / content validation:**
- `content_validation::ghost_tech_refs` — walk every `required_tech` on modules, recipes, sensors, satellites, ground_facilities; assert each ID exists in `techs.json` (Bug: ghost techs)
- `content_validation::orphaned_research_domains` — every `ResearchDomain` used in tech requirements must have at least one producing lab module in `module_defs.json` (Bug: module repurpose)

**sim_world scenario validation:**
- `scenario_validation::starting_modules_have_required_tech` — for every `*_start.json`, assert each starting module's `required_tech` is in `research.unlocked` (Bug: silent burn)

**sim_control autopilot:**
- `autopilot::sensor_purchase_skips_tech_gated_without_unlock` — GF with sensor concern, no tech unlocked, run 100 ticks, assert balance delta == 0 (Bug: silent burn)
- `autopilot::concern_blocked_emits_event` — assert diagnostic event fires on install failure (prevention)
- `autopilot::repeated_failure_backoff` — concern doesn't retry every tick (prevention)

**sim_core parity:**
- `lab_dr::station_and_gf_parity` — identical lab loadouts on a station and a GF; assert DR values match tick-by-tick (Bug: GF exclusion)
- `lab_dr::cross_facility_lab_count` — 2 station labs + 2 GF labs, assert each lab sees count=4 (Bug: GF exclusion)

**sim_bench scenarios:**
- `scenarios/gf_tech_gated_health.json` — GF with tech-gated sensors in starting inventory; KPI: `gf_balance_delta` must be within threshold

**ui_web theme coverage:**
- `theme.coverage.test.ts` — load content fixtures, assert every `DataKind` has entries in `DATA_KIND_COLORS` + `DATA_KIND_LABELS`, every `ResearchDomain` has `DOMAIN_COLORS` entry. Fail build on missing keys (Bug: theme.ts missing)

**CI / workflow:**
- Update `.claude/hooks/after-edit.sh` to run `cargo clippy -- -D warnings` matching `scripts/ci_rust.sh` exactly (Bug: clippy drift)
- Extract shared `scripts/ci_clippy.sh` to prevent CI/local drift

---

## Migration Checklist: Tech Tree / Content Changes

Use for: adding `required_tech`, migrating an enum to content string, adding a research domain, adding an autopilot concern that imports content.

1. **Reference walk**: Use `rust_analyzer_references` or grep the content ID across `content/`, `scenarios/`, `sim_core/`, `sim_control/`, `sim_world/`, `ui_web/`, `docs/`.
2. **Start state audit**: If adding `required_tech` to an existing module, grep every `*_start.json` and `scenarios/*.json` for the module ID. For each hit, verify the tech is in `research.unlocked`.
3. **Theme update**: If adding a new content-driven string (DataKind, ResearchDomain, AnomalyTag), update `ui_web/src/config/theme.ts` (colors, labels, icons) in the same commit.
4. **Domain producers**: If adding a `ResearchDomain`, ensure at least one lab module produces its data kind.
5. **Cross-reference validator**: If adding a `required_*` field on a new content type, add a validator loop in `validate_techs()`.
6. **Prefer new over repurposed**: For existing module/tech/recipe changes, prefer adding a new entity and deprecating the old one.
7. **Autopilot feasibility check**: If adding a concern that imports/installs, verify it checks install feasibility BEFORE spending, emits diagnostic events on failure, backs off on repeated failure.
8. **Facility-host aggregates**: Station-level aggregates (labs, power, crew) must iterate both `state.stations` and `state.ground_facilities`.
9. **Bench scenario**: For balance-affecting changes, add or update a sim_bench scenario that exercises the new path.
10. **Local clippy parity**: Run `cargo clippy -- -D warnings` (not `cargo clippy` alone) before pushing.
11. **Scoped tests**: `cargo nextest run -p sim_world -p sim_core -p sim_control` after content changes.
12. **Docs**: Update `docs/reference.md` if a new type, field, or tick-order step was added.
13. **Event sync**: If adding to `Event` enum, update `ui_web/src/hooks/applyEvents.ts` (CI enforces).
14. **FE smoke**: `cd ui_web && npm run dev`, eyeball the relevant panel for gray-fallback rendering.
15. **pr-reviewer**: Always run before merging, even for trivial-looking changes.

---

## PR Reviewer Meta-Learning

**3 of 9 P3 PRs (33%) had blocking issues found by pr-reviewer.**

### What pr-reviewers catch well (strengths)

1. **Cross-file consistency bugs** — theme.ts omissions, scenario unlock drift, proxy-station parity. Requires holding multiple files in context simultaneously.
2. **Silent failure paths** — unwrap on None, swallowed Results, infinite retry loops without telemetry. Reviewers read with a "what could go wrong" mindset.
3. **Content scalability violations** — hardcoded colors, string-matching on content IDs, enum variants where content strings belong.
4. **Function size / decomposition** — visual scanning catches 150-line functions faster than any lint.
5. **Asymmetric handling of parallel structures** — stations vs. ground facilities, success vs. failure paths, create vs. delete.

### What pr-reviewers miss or catch late (blind spots)

1. **Semantic mismatches invisible in the diff** — reviewers may not load pre-change context (e.g., "engineering_lab was already Manufacturing").
2. **CI/tooling drift** — reviewers focus on code, not CI script parity with local tooling.
3. **Balance / runtime behavior** — can spot "this might loop" but can't quantify economic impact without running the sim.
4. **Cross-reference graph integrity** — reviewers might notice one missing ref but not systematically walk the whole graph.
5. **Scenario file omissions** — if the PR doesn't touch `scenarios/`, reviewers often don't think to check scenarios.
6. **Non-Rust / non-TS files** — JSON content edits get less scrutiny than code.

### Implications for the pr-reviewer agent

- Strengthen the "grep the diff for content IDs and trace references" instruction
- Add an explicit "if the diff adds `required_tech`, verify scenario unlocks" step
- Add "if the diff touches station aggregates, verify GF parity" step
- Treat JSON content edits with the same rigor as code

### Implications for implementers

- Spend more time on pre-implementation grep + reference walks
- Before declaring work done, run `cargo nextest run -p sim_world` (content validation) in addition to crate-local tests
- For any `required_*` field addition, do the 3-grep dance: `content/`, `scenarios/`, `docs/`

---

## Related Documentation

### Direct follow-ups / siblings

- **`docs/solutions/patterns/multi-ticket-satellite-system-implementation.md`** — P4 sibling retrospective. Same multi-ticket execution pattern, immediately prior project. The `GroundFacilityConcern` and tech-gated recipe patterns established there are reused throughout P3.
- **`docs/solutions/patterns/stat-modifier-tech-expansion.md`** — Epic 5 origin of the StatModifier reuse pattern (Pattern 5 above). P3's `tech_research_acceleration` is the latest application.

### Overlapping patterns

- **`docs/solutions/patterns/cross-layer-enum-refactor-and-dag-ui.md`** — prior research-domain enum rename work. P3 supersedes its "enum rename must be atomic" rule for new domains (since VIO-544, adding a domain is pure content).
- **`docs/solutions/patterns/progression-system-implementation.md`** — P1 progression system. The `TickTimings` field count pattern and multi-seed integration test pattern apply to P3's pacing validation scenarios.
- **`docs/solutions/patterns/multi-epic-project-execution.md`** — "treat ticket text as a hypothesis, not a contract." P3 plan pre-dated VIO-544 completion, so the Engineering domain ticket shape changed significantly.

### Complementary

- **`docs/solutions/patterns/multi-project-planning-and-consolidation.md`** — origin of the P0-P6 sequence, explicitly calls out `DataKind`/`ResearchDomain` migration as a P2 prerequisite that unlocked P3.

---

## Docs That Need Refreshing

Identified by the Related Docs Finder subagent during this compound:

1. **`docs/reference.md`** — multiple stale entries:
   - `ResearchDomain` listed as an enum (now a content-driven string newtype)
   - "Research Domains (4)" table missing the Engineering entry
   - Constants table missing `research_speed_multiplier`, `research_domain_rates`, `research_tier_scaling`, `research_lab_diminishing_returns`
   - `techs.json` description severely stale ("only current tech is tech_deep_scan_v1")

2. **`docs/solutions/patterns/stat-modifier-tech-expansion.md`** — add forward-reference note that P3's `research_speed_multiplier` + diminishing returns are the new preferred tuning levers for the "evidence accumulates at ~0.04/tick" problem documented there.

3. **`docs/solutions/patterns/cross-layer-enum-refactor-and-dag-ui.md`** — add one-line note that the "enum rename must be atomic" rule no longer applies to adding new research domains (VIO-544 made them content strings).

**Recommendation**: Run `ce:compound-refresh reference.md` after this doc lands to update the stale type docs — that's the highest-impact refresh target.

---

## PR List

| Ticket | PR | Size | Description |
|---|---|---|---|
| VIO-616 | #435 | Medium | ResearchDomain enum → content-driven string newtype (60 refs, 18 files) |
| VIO-581 | #436 | Medium | TechDef.tier field + 4 research pacing constants + required_tech cross-reference validation |
| VIO-583 | #438 | Medium | Engineering research domain + repurpose engineering lab + new manufacturing lab + dual data generation |
| VIO-582 | #439 | Medium | Research pacing engine (speed, domain rate, tier scaling, lab DR) with cross-facility counting |
| VIO-584 | #440 | Small | Tier 1 content (8 techs, 2 new + 6 retiered + DAG restructure) |
| VIO-585 | #441 | Small | Tier 2 content (10 techs, 4 new + 6 retiered) |
| VIO-586 | #443 | Small | Tier 3 content (8 techs, 4 new + 4 retiered, StatModifier reuse for research acceleration) |
| VIO-587 | #444 | Small | Ground sensor gating + sensor_purchase autopilot tech filter fix |
| VIO-588 | #445 | Small | Pacing validation scenarios (baseline/fast 3x/slow 0.5x) |
| VIO-589 | — | Cancel | Tech tree API — covered by existing `/api/v1/content` + VIO-581 tier field |

**Total**: 9 merged PRs, 1 cancelled, 1 pre-existing Done (VIO-505 planning).
**Test growth**: 989 → 1008 tests (19 new).
**Review hit rate**: 3/9 PRs had blocking issues caught by pr-reviewer.
