# sim_bench Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build `sim_bench`, a CLI tool that runs scenarios (multiple seeds in parallel) and produces per-seed CSV metrics + cross-seed summary statistics.

**Architecture:** New workspace binary crate `crates/sim_bench`. Loads a JSON scenario file, applies constant overrides, runs N seeds in parallel via rayon (each seed: `build_initial_state` → autopilot tick loop → collect metrics snapshots), writes per-seed CSVs and a cross-seed `summary.json`.

**Tech Stack:** Rust, clap (CLI), serde_json (scenario parsing), rayon (parallelism), anyhow (errors), chrono (timestamps). Depends on `sim_core`, `sim_world`, `sim_control`.

---

## Reference Files

These files are essential context. Read them before starting any task:

- **Design doc:** `docs/plans/2026-02-23-sim-bench-design.md` — scenario format, output structure, summary stats
- **sim_cli main.rs:** `crates/sim_cli/src/main.rs` — existing tick loop pattern to follow
- **sim_world lib.rs:** `crates/sim_world/src/lib.rs` — `load_content`, `build_initial_state`, `create_run_dir`, `write_run_info`, `generate_run_id`
- **sim_core metrics.rs:** `crates/sim_core/src/metrics.rs` — `MetricsSnapshot`, `MetricsFileWriter`, `compute_metrics`
- **sim_core types.rs:** `crates/sim_core/src/types.rs:653-684` — `Constants` struct (24 fields)
- **sim_control lib.rs:** `crates/sim_control/src/lib.rs` — `AutopilotController`, `CommandSource` trait

## Important Notes

- **After-edit hook:** `.claude/hooks/after-edit.sh` runs `cargo fmt` + `cargo test -p <crate>` after every `.rs` edit. If `cargo` isn't in PATH, prefix Bash commands with `source "$HOME/.cargo/env" 2>/dev/null;`.
- **Determinism:** All collection iteration must be sorted by ID before RNG use. Each seed gets its own `ChaCha8Rng`.
- **Workspace lints:** `clippy::pedantic` is warn-level. Code must pass `cargo clippy -p sim_bench`.

---

### Task 1: Scaffold the crate and verify it builds

**Files:**
- Create: `crates/sim_bench/Cargo.toml`
- Create: `crates/sim_bench/src/main.rs`
- Modify: `Cargo.toml` (workspace root — add `"crates/sim_bench"` to members)

**Step 1: Create `crates/sim_bench/Cargo.toml`**

```toml
[package]
name = "sim_bench"
version = "0.1.0"
edition = "2021"

[lints]
workspace = true

[dependencies]
sim_core = { path = "../sim_core" }
sim_control = { path = "../sim_control" }
sim_world = { path = "../sim_world" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
clap = { version = "4", features = ["derive"] }
rayon = "1.10"
chrono = "0.4"
rand = "0.8"
rand_chacha = "0.3"
```

**Step 2: Create minimal `crates/sim_bench/src/main.rs`**

```rust
use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sim_bench", about = "Automated scenario runner for sim benchmarking")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a scenario file across multiple seeds.
    Run {
        /// Path to the scenario JSON file.
        #[arg(long)]
        scenario: String,
        /// Output directory (default: runs/).
        #[arg(long, default_value = "runs")]
        output_dir: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            scenario,
            output_dir,
        } => {
            println!("Would run scenario: {scenario} -> {output_dir}");
        }
    }
    Ok(())
}
```

**Step 3: Add `"crates/sim_bench"` to workspace members in root `Cargo.toml`**

Add `"crates/sim_bench"` to the `members` list.

**Step 4: Verify it builds**

Run: `cargo build -p sim_bench`
Expected: Compiles successfully.

**Step 5: Commit**

```bash
git add crates/sim_bench/ Cargo.toml Cargo.lock
git commit -m "feat(bench): scaffold sim_bench crate with CLI skeleton"
```

---

### Task 2: Scenario loading and seed expansion

**Files:**
- Create: `crates/sim_bench/src/scenario.rs`
- Modify: `crates/sim_bench/src/main.rs`

**Step 1: Write the scenario types and loader in `scenario.rs`**

```rust
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Scenario {
    pub name: String,
    pub ticks: u64,
    #[serde(default = "default_metrics_every")]
    pub metrics_every: u64,
    pub seeds: SeedSpec,
    #[serde(default = "default_content_dir")]
    pub content_dir: String,
    #[serde(default)]
    pub overrides: HashMap<String, serde_json::Value>,
}

fn default_metrics_every() -> u64 {
    60
}

fn default_content_dir() -> String {
    "./content".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum SeedSpec {
    List(Vec<u64>),
    Range { range: [u64; 2] },
}

impl SeedSpec {
    pub fn expand(&self) -> Vec<u64> {
        match self {
            SeedSpec::List(seeds) => seeds.clone(),
            SeedSpec::Range { range } => (range[0]..=range[1]).collect(),
        }
    }
}

pub fn load_scenario(path: &Path) -> Result<Scenario> {
    let json = std::fs::read_to_string(path)
        .with_context(|| format!("reading scenario file: {}", path.display()))?;
    let scenario: Scenario = serde_json::from_str(&json)
        .with_context(|| format!("parsing scenario file: {}", path.display()))?;
    if scenario.name.is_empty() {
        bail!("scenario 'name' must not be empty");
    }
    if scenario.ticks == 0 {
        bail!("scenario 'ticks' must be > 0");
    }
    let seeds = scenario.seeds.expand();
    if seeds.is_empty() {
        bail!("scenario 'seeds' must produce at least one seed");
    }
    Ok(scenario)
}
```

**Step 2: Write tests for scenario loading**

Add a `#[cfg(test)]` module at the bottom of `scenario.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp_scenario(json: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(json.as_bytes()).unwrap();
        file
    }

    #[test]
    fn test_load_scenario_with_seed_list() {
        let file = write_temp_scenario(r#"{
            "name": "test_scenario",
            "ticks": 1000,
            "seeds": [1, 2, 3]
        }"#);
        let scenario = load_scenario(file.path()).unwrap();
        assert_eq!(scenario.name, "test_scenario");
        assert_eq!(scenario.ticks, 1000);
        assert_eq!(scenario.metrics_every, 60);
        assert_eq!(scenario.seeds.expand(), vec![1, 2, 3]);
        assert_eq!(scenario.content_dir, "./content");
        assert!(scenario.overrides.is_empty());
    }

    #[test]
    fn test_load_scenario_with_seed_range() {
        let file = write_temp_scenario(r#"{
            "name": "range_test",
            "ticks": 500,
            "seeds": {"range": [1, 5]}
        }"#);
        let scenario = load_scenario(file.path()).unwrap();
        assert_eq!(scenario.seeds.expand(), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_load_scenario_with_overrides() {
        let file = write_temp_scenario(r#"{
            "name": "override_test",
            "ticks": 100,
            "seeds": [42],
            "overrides": {
                "station_cargo_capacity_m3": 200.0,
                "mining_rate_kg_per_tick": 5.0
            }
        }"#);
        let scenario = load_scenario(file.path()).unwrap();
        assert_eq!(scenario.overrides.len(), 2);
    }

    #[test]
    fn test_load_scenario_empty_name_fails() {
        let file = write_temp_scenario(r#"{
            "name": "",
            "ticks": 100,
            "seeds": [1]
        }"#);
        let result = load_scenario(file.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name"));
    }

    #[test]
    fn test_load_scenario_zero_ticks_fails() {
        let file = write_temp_scenario(r#"{
            "name": "bad",
            "ticks": 0,
            "seeds": [1]
        }"#);
        let result = load_scenario(file.path());
        assert!(result.is_err());
    }
}
```

Add `tempfile = "3"` to `[dev-dependencies]` in `crates/sim_bench/Cargo.toml`.

**Step 3: Wire `scenario.rs` into `main.rs`**

Add `mod scenario;` at the top of `main.rs` and import `scenario::load_scenario`. In the `Run` arm, call `load_scenario` and print the loaded scenario name and seed count.

**Step 4: Run tests**

Run: `cargo test -p sim_bench`
Expected: All scenario tests pass.

**Step 5: Commit**

```bash
git add crates/sim_bench/
git commit -m "feat(bench): add scenario file loading with seed list/range support"
```

---

### Task 3: Override application

**Files:**
- Create: `crates/sim_bench/src/overrides.rs`
- Modify: `crates/sim_bench/src/main.rs` (add `mod overrides;`)

**Step 1: Write the override applicator in `overrides.rs`**

This function takes a mutable `Constants` and a map of overrides, applies each override by matching on the field name string, and returns an error for unknown keys.

Read `crates/sim_core/src/types.rs:653-684` to see all 24 fields of `Constants`. For each field, match the string key to the struct field. Use `serde_json::Value` to extract the value as the correct type (`as_f64() as f32` for f32, `as_u64()` for u64/u32).

```rust
use anyhow::{bail, Result};
use sim_core::Constants;
use std::collections::HashMap;

/// List of all valid override keys — used in error messages.
const VALID_KEYS: &[&str] = &[
    "survey_scan_ticks",
    "deep_scan_ticks",
    "travel_ticks_per_hop",
    "survey_scan_data_amount",
    "survey_scan_data_quality",
    "deep_scan_data_amount",
    "deep_scan_data_quality",
    "survey_tag_detection_probability",
    "asteroid_count_per_template",
    "asteroid_mass_min_kg",
    "asteroid_mass_max_kg",
    "ship_cargo_capacity_m3",
    "station_cargo_capacity_m3",
    "mining_rate_kg_per_tick",
    "deposit_ticks",
    "station_compute_units_total",
    "station_power_per_compute_unit_per_tick",
    "station_efficiency",
    "station_power_available_per_tick",
    "autopilot_iron_rich_confidence_threshold",
    "autopilot_refinery_threshold_kg",
    "wear_band_degraded_threshold",
    "wear_band_critical_threshold",
    "wear_band_degraded_efficiency",
    "wear_band_critical_efficiency",
];

pub fn apply_overrides(
    constants: &mut Constants,
    overrides: &HashMap<String, serde_json::Value>,
) -> Result<()> {
    for (key, value) in overrides {
        match key.as_str() {
            "survey_scan_ticks" => constants.survey_scan_ticks = as_u64(key, value)?,
            "deep_scan_ticks" => constants.deep_scan_ticks = as_u64(key, value)?,
            "travel_ticks_per_hop" => constants.travel_ticks_per_hop = as_u64(key, value)?,
            "survey_scan_data_amount" => constants.survey_scan_data_amount = as_f32(key, value)?,
            "survey_scan_data_quality" => constants.survey_scan_data_quality = as_f32(key, value)?,
            "deep_scan_data_amount" => constants.deep_scan_data_amount = as_f32(key, value)?,
            "deep_scan_data_quality" => constants.deep_scan_data_quality = as_f32(key, value)?,
            "survey_tag_detection_probability" => {
                constants.survey_tag_detection_probability = as_f32(key, value)?;
            }
            "asteroid_count_per_template" => {
                constants.asteroid_count_per_template = as_u32(key, value)?;
            }
            "asteroid_mass_min_kg" => constants.asteroid_mass_min_kg = as_f32(key, value)?,
            "asteroid_mass_max_kg" => constants.asteroid_mass_max_kg = as_f32(key, value)?,
            "ship_cargo_capacity_m3" => constants.ship_cargo_capacity_m3 = as_f32(key, value)?,
            "station_cargo_capacity_m3" => {
                constants.station_cargo_capacity_m3 = as_f32(key, value)?;
            }
            "mining_rate_kg_per_tick" => constants.mining_rate_kg_per_tick = as_f32(key, value)?,
            "deposit_ticks" => constants.deposit_ticks = as_u64(key, value)?,
            "station_compute_units_total" => {
                constants.station_compute_units_total = as_u32(key, value)?;
            }
            "station_power_per_compute_unit_per_tick" => {
                constants.station_power_per_compute_unit_per_tick = as_f32(key, value)?;
            }
            "station_efficiency" => constants.station_efficiency = as_f32(key, value)?,
            "station_power_available_per_tick" => {
                constants.station_power_available_per_tick = as_f32(key, value)?;
            }
            "autopilot_iron_rich_confidence_threshold" => {
                constants.autopilot_iron_rich_confidence_threshold = as_f32(key, value)?;
            }
            "autopilot_refinery_threshold_kg" => {
                constants.autopilot_refinery_threshold_kg = as_f32(key, value)?;
            }
            "wear_band_degraded_threshold" => {
                constants.wear_band_degraded_threshold = as_f32(key, value)?;
            }
            "wear_band_critical_threshold" => {
                constants.wear_band_critical_threshold = as_f32(key, value)?;
            }
            "wear_band_degraded_efficiency" => {
                constants.wear_band_degraded_efficiency = as_f32(key, value)?;
            }
            "wear_band_critical_efficiency" => {
                constants.wear_band_critical_efficiency = as_f32(key, value)?;
            }
            _ => bail!(
                "unknown override key '{key}'. Valid keys: {}",
                VALID_KEYS.join(", ")
            ),
        }
    }
    Ok(())
}

fn as_f32(key: &str, value: &serde_json::Value) -> Result<f32> {
    value
        .as_f64()
        .map(|v| v as f32)
        .ok_or_else(|| anyhow::anyhow!("override '{key}': expected a number, got {value}"))
}

fn as_u64(key: &str, value: &serde_json::Value) -> Result<u64> {
    value
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("override '{key}': expected a positive integer, got {value}"))
}

fn as_u32(key: &str, value: &serde_json::Value) -> Result<u32> {
    let val = as_u64(key, value)?;
    u32::try_from(val)
        .map_err(|_| anyhow::anyhow!("override '{key}': value {val} exceeds u32 range"))
}
```

**Step 2: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn default_constants() -> Constants {
        serde_json::from_str(include_str!("../../../content/constants.json")).unwrap()
    }

    #[test]
    fn test_apply_f32_override() {
        let mut constants = default_constants();
        let overrides = HashMap::from([(
            "station_cargo_capacity_m3".to_string(),
            serde_json::json!(200.0),
        )]);
        apply_overrides(&mut constants, &overrides).unwrap();
        assert!((constants.station_cargo_capacity_m3 - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_apply_u64_override() {
        let mut constants = default_constants();
        let overrides = HashMap::from([(
            "survey_scan_ticks".to_string(),
            serde_json::json!(99),
        )]);
        apply_overrides(&mut constants, &overrides).unwrap();
        assert_eq!(constants.survey_scan_ticks, 99);
    }

    #[test]
    fn test_unknown_key_errors() {
        let mut constants = default_constants();
        let overrides = HashMap::from([(
            "nonexistent_field".to_string(),
            serde_json::json!(1.0),
        )]);
        let result = apply_overrides(&mut constants, &overrides);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown override key"));
        assert!(err.contains("nonexistent_field"));
    }

    #[test]
    fn test_type_mismatch_errors() {
        let mut constants = default_constants();
        let overrides = HashMap::from([(
            "survey_scan_ticks".to_string(),
            serde_json::json!("not_a_number"),
        )]);
        let result = apply_overrides(&mut constants, &overrides);
        assert!(result.is_err());
    }
}
```

**Step 3: Wire into main.rs**

Add `mod overrides;` to `main.rs`.

**Step 4: Run tests**

Run: `cargo test -p sim_bench`
Expected: All tests pass.

**Step 5: Commit**

```bash
git add crates/sim_bench/
git commit -m "feat(bench): add constants override application with validation"
```

---

### Task 4: Single-seed runner function

**Files:**
- Create: `crates/sim_bench/src/runner.rs`
- Modify: `crates/sim_bench/src/main.rs` (add `mod runner;`)

**Step 1: Write the single-seed runner**

This function takes content (with overrides already applied), a seed, tick count, metrics_every, and an output directory path. It builds initial state, runs the autopilot tick loop, collects metrics snapshots, writes per-seed CSV, and returns the final `MetricsSnapshot`.

```rust
use anyhow::{Context, Result};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_control::{AutopilotController, CommandSource};
use sim_core::{EventLevel, GameContent, MetricsSnapshot};
use std::path::Path;

pub struct SeedResult {
    pub seed: u64,
    pub final_snapshot: MetricsSnapshot,
}

pub fn run_seed(
    content: &GameContent,
    seed: u64,
    ticks: u64,
    metrics_every: u64,
    seed_dir: &Path,
) -> Result<SeedResult> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut state = sim_world::build_initial_state(content, seed, &mut rng);
    let mut autopilot = AutopilotController;
    let mut next_command_id = 0u64;

    std::fs::create_dir_all(seed_dir)
        .with_context(|| format!("creating seed directory: {}", seed_dir.display()))?;

    // Write run_info.json
    sim_world::write_run_info(
        seed_dir,
        &format!("seed_{seed}"),
        seed,
        &content.content_version,
        metrics_every,
        serde_json::json!({
            "runner": "sim_bench",
            "ticks": ticks,
        }),
    )?;

    let mut metrics_writer = sim_core::MetricsFileWriter::new(seed_dir.to_path_buf())
        .with_context(|| format!("opening metrics CSV in {}", seed_dir.display()))?;

    let mut last_snapshot = sim_core::compute_metrics(&state, content);

    for _ in 0..ticks {
        let commands = autopilot.generate_commands(&state, content, &mut next_command_id);
        sim_core::tick(&mut state, &commands, content, &mut rng, EventLevel::Normal);

        if state.meta.tick % metrics_every == 0 {
            let snapshot = sim_core::compute_metrics(&state, content);
            metrics_writer.write_row(&snapshot).context("writing metrics row")?;
            last_snapshot = snapshot;
        }
    }

    // Always capture final snapshot
    let final_snapshot = sim_core::compute_metrics(&state, content);
    if state.meta.tick % metrics_every != 0 {
        metrics_writer.write_row(&final_snapshot).context("writing final metrics row")?;
    }
    metrics_writer.flush().context("flushing metrics")?;

    Ok(SeedResult {
        seed,
        final_snapshot,
    })
}
```

**Step 2: Write a basic integration test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_run_seed_produces_output() {
        let content = sim_world::load_content("../../content").unwrap();
        let temp_dir = TempDir::new().unwrap();
        let seed_dir = temp_dir.path().join("seed_42");

        let result = run_seed(&content, 42, 120, 60, &seed_dir).unwrap();

        assert_eq!(result.seed, 42);
        assert_eq!(result.final_snapshot.tick, 120);
        assert!(seed_dir.join("run_info.json").exists());
        assert!(seed_dir.join("metrics_000.csv").exists());
    }

    #[test]
    fn test_run_seed_determinism() {
        let content = sim_world::load_content("../../content").unwrap();
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();

        let result1 = run_seed(&content, 42, 120, 60, &dir1.path().join("seed_42")).unwrap();
        let result2 = run_seed(&content, 42, 120, 60, &dir2.path().join("seed_42")).unwrap();

        assert_eq!(result1.final_snapshot.tick, result2.final_snapshot.tick);
        assert_eq!(
            result1.final_snapshot.techs_unlocked,
            result2.final_snapshot.techs_unlocked
        );
        assert_eq!(
            result1.final_snapshot.fleet_total,
            result2.final_snapshot.fleet_total
        );
    }
}
```

**Step 3: Wire into main.rs**

Add `mod runner;` to `main.rs`.

**Step 4: Run tests**

Run: `cargo test -p sim_bench`
Expected: All tests pass (including integration tests that load real content).

**Step 5: Commit**

```bash
git add crates/sim_bench/
git commit -m "feat(bench): add single-seed runner with metrics CSV output"
```

---

### Task 5: Summary statistics and collapse detection

**Files:**
- Create: `crates/sim_bench/src/summary.rs`
- Modify: `crates/sim_bench/src/main.rs` (add `mod summary;`)

**Step 1: Write summary statistics computation**

Compute mean/min/max/stddev for 6 metrics from final snapshots, plus collapse detection.

```rust
use serde::Serialize;
use sim_core::MetricsSnapshot;

#[derive(Debug, Serialize)]
pub struct SummaryStats {
    pub seed_count: usize,
    pub collapsed_count: usize,
    pub metrics: Vec<MetricSummary>,
}

#[derive(Debug, Serialize)]
pub struct MetricSummary {
    pub name: String,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
    pub stddev: f64,
}

pub fn compute_summary(snapshots: &[(u64, &MetricsSnapshot)]) -> SummaryStats {
    let seed_count = snapshots.len();

    let collapsed_count = snapshots
        .iter()
        .filter(|(_, s)| s.refinery_starved_count > 0 && s.fleet_idle == s.fleet_total)
        .count();

    let extractors: Vec<(&str, Box<dyn Fn(&MetricsSnapshot) -> f64>)> = vec![
        ("storage_saturation_pct", Box::new(|s| f64::from(s.station_storage_used_pct))),
        ("fleet_idle_pct", Box::new(|s| {
            if s.fleet_total == 0 { 0.0 } else { f64::from(s.fleet_idle) / f64::from(s.fleet_total) }
        })),
        ("refinery_starved_count", Box::new(|s| f64::from(s.refinery_starved_count))),
        ("techs_unlocked", Box::new(|s| f64::from(s.techs_unlocked))),
        ("avg_module_wear", Box::new(|s| f64::from(s.avg_module_wear))),
        ("repair_kits_remaining", Box::new(|s| f64::from(s.repair_kits_remaining))),
    ];

    let metrics = extractors
        .iter()
        .map(|(name, extract)| {
            let values: Vec<f64> = snapshots.iter().map(|(_, s)| extract(s)).collect();
            compute_metric_summary(name, &values)
        })
        .collect();

    SummaryStats {
        seed_count,
        collapsed_count,
        metrics,
    }
}

fn compute_metric_summary(name: &str, values: &[f64]) -> MetricSummary {
    let count = values.len() as f64;
    let mean = values.iter().sum::<f64>() / count;
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count;
    let stddev = variance.sqrt();

    MetricSummary {
        name: name.to_string(),
        mean,
        min,
        max,
        stddev,
    }
}

pub fn print_summary(scenario_name: &str, ticks: u64, stats: &SummaryStats) {
    println!(
        "\n=== {} ({} seeds, {}k ticks each) ===\n",
        scenario_name,
        stats.seed_count,
        ticks / 1000
    );
    println!(
        "{:<30} {:>8} {:>8} {:>8} {:>8}",
        "Metric", "Mean", "Min", "Max", "StdDev"
    );
    println!("{}", "-".repeat(70));
    for metric in &stats.metrics {
        println!(
            "{:<30} {:>8.2} {:>8.2} {:>8.2} {:>8.2}",
            metric.name, metric.mean, metric.min, metric.max, metric.stddev
        );
    }
    println!(
        "{:<30} {}/{}",
        "collapse_rate", stats.collapsed_count, stats.seed_count
    );
}
```

**Step 2: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_snapshot(
        tick: u64,
        storage_pct: f32,
        fleet_total: u32,
        fleet_idle: u32,
        refinery_starved: u32,
        techs: u32,
        avg_wear: f32,
        repair_kits: u32,
    ) -> MetricsSnapshot {
        MetricsSnapshot {
            tick,
            metrics_version: 3,
            total_ore_kg: 0.0,
            total_material_kg: 0.0,
            total_slag_kg: 0.0,
            total_iron_material_kg: 0.0,
            station_storage_used_pct: storage_pct,
            ship_cargo_used_pct: 0.0,
            avg_ore_fe_fraction: 0.0,
            ore_lot_count: 0,
            min_ore_fe_fraction: 0.0,
            max_ore_fe_fraction: 0.0,
            avg_material_quality: 0.0,
            refinery_active_count: 0,
            refinery_starved_count: refinery_starved,
            refinery_stalled_count: 0,
            assembler_active_count: 0,
            assembler_stalled_count: 0,
            fleet_total,
            fleet_idle,
            fleet_mining: 0,
            fleet_transiting: 0,
            fleet_surveying: 0,
            fleet_depositing: 0,
            scan_sites_remaining: 0,
            asteroids_discovered: 0,
            asteroids_depleted: 0,
            techs_unlocked: techs,
            total_scan_data: 0.0,
            max_tech_evidence: 0.0,
            avg_module_wear: avg_wear,
            max_module_wear: 0.0,
            repair_kits_remaining: repair_kits,
        }
    }

    #[test]
    fn test_summary_basic_stats() {
        let s1 = make_snapshot(100, 0.5, 2, 0, 0, 3, 0.2, 5);
        let s2 = make_snapshot(100, 0.7, 2, 0, 0, 5, 0.4, 3);
        let snapshots: Vec<(u64, &MetricsSnapshot)> = vec![(1, &s1), (2, &s2)];
        let stats = compute_summary(&snapshots);

        assert_eq!(stats.seed_count, 2);
        assert_eq!(stats.collapsed_count, 0);

        let storage = &stats.metrics[0];
        assert_eq!(storage.name, "storage_saturation_pct");
        assert!((storage.mean - 0.6).abs() < 1e-5);
        assert!((storage.min - 0.5).abs() < 1e-5);
        assert!((storage.max - 0.7).abs() < 1e-5);
    }

    #[test]
    fn test_collapse_detection() {
        // Collapsed: refinery_starved > 0 AND fleet_idle == fleet_total
        let collapsed = make_snapshot(100, 0.5, 2, 2, 1, 3, 0.2, 5);
        let healthy = make_snapshot(100, 0.5, 2, 0, 0, 3, 0.2, 5);
        let snapshots: Vec<(u64, &MetricsSnapshot)> = vec![(1, &collapsed), (2, &healthy)];
        let stats = compute_summary(&snapshots);

        assert_eq!(stats.collapsed_count, 1);
    }

    #[test]
    fn test_stddev_zero_for_identical() {
        let s1 = make_snapshot(100, 0.5, 2, 1, 0, 3, 0.2, 5);
        let s2 = make_snapshot(100, 0.5, 2, 1, 0, 3, 0.2, 5);
        let snapshots: Vec<(u64, &MetricsSnapshot)> = vec![(1, &s1), (2, &s2)];
        let stats = compute_summary(&snapshots);

        for metric in &stats.metrics {
            assert!(
                metric.stddev.abs() < 1e-10,
                "stddev for {} should be 0, got {}",
                metric.name,
                metric.stddev
            );
        }
    }
}
```

**Step 3: Wire into main.rs**

Add `mod summary;` to `main.rs`.

**Step 4: Run tests**

Run: `cargo test -p sim_bench`
Expected: All tests pass.

**Step 5: Commit**

```bash
git add crates/sim_bench/
git commit -m "feat(bench): add cross-seed summary statistics and collapse detection"
```

---

### Task 6: Wire everything together in main.rs — parallel execution and output

**Files:**
- Modify: `crates/sim_bench/src/main.rs`

**Step 1: Implement the full `run` function**

Wire together: load scenario → load content → apply overrides → create timestamped output dir → copy scenario file → run seeds in parallel with rayon → collect results → compute summary → print summary → write summary.json.

```rust
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rayon::prelude::*;
use std::path::{Path, PathBuf};

mod overrides;
mod runner;
mod scenario;
mod summary;

#[derive(Parser)]
#[command(name = "sim_bench", about = "Automated scenario runner for sim benchmarking")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a scenario file across multiple seeds.
    Run {
        /// Path to the scenario JSON file.
        #[arg(long)]
        scenario: String,
        /// Output directory (default: runs/).
        #[arg(long, default_value = "runs")]
        output_dir: String,
    },
}

fn run(scenario_path: &str, output_dir: &str) -> Result<()> {
    let scenario = scenario::load_scenario(Path::new(scenario_path))?;
    let seeds = scenario.seeds.expand();

    println!(
        "Loading scenario '{}': {} seeds × {} ticks",
        scenario.name,
        seeds.len(),
        scenario.ticks
    );

    // Load content and apply overrides.
    let mut content = sim_world::load_content(&scenario.content_dir)?;
    overrides::apply_overrides(&mut content.constants, &scenario.overrides)?;

    // Create timestamped output directory.
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let run_dir = PathBuf::from(output_dir).join(format!("{}_{}", scenario.name, timestamp));
    std::fs::create_dir_all(&run_dir)
        .with_context(|| format!("creating output directory: {}", run_dir.display()))?;

    // Copy scenario file into output dir.
    std::fs::copy(scenario_path, run_dir.join("scenario.json"))
        .context("copying scenario file")?;

    println!("Output: {}", run_dir.display());
    println!("Running {} seeds in parallel...", seeds.len());

    // Run all seeds in parallel.
    let results: Vec<Result<runner::SeedResult>> = seeds
        .par_iter()
        .map(|&seed| {
            let seed_dir = run_dir.join(format!("seed_{seed}"));
            runner::run_seed(&content, seed, scenario.ticks, scenario.metrics_every, &seed_dir)
        })
        .collect();

    // Collect results, reporting any failures.
    let mut seed_results = Vec::new();
    for result in results {
        match result {
            Ok(seed_result) => seed_results.push(seed_result),
            Err(err) => eprintln!("Seed failed: {err:#}"),
        }
    }

    if seed_results.is_empty() {
        anyhow::bail!("all seeds failed");
    }

    // Compute and print summary.
    let snapshot_refs: Vec<(u64, &sim_core::MetricsSnapshot)> = seed_results
        .iter()
        .map(|r| (r.seed, &r.final_snapshot))
        .collect();

    let stats = summary::compute_summary(&snapshot_refs);
    summary::print_summary(&scenario.name, scenario.ticks, &stats);

    // Write summary.json
    let summary_path = run_dir.join("summary.json");
    let summary_json = serde_json::to_string_pretty(&stats).context("serializing summary")?;
    std::fs::write(&summary_path, summary_json)
        .with_context(|| format!("writing {}", summary_path.display()))?;

    println!("\nSummary written to {}", summary_path.display());
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            scenario,
            output_dir,
        } => run(&scenario, &output_dir)?,
    }
    Ok(())
}
```

**Step 2: Verify it builds**

Run: `cargo build -p sim_bench`
Expected: Compiles without errors.

**Step 3: Run all tests**

Run: `cargo test -p sim_bench`
Expected: All tests pass.

**Step 4: Commit**

```bash
git add crates/sim_bench/
git commit -m "feat(bench): wire parallel seed execution with rayon and summary output"
```

---

### Task 7: Example scenario file and end-to-end smoke test

**Files:**
- Create: `scenarios/cargo_sweep.json`
- Verify: End-to-end run with real content

**Step 1: Create example scenario**

```json
{
  "name": "cargo_sweep",
  "ticks": 10000,
  "metrics_every": 60,
  "seeds": [1, 2, 3, 4, 5],
  "content_dir": "./content",
  "overrides": {
    "station_cargo_capacity_m3": 200.0,
    "wear_band_degraded_threshold": 0.6
  }
}
```

**Step 2: Run end-to-end**

Run: `cargo run -p sim_bench -- run --scenario scenarios/cargo_sweep.json --output-dir /tmp/bench_test`

Expected:
- Prints scenario name, seed count, ticks
- Runs 5 seeds in parallel
- Prints summary table with 6 metrics + collapse_rate
- Creates `/tmp/bench_test/cargo_sweep_<timestamp>/` with `scenario.json`, `summary.json`, and 5 seed directories each containing `run_info.json` and `metrics_000.csv`

**Step 3: Verify output files exist**

```bash
ls /tmp/bench_test/cargo_sweep_*/
ls /tmp/bench_test/cargo_sweep_*/seed_1/
cat /tmp/bench_test/cargo_sweep_*/summary.json
```

**Step 4: Run `cargo clippy -p sim_bench`**

Expected: No warnings.

**Step 5: Run all workspace tests**

Run: `cargo test`
Expected: All tests pass across all crates.

**Step 6: Commit**

```bash
git add scenarios/ crates/sim_bench/
git commit -m "feat(bench): add example scenario and verify end-to-end execution"
```

---

## Verification Checklist

After all tasks are complete:

```bash
cargo test                     # All crates pass
cargo clippy                   # No warnings
cargo run -p sim_bench -- run --scenario scenarios/cargo_sweep.json --output-dir /tmp/bench_verify
ls /tmp/bench_verify/cargo_sweep_*/seed_*/metrics_000.csv  # 5 CSV files
cat /tmp/bench_verify/cargo_sweep_*/summary.json           # Valid JSON with stats
```
