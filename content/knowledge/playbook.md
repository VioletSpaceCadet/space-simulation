# Gameplay Strategy Playbook

A living document of strategy patterns, bottleneck resolutions, and parameter relationships learned from simulation analysis. Updated by Claude Code after analysis sessions via MCP tools or direct edits.

## Bottleneck Resolutions

### Ore Supply

- **Single-ship starvation:** With default `ticks_per_au` (2133) and `minutes_per_tick` (60), a single mining ship has round-trip transit times of ~2880 ticks for typical asteroid distances. During transit, refineries starve. Root fix: build additional mining ships early.
- **Refinery over-provisioning:** Running 3 refineries with 1 mining ship guarantees 2 are perpetually starved. Match refinery count to fleet mining throughput, not available ore.
- **Asteroid distance variance:** Closer asteroids drastically improve ore supply stability. Autopilot should prioritize closer iron-rich asteroids when ore starvation is active.

### Slag Backpressure

- **Steady-state slag ratio:** Iron smelting produces ~43% slag by mass at typical ore compositions. With 3 active refineries, slag accumulation reaches storage saturation around tick 3500.
- **Jettison threshold:** `autopilot_slag_jettison_pct` (0.75) controls when autopilot jettisons slag. Lowering this reduces storage pressure but wastes potential reprocessing feedstock.
- **Slag export:** Slag is not exportable in current pricing config. Jettison is the only disposal path until slag reprocessing recipes are added.

### Storage Saturation

- **Early warning at 70%:** Storage pressure above 70% signals imminent saturation. Primary drivers: slag accumulation, unprocessed ore backlog, or unexported materials.
- **Material export as relief:** Exporting refined Fe at $50/kg + $50/kg surcharge provides both storage relief and revenue. Autopilot exports when Fe exceeds `autopilot_fe_reserve_kg` (12,000 kg).

### Power Deficit

- **Module power draw:** Each active module draws power per tick from `station_power_available_per_minute`. Adding modules without matching solar arrays causes brownouts.
- **Battery as buffer:** Batteries smooth short-term power fluctuations but don't increase sustained capacity. Solar arrays are the only generation source.

## Parameter Relationships

- **`ticks_per_au` vs fleet utilization:** Lower values = faster transit = higher mining throughput per ship. At default (2133), a single ship spends ~80% of time in transit. Halving to ~1000 roughly doubles effective mining rate.
- **`mining_rate_kg_per_minute` vs refinery throughput:** Mining rate of 15 kg/min produces ~900 kg/hr. A single refinery processes faster than one ship can mine, so refinery starvation is the norm with a single ship.
- **`minutes_per_tick` (60):** 1 tick = 1 game hour. All rate-based parameters are per-minute and get converted via `Constants::rate_per_minute_to_per_tick()`. Test fixtures use `minutes_per_tick: 1`.
- **`autopilot_refinery_threshold_kg` (2000):** Refineries only process when ore exceeds this threshold. Higher values batch larger runs but increase idle time. Lower values reduce idle time but cause more frequent small batches.

## Fleet Sizing

- **Single ship:** Sufficient for 1 refinery. Chronic starvation with 2+ refineries. Viable only in early game before assembler produces additional ships.
- **Two ships:** Can sustain 2 refineries with staggered mining runs. Transit overlap reduces starvation windows.
- **Ship construction priority:** Early assembler should prioritize thrusters + ship construction. Each additional ship has multiplicative impact on ore throughput.
- **Ship construction cost:** 1 thruster ($1M) + Fe for hull. Thrusters are the gating component and require assembler recipe.

## Economy

- **Starting balance:** $1B. Sufficient for initial module imports but depletes quickly with large purchases.
- **Trade unlock timing:** Trade (import/export) unlocks after `trade_unlock_tick` derived from `minutes_per_tick`. Available early in default config.
- **Import vs manufacture:** Importing modules ($1.5M-$10M each + surcharge) is faster but expensive. Manufacturing requires assembler + recipe chain but is cheaper long-term.
- **Export strategy:** Fe export at $50/kg base + $50/kg surcharge = $100/kg effective price. Export batches of 500 kg minimum ($50K revenue min). Steady Fe export is the primary revenue source.
- **Propellant economics:** LH2 at $500/kg and LOX at $150/kg make propellant import expensive. On-station electrolysis (H2O → LH2 + LOX) is significantly cheaper once the module is installed.

## Research

- **Lab throughput:** Labs consume raw data from the sim-wide `data_pool` each tick and produce domain-specific research points. More labs = faster research, but diminishing returns as data pool depletes.
- **Research roll interval:** Tech unlock rolls happen every `research_roll_interval_minutes` (60) game minutes = 1 tick at default `minutes_per_tick`. Probabilistic — high-evidence techs unlock faster.
- **Domain specialization:** Each lab type (exploration, materials, engineering) processes one `DataKind` and produces points in one `ResearchDomain`. Multi-domain research requires multiple lab types.
- **Deep scan gating:** `DeepScan` commands require at least one unlocked tech with `EnableDeepScan` effect. Without this tech, deep scan attempts are silently dropped.

## Thermal Management

- **Smelter heat:** Processors with thermal requirements (smelters) generate heat per run. Without radiators, temperature rises until overheat zones trigger.
- **Radiator sizing:** Radiator `cooling_capacity_w` is shared across the thermal group. Size radiator count to match worst-case heat generation from all thermal modules.
- **Overheat wear penalty:** Warning zone = 2x wear rate. Critical zone = 4x wear rate + auto-disable. Thermal management directly impacts maintenance costs.
- **Passive cooling:** All thermal modules lose heat via Newton's law toward `thermal_sink_temp_mk` (293K). Passive cooling alone is insufficient for active smelting.

## Propellant Pipeline

- **Electrolysis chain:** H2O → LH2 + LOX via electrolysis unit. Requires H2O in station inventory (from volatile-rich asteroid mining or import).
- **Boiloff:** LH2 and LOX are cryogenic and experience boiloff. Storage duration is limited without active cooling (future feature).
- **Volatile mining priority:** Autopilot targets volatile-rich asteroids when LH2 drops below `autopilot_lh2_threshold_kg` (5000 kg). Volatile confidence threshold at 0.7.
