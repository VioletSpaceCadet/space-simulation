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
        let file = write_temp_scenario(
            r#"{
            "name": "test_scenario",
            "ticks": 1000,
            "seeds": [1, 2, 3]
        }"#,
        );
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
        let file = write_temp_scenario(
            r#"{
            "name": "range_test",
            "ticks": 500,
            "seeds": {"range": [1, 5]}
        }"#,
        );
        let scenario = load_scenario(file.path()).unwrap();
        assert_eq!(scenario.seeds.expand(), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_load_scenario_with_overrides() {
        let file = write_temp_scenario(
            r#"{
            "name": "override_test",
            "ticks": 100,
            "seeds": [42],
            "overrides": {
                "station_cargo_capacity_m3": 200.0,
                "mining_rate_kg_per_tick": 5.0
            }
        }"#,
        );
        let scenario = load_scenario(file.path()).unwrap();
        assert_eq!(scenario.overrides.len(), 2);
    }

    #[test]
    fn test_load_scenario_empty_name_fails() {
        let file = write_temp_scenario(
            r#"{
            "name": "",
            "ticks": 100,
            "seeds": [1]
        }"#,
        );
        let result = load_scenario(file.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name"));
    }

    #[test]
    fn test_load_scenario_zero_ticks_fails() {
        let file = write_temp_scenario(
            r#"{
            "name": "bad",
            "ticks": 0,
            "seeds": [1]
        }"#,
        );
        let result = load_scenario(file.path());
        assert!(result.is_err());
    }
}
