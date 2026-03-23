# Model Interface Contract

Baseline model stubs for the ML pipeline. These prove the end-to-end flow works
(Parquet -> DuckDB -> features -> labels -> model) without committing to model
architecture. Replace with real models when ready.

## Bottleneck Classifier

**Task:** Predict `bottleneck_type` for each tick window.

**Input schema:** Output of `bottleneck_timeline()` from `labels.py`:

| Column | Type | Description |
|---|---|---|
| seed | INTEGER | Simulation seed |
| tick_start | BIGINT | Start of bottleneck span |
| tick_end | BIGINT | End of bottleneck span |
| bottleneck_type | VARCHAR | One of: StorageFull, WearCritical, SlagBackpressure, OreSupply, FleetIdle, Healthy |

**Output schema:** Same as input with `predicted_type` column added.

**Current baseline:** Majority-class classifier (always predicts the most common type).

**To replace:** Swap `majority_class_train` / `majority_class_evaluate` with a real
classifier (XGBoost, LightGBM). The input/output schema stays the same.

## Economy Scorer

**Task:** Predict `score` (composite economy metric) for each seed.

**Input schema:** Output of `final_score()` from `labels.py`:

| Column | Type | Description |
|---|---|---|
| seed | INTEGER | Simulation seed |
| score | DOUBLE | Composite score (0-1 range, weighted: balance 30%, techs 30%, fleet 20%, throughput 20%) |
| final_tick | BIGINT | Last tick of the simulation |

**Output schema:** Relation with `seed`, `predicted_score`.

**Current baseline:** Mean predictor (always predicts the average score).

**To replace:** Swap `mean_predict_train` / `mean_predict_evaluate` with a real
regression model. Feed full feature set from `features.py` as input.

## Training Workflow

```bash
# 1. Generate training data (requires sim_bench build)
cargo run -p sim_bench -- run --scenario scenarios/baseline.json

# 2. Run bottleneck baseline
python3 scripts/analysis/models/bottleneck_stub.py data/training/v1/baseline/

# 3. Run scoring baseline
python3 scripts/analysis/models/scoring_stub.py data/training/v1/baseline/
```

## Weight Export Format

**Phase 1 (current):** No weights exported. Baselines are computed at inference time.

**Phase 2 (planned):**
- Decision trees: JSON export (`{feature_splits: [...], leaf_values: [...]}`)
- Neural nets: Binary weights for Rust inference (tract/burn format)

The weight file is the contract between Python training and future Rust inference.
Training writes the file; Rust reads it. Schema versioned alongside `METRICS_VERSION`.
