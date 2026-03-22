# AI Knowledge System: ML Data Pipeline Design

**Goal:** Build a Parquet-based data pipeline from sim_bench to DuckDB analysis, with model scaffolding that validates the end-to-end flow.
**Status:** Planned
**Linear Project:** [Game Knowledge System](https://linear.app/violetspacecadet/project/game-knowledge-system-5e1b333643bd) (Phase 1.5 milestone)

## Overview

Phase 1 (Structured Files) is complete — journal schema, playbook, MCP tools for knowledge capture and retrieval all work. Phase 1.5 builds the data pipeline that turns sim_bench bulk runs into ML-ready training data.

The pipeline is: **sim_bench → Parquet → DuckDB → features/labels → model stubs**. Each stage is stateless and file-based. The design prioritizes extensibility — when Entity Depth, Events, and Crew add new metrics, they flow through automatically via MetricsSnapshot.

## Design

### Parquet Export (sim_bench)

**Crates:** `arrow` + `parquet` added to sim_bench dependencies only.

New `ParquetMetricsWriter` in `sim_bench/src/parquet_writer.rs`:
- `new(path: &Path) -> Result<Self>` — creates file, writes schema from MetricsSnapshot fields
- `write_row(snapshot: &MetricsSnapshot) -> Result<()>` — appends row
- `finish(self) -> Result<()>` — flush and close
- `build_schema() -> Schema` — derives Arrow schema from MetricsSnapshot field names + types

Integration in `runner.rs`:
- `run_seed()` creates `ParquetMetricsWriter` alongside existing `MetricsFileWriter`
- Both receive every `compute_metrics` call (sampling controlled by existing `metrics_every`)
- Output: `seed_{seed}/metrics.parquet`
- Parquet metadata includes `metrics_version` for schema evolution

**Schema versioning:** Parquet handles sparse columns gracefully. New MetricsSnapshot fields = new Parquet columns. Old readers skip unknown columns. `metrics_version` in file metadata lets analysis scripts adapt.

### Python Analysis Pipeline

**Location:** `scripts/analysis/`
**Deps:** `requirements.txt` with `duckdb>=1.0`, `pyarrow>=17.0`

Scripts:
- `load_run.py` — Load a sim_bench run directory into DuckDB. Returns a relation with all seeds' metrics joined with run metadata.
- `features.py` — Compute derived features: throughput rates (delta/tick for material, ore, slag), fleet utilization ratios, bottleneck window durations, power surplus trends.
- `labels.py` — Outcome labeling: collapse detection (balance <= 0 or all ships idle 500+ ticks), storage saturation tick, research stall tick, final economy score.
- `cross_seed.py` — Multi-seed comparison: mean/stddev/min/max for key outcomes per scenario. Determinism verification.
- `example_analysis.py` — End-to-end demo script.

### Model Scaffolding (Stubs)

**Location:** `scripts/analysis/models/`

Stubs that prove the pipeline works end-to-end without committing to model architecture:
- `bottleneck_stub.py` — Loads features, trains majority-class baseline, prints accuracy
- `scoring_stub.py` — Loads asteroid features, trains mean-predictor baseline, prints MSE
- `README.md` — Model interface contract: input/output schema, training workflow, weight export format

### Extensibility Safeguards

1. **Parquet schema follows MetricsSnapshot** — one source of truth, no separate schema definition
2. **Analysis scripts are stateless** — read Parquet, output to stdout/file. No database, no server
3. **Model interface is file-based** — weight files are the contract between Python training and future Rust inference
4. **Autopilot config already serializable** — behavior registry (VIO-341) supports future LLM-driven parameter updates
5. **MetricsSnapshot is the extension point** — Entity Depth, Events, Crew metrics all flow through automatically

## Testing Plan

- **Parquet round-trip test** (Rust): write 100 snapshots, read back, verify field equality
- **Pipeline smoke test** (Python): sim_bench → Parquet → DuckDB → features → labels → stub model
- **Schema version test** (Rust): verify Parquet metadata contains metrics_version
- **Cross-seed determinism** (Python): 3-seed run, compare DuckDB loads

## Ticket Breakdown

### Phase 1.5: ML Data Pipeline

1. **ML-01: Parquet export from sim_bench** (VIO-337, update) — arrow+parquet crates, ParquetMetricsWriter, schema from MetricsSnapshot, integration in runner.rs
2. **ML-02: Parquet round-trip test** — Write/read verification, schema version in metadata
3. **ML-03: Python analysis scaffolding** (VIO-338, update) — load_run.py, features.py, requirements.txt, pyproject.toml
4. **ML-04: Outcome labeling scripts** — labels.py with collapse/saturation/stall detection
5. **ML-05: Cross-seed analysis** — cross_seed.py for multi-seed comparison and determinism checks
6. **ML-06: Training dataset generation** — Bulk sim_bench runs (20+ seeds, 3+ scenarios), labeled Parquet output
7. **ML-07: Model stubs** — bottleneck_stub.py, scoring_stub.py, README.md with interface contract
8. **ML-08: End-to-end pipeline smoke test** — Single script running full pipeline, verifiable in CI

Dependencies: ML-01 → ML-02, ML-03; ML-03 → ML-04, ML-05; ML-04 + ML-06 → ML-07; all → ML-08

## Open Questions

- **Parquet file rotation:** Should ParquetMetricsWriter rotate files like MetricsFileWriter (50k rows)? Probably not — Parquet handles large files well with row groups.
- **CI integration:** Should the pipeline smoke test run in CI? Adds Python to CI deps. Defer until pipeline is stable.
- **Asteroid features for scoring model:** Needs content design pass once Entity Depth defines hull+slot ship types that affect mining value. Deferred to Phase 2 planning.
- **Rust inference crate:** burn vs tract vs hand-rolled for decision trees. Research when Phase 2 models are designed.
