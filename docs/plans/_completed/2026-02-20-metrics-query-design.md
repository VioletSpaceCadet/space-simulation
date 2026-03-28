# Metrics Query System Design

## Summary

Embed DuckDB in the sim_daemon and sim_cli to enable SQL-based querying of metrics data. DuckDB reads directly from the rotating CSV files already produced by the per-run storage system — no changes to the sim loop or metrics collection.

Two query surfaces:
1. **Daemon:** Read-only SQL-over-HTTP endpoint (`/api/v1/query`) for live-run queries, UI integration, and AI access
2. **CLI:** `sim_cli analyze` subcommand for post-run analysis, cross-run comparison, and CI regression detection

## Architecture

```
sim loop → MetricsFileWriter → metrics_000.csv, metrics_001.csv, ...
                                        ↑
                            DuckDB reads CSV files directly
                                        ↓
                    /api/v1/query (daemon)  |  sim_cli analyze (CLI)
                                        ↓
                            JSON results → UI / AI / terminal
```

### Daemon: `/api/v1/query` endpoint

- `POST /api/v1/query` with `{ "sql": "SELECT ..." }`
- Returns `{ "columns": [...], "rows": [...] }` as JSON
- Read-only — DuckDB opened in read-only mode, query validated
- DuckDB reads from the current run's CSV files via `read_csv_auto('runs/<run_id>/metrics_*.csv')`
- Non-blocking: queries run on a tokio blocking task, don't hold the sim lock
- Connection created lazily on first query, reused across requests

### CLI: `sim_cli analyze` subcommand

- `sim_cli analyze --run <run_id> --sql "SELECT ..."`
- `sim_cli analyze --runs "runs/*" --sql "SELECT ..."` (cross-run)
- Opens DuckDB in-memory, reads CSV files, executes query, prints results as table or CSV
- No daemon needed — fully offline post-run analysis

### DuckDB integration

- Add `duckdb` crate to `sim_daemon` and `sim_cli`
- In-memory database (no persistent .duckdb file)
- Create views on startup pointing to the run's CSV glob pattern
- Example view: `CREATE VIEW metrics AS SELECT * FROM read_csv_auto('runs/<run_id>/metrics_*.csv')`
- For cross-run queries, include run_id as a column derived from the file path

### Security / safety

- Read-only DuckDB connection
- No filesystem writes from queries
- Consider query timeout (e.g. 5 seconds) to prevent runaway queries
- Allowlist or validate that queries don't contain DDL/DML (belt and suspenders with read-only mode)

## Example queries

```sql
-- Throughput: ore mined per 1000 ticks
SELECT tick / 1000 AS period, MAX(total_ore_kg) - MIN(total_ore_kg) AS ore_delta
FROM metrics GROUP BY period ORDER BY period;

-- Fleet utilization over time
SELECT tick, fleet_idle::FLOAT / fleet_total AS idle_pct
FROM metrics WHERE fleet_total > 0;

-- Refinery starvation rate
SELECT COUNT(*) FILTER (WHERE refinery_starved_count > 0) * 100.0 / COUNT(*) AS starved_pct
FROM metrics;

-- Cross-run comparison (CLI with --runs glob)
SELECT run_id, MAX(techs_unlocked) AS final_techs, MAX(tick) AS duration
FROM metrics GROUP BY run_id;
```

## Files to modify

| File | Change |
|---|---|
| `Cargo.toml` (workspace) | Add `duckdb` to workspace deps |
| `crates/sim_daemon/Cargo.toml` | Add `duckdb` dependency |
| `crates/sim_cli/Cargo.toml` | Add `duckdb` dependency |
| `crates/sim_daemon/src/routes.rs` | Add `query_handler` for POST `/api/v1/query` |
| `crates/sim_daemon/src/state.rs` | Add `duckdb::Connection` to `AppState` (not `SimState`) |
| `crates/sim_daemon/src/main.rs` | Initialize DuckDB connection, create metrics view |
| `crates/sim_cli/src/main.rs` | Add `Analyze` subcommand |

## Open questions

1. **Query result format:** Should `/api/v1/query` return column-oriented `{ columns, types, data }` or row-oriented `[{col: val, ...}, ...]`? Column-oriented is more compact, row-oriented easier to consume in JS.

2. **Cross-run in daemon:** Should the daemon be able to query other runs (not just its own)? Could create a view across all `runs/*/metrics_*.csv`. Useful but may expose stale data.

3. **Predefined queries vs. raw SQL:** Should we ship a set of named queries (e.g. `/api/v1/metrics/throughput`) alongside raw SQL? Named queries are safer and easier for the UI; raw SQL is more flexible for AI.

4. **run_id in metrics rows:** Currently the CSV doesn't include `run_id` as a column. DuckDB can derive it from the `filename` column in `read_csv_auto`, but it might be cleaner to add `run_id` as a column in the CSV itself for cross-run queries.

5. **DuckDB crate version / bundled vs. system:** The `duckdb` Rust crate bundles `libduckdb` by default (adds ~15MB to binary). Acceptable? Or prefer dynamic linking?

6. **UI integration scope:** Is exposing the endpoint enough for now, or should we also design specific UI components (charts, comparison views) in this iteration?
