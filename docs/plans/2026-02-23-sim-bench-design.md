# sim_bench: Automated Scenario Runner

## Goal

Run dozens/hundreds of sims automatically to detect collapse patterns, find balance problems, and see nonlinear scaling effects — turning the sim into a statistical system.

## Architecture

New workspace binary crate: `crates/sim_bench`. Depends on `sim_core`, `sim_world`, `sim_control`, `rayon`, `clap`, `serde_json`, `anyhow`.

### CLI

```
sim_bench run --scenario scenarios/cargo_sweep.json
sim_bench run --scenario scenarios/cargo_sweep.json --output-dir ./bench_results
```

One subcommand: `run`. Takes a scenario JSON file and optional output directory (default `runs/`).

## Scenario File Format

```json
{
  "name": "cargo_sweep",
  "ticks": 100000,
  "metrics_every": 60,
  "seeds": [1, 2, 3, 4, 5],
  "content_dir": "./content",
  "overrides": {
    "station_cargo_capacity_m3": 200.0,
    "wear_band_degraded_threshold": 0.6
  }
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | required | Scenario identifier, used in output directory |
| `ticks` | u64 | required | Ticks per seed |
| `metrics_every` | u64 | `60` | Metrics sampling interval |
| `seeds` | `[u64]` or `{"range": [start, end]}` | required | Seeds to run |
| `content_dir` | string | `"./content"` | Content directory path |
| `overrides` | object | `{}` | Constants field overrides (flat key-value) |

Unknown override keys are hard errors at load time.

## Execution Flow

1. Load and validate scenario JSON
2. Load content from `content_dir`
3. Apply `overrides` to `content.constants` (validate field names)
4. Run all seeds in parallel via `rayon::par_iter`
5. Each seed: `build_initial_state` → autopilot tick loop → collect `Vec<MetricsSnapshot>`
6. Write per-seed CSV to `<output_dir>/<name>_<timestamp>/seed_<N>/metrics_*.csv`
7. Compute cross-seed summary statistics from final snapshots
8. Print summary table to stdout
9. Write `summary.json` to the scenario output directory

## Override Application

`apply_overrides(constants: &mut Constants, overrides: &HashMap<String, Value>) -> Result<()>`

Manual match on field name strings. ~24 fields in Constants — no reflection needed. Unknown keys produce an error listing valid field names.

## Summary Statistics

Computed from the **final snapshot** of each seed:

| Metric | Source |
|--------|--------|
| Storage saturation % | `station_storage_used_pct` |
| Fleet idle % | `fleet_idle / fleet_total` |
| Refinery starved count | `refinery_starved_count` |
| Techs unlocked | `techs_unlocked` |
| Avg module wear | `avg_module_wear` |
| Repair kits remaining | `repair_kits_remaining` |

Each metric: mean, min, max, stddev across seeds.

### Collapse Detection

A seed is "collapsed" if its final snapshot has `refinery_starved_count > 0 AND fleet_idle == fleet_total`. Summary reports `collapsed_seeds / total_seeds`.

## Output Structure

```
runs/cargo_sweep_20260223_143022/
  scenario.json          # copy of input scenario
  summary.json           # cross-seed stats
  seed_1/
    run_info.json
    metrics_000.csv
  seed_2/
    run_info.json
    metrics_000.csv
  ...
```

## Example Output

```
=== cargo_sweep (5 seeds, 100k ticks each) ===

Metric                  Mean    Min     Max     StdDev
-------------------------------------------------------
storage_saturation_pct  0.73    0.61    0.88    0.11
fleet_idle_pct          0.12    0.05    0.22    0.07
refinery_starved_count  1.40    0.00    3.00    1.14
techs_unlocked          4.80    4.00    5.00    0.45
avg_module_wear         0.42    0.31    0.55    0.09
repair_kits_remaining   2.20    0.00    5.00    1.92
collapse_rate           0/5
```

## Dependencies

- `rayon` — parallel seed execution
- `clap` — CLI parsing (already in workspace)
- `serde_json` — scenario parsing (already in workspace)
- `anyhow` — error handling (already in workspace)
- No DuckDB, no Parquet — CSV output only
