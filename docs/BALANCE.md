# Balance Analysis

> Last updated: 2026-02-24 (post VIO-5 through VIO-9).
> Baseline and balance_v1_tuned scenarios, 5 seeds each, 20,160 ticks (2 weeks sim-time).
> Using `dev_base_state.json` with full starting loadout (refinery, assembler, maintenance bay, 2 labs, 500 kg Fe, 10 repair kits).

## 1. Current State: Gameplay Loop Works

After VIO-5 through VIO-9, the full gameplay loop functions end-to-end:

```
Survey → Discover asteroid → Lab consumes ScanData → Research unlocks tech_deep_scan_v1
→ Deep scan asteroid → Mine → Deposit ore → Refinery processes → Material → Assembler → Repair kits
```

### Gameplay Milestones (Seed 1, consistent across seeds)

| Milestone | Tick | Sim Time | Notes |
|---|---|---|---|
| First asteroid discovered | 3,060 | ~2.1 days | Survey + transit cycle |
| tech_deep_scan_v1 unlocked | 3,660 | ~2.5 days | Exploration lab consumed scan data |
| Mining begins | 9,540 | ~6.6 days | After deep scan of viable asteroid |
| First ore deposited | 19,500 | ~13.5 days | 150,000 kg in one delivery |

### Baseline Metrics (5-seed average, current module_defs)

| Metric | Mean | Min | Max | Notes |
|---|---|---|---|---|
| total_ore_kg | 138,000 | 90,000 | 150,000 | One full cargo load per seed |
| total_material_kg | 8,410 | 0 | 42,051 | High variance — depends on timing |
| asteroids_discovered | 2.8 | 2 | 3 | |
| asteroids_depleted | 0 | 0 | 0 | Expected — 500k+ kg asteroids take weeks |
| techs_unlocked | 1.0 | 1 | 1 | Deep scan, every seed |
| refinery_starved | 0.8 | 0 | 1 | 4/5 seeds end with starved refinery |
| repair_kits_remaining | 0.8 | 0 | 1 | Nearly exhausted |
| avg_module_wear | 0.004 | 0 | 0.018 | Low — most modules idle |
| station_storage_used_pct | 0.5% | 0.05% | 2.1% | 2,000 m³ station barely used |
| collapse_rate | 0/5 | | | No collapses |

### Tuned vs Baseline (balance_v1_tuned adds lab interval=10, lab wear=0.002, refinery interval=180)

| Metric | Baseline | Tuned | Delta |
|---|---|---|---|
| total_ore_kg | 138,000 | 146,000 | +6% (less variance) |
| total_material_kg | 8,410 | 2,665 | -68% (slower refinery) |
| repair_kits_remaining | 0.8 | 0.0 | Worse — slower assembler depleted all kits |
| avg_module_wear | 0.004 | 0.002 | -50% (lab wear reduction works) |
| techs_unlocked | 1.0 | 1.0 | Identical — lab interval doesn't affect unlock timing |

---

## 2. Critical Issue: Maintenance Bay Wastes Repair Kits (VIO-11)

The maintenance bay fires every 30 ticks and consumes a repair kit if **any** module has
wear > 0.0 — even 0.001. Each repair removes 0.2 wear, massively overshooting trivial wear.

**The math:**
- Labs at `research_interval_ticks: 1`, `wear_per_run: 0.005` → 0.15 wear per 30 ticks
- Maintenance bay fires → consumes 1 kit, removes 0.2 (0.05 wasted)
- Rate: 2 kits/hour = 48 kits/day
- Starting 10 kits + 5 produced from initial Fe = 15 total → gone in ~7.5 hours
- **12+ day gap** with zero maintenance capability (tick ~1,260 to ~19,500)

**Fix:** Add `repair_threshold` to maintenance bay behavior. Only consume a kit if
most-worn module exceeds threshold (e.g., 0.1 = 10% wear).

---

## 3. Applied Constants (VIO-9, now in constants.json)

| Constant | Old | New | Rationale |
|---|---|---|---|
| `asteroid_mass_min_kg` | 100 | 500,000 | Real small asteroid (~7m dia S-type) |
| `asteroid_mass_max_kg` | 100,000 | 10,000,000 | ~20m diameter, substantial for gameplay |
| `station_cargo_capacity_m3` | 100 | 2,000 | Breathing room for batch processing |
| `ship_cargo_capacity_m3` | 20 | 50 | Shipping container — starter mining shuttle |
| `mining_rate_kg_per_tick` | 50 | 15 | 15 kg/min = ~21.6 t/day. Slow starter drill. |
| `deposit_ticks` | 60 | 120 | 2 hours to offload |
| `autopilot_refinery_threshold_kg` | 500 | 2,000 | Buffer above 1,000 kg batch size |

Constants kept as-is: `travel_ticks_per_hop` (2,880), `survey_scan_ticks` (120),
`deep_scan_ticks` (480), `research_roll_interval_ticks` (60), wear thresholds (0.5/0.8).

---

## 4. Module-Level Tuning Recommendations (VIO-10)

Testable via `module.*` overrides in sim_bench scenarios.

### Labs — Confirmed beneficial

| Parameter | Current | Proposed | Sim Result |
|---|---|---|---|
| `research_interval_ticks` | 1 | 10 | No effect on tech unlock timing (both at tick 3,660). Reduces wear. |
| `wear_per_run` | 0.005 | 0.002 | Halves avg wear (0.004 → 0.002). |

### Refinery — Needs longer scenario to verify

| Parameter | Current | Proposed | Sim Result |
|---|---|---|---|
| `processing_interval_ticks` | 60 | 180 | Less material produced (2,665 vs 8,410 kg) but only 660 ticks of processing time in 2-week run. Need 30+ day scenario. |

### Assembler — REVISED: keep at 120

| Parameter | Current | Proposed | Sim Result |
|---|---|---|---|
| `assembly_interval_ticks` | 120 | ~~240~~ **120 (keep)** | Doubling to 240 exhausted all kits by tick 1,260. Assembler needs to match maintenance demand. |

### Maintenance Bay — NEW recommendation

| Parameter | Current | Proposed | Rationale |
|---|---|---|---|
| `repair_threshold` | 0.0 (implicit) | 0.1 | Don't waste kits on trivial wear. See VIO-11. |

---

## 5. Overridable Constants (VIO-8, implemented)

Module-level overrides are now supported via dotted keys in scenario JSON:

```json
{
  "overrides": {
    "module.processor.processing_interval_ticks": 180,
    "module.lab.research_interval_ticks": 10,
    "module.lab.wear_per_run": 0.002,
    "module.assembler.assembly_interval_ticks": 240,
    "module.maintenance.wear_reduction_per_run": 0.2
  }
}
```

### Still needed

| Proposed Override | Why |
|---|---|
| `module.maintenance.repair_threshold` | New field — blocks VIO-11 fix from being testable via overrides |
| `tech.difficulty_multiplier` | Global tech difficulty scaling |
| `tech.domain_requirement_multiplier` | Scale domain point requirements |

---

## 6. Design Principles

### "Hard Sci-Fi With Gameplay Concessions"

**Hard sci-fi:** Real asteroid masses, multi-day travel, analyze-before-extract,
equipment degradation, probabilistic research.

**Concessions:** Single-ship operations, starter equipment is bad but present,
compressed time scales, visible progress within a play session.

### Observed Pacing (1 tick = 1 minute)

| Activity | Observed Duration | Target | Status |
|---|---|---|---|
| Transit to asteroid | 2 days (2,880 ticks) | 2–4 days | Good |
| Survey a site | 2 hours (120 ticks) | 2 hours | Good |
| First tech unlock | 2.5 days (3,660 ticks) | 1–2 weeks | Fast — may want higher difficulty |
| Mining a full load | ~10 days (9,960 ticks) | 5–10 days | Good |
| First ore on station | 13.5 days (19,500 ticks) | — | Feels right for hard sci-fi bootstrap |

### Balance Levers (Ranked by Impact)

1. **Maintenance threshold** — Kit wastage is the #1 sustainability problem (VIO-11)
2. **Mining rate** — Fundamental economic input. 15 kg/min feels right.
3. **Travel time** — 2 days/hop is good. Don't change yet.
4. **Processing intervals** — Need longer scenarios to evaluate.
5. **Research speed** — Tech unlock at 2.5 days may be too fast. Revisit.
6. **Wear rates** — Lab wear reduction confirmed effective.
7. **Starting resources** — 500 kg Fe + 10 kits may need increase if VIO-11 alone isn't enough.

---

## 7. Open Issues

| Issue | Priority | Status | Summary |
|---|---|---|---|
| VIO-10 | Medium | Backlog | Apply lab interval/wear tuning to module_defs.json |
| VIO-11 | High | Backlog | Maintenance bay repair_threshold to prevent kit waste |
| VIO-12 | Medium | Backlog | Add 30-day and 90-day benchmark scenarios |
| VIO-13 | High | Backlog | Repair kit sustainability gap (blocked by VIO-11) |

### Resolved

| Issue | Summary |
|---|---|
| VIO-5 | Labs added to dev_base_state.json |
| VIO-6 | State file support in sim_bench |
| VIO-7 | build_initial_state() parity |
| VIO-8 | Module-level overrides |
| VIO-9 | Constants rebalancing applied |
