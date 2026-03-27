//! Generic modifier system for applying stacking bonuses/penalties to game stats.
//!
//! Resolution pipeline (4 phases):
//! 1. **Flat** — add/subtract from base value
//! 2. **`PctAdditive`** — sum all %, apply as one multiplier: `× (1 + sum)`
//! 3. **`PctMultiplicative`** — each applied sequentially (sorted by source)
//! 4. **Override** — replaces result entirely (last wins)

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// StatId — every modifiable game stat
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatId {
    // Production
    AssemblyInterval,
    DataGeneration,
    ProcessingInterval,
    ProcessingQuality,
    ProcessingYield,

    // Mining & scanning
    MiningRate,
    ScanDuration,
    ScanInterval,

    // Research
    ResearchSpeed,

    // Ship
    CargoCapacity,
    FuelEfficiency,
    PropellantCapacity,
    ShipSpeed,

    // Station
    BoiloffRate,
    CoolingRate,
    HeatGeneration,
    PowerConsumption,
    PowerOutput,
    SolarOutput,

    // Maintenance
    RepairRate,
    WearRate,
}

// ---------------------------------------------------------------------------
// ModifierOp — how the modifier combines with the base value
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModifierOp {
    /// Phase 1: added directly to base value (+10, -5).
    Flat,
    /// Phase 2: all summed, then applied as `× (1 + sum)`. +0.20 = +20%.
    PctAdditive,
    /// Phase 3: each applied sequentially. 1.5 = ×1.5.
    PctMultiplicative,
    /// Phase 4: replaces the result entirely (caps, immunity). Last wins.
    Override,
}

// ---------------------------------------------------------------------------
// ModifierSource — where the modifier came from (for removal & determinism)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModifierSource {
    /// Crew-based modifier (forward compat — not wired in Phase 1).
    Crew(crate::CrewRole),
    Environment,
    Equipment(String),
    Event(String),
    #[serde(rename = "fitted_module")]
    FittedModule(crate::ModuleDefId, usize),
    Hull(crate::HullId),
    Tech(String),
    Thermal,
    Wear,
}

// ---------------------------------------------------------------------------
// Condition — optional filter for when a modifier applies
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    Station(String),
    Ship(String),
    Module(String),
    ResourceType(String),
}

// ---------------------------------------------------------------------------
// Modifier
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Modifier {
    pub stat: StatId,
    pub op: ModifierOp,
    pub value: f64,
    pub source: ModifierSource,
    /// Reserved for future conditional modifiers (e.g. only applies to a
    /// specific station or resource type). Not evaluated by `resolve()` yet —
    /// callers are responsible for placing modifiers in the correct entity's set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<Condition>,
}

impl Modifier {
    /// Shorthand for a `PctMultiplicative` modifier (the most common type).
    #[must_use]
    pub fn pct_mult(stat: StatId, value: f64, source: ModifierSource) -> Self {
        Self {
            stat,
            op: ModifierOp::PctMultiplicative,
            value,
            source,
            condition: None,
        }
    }
}

// ---------------------------------------------------------------------------
// ModifierSet
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModifierSet {
    modifiers: Vec<Modifier>,
    /// Monotonically increasing counter, bumped on every add/remove.
    /// Used by caches to detect modifier changes (including same-tick
    /// add+remove that leaves `len()` unchanged).
    #[serde(skip, default)]
    generation: u64,
}

/// Equality compares only the modifiers, not the generation counter
/// (which is a transient cache-invalidation aid, not semantic state).
impl PartialEq for ModifierSet {
    fn eq(&self, other: &Self) -> bool {
        self.modifiers == other.modifiers
    }
}

impl ModifierSet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, modifier: Modifier) {
        self.modifiers.push(modifier);
        self.generation += 1;
    }

    /// Remove all modifiers originating from the given source.
    pub fn remove_by_source(&mut self, source: &ModifierSource) {
        let before = self.modifiers.len();
        self.modifiers.retain(|m| m.source != *source);
        if self.modifiers.len() != before {
            self.generation += 1;
        }
    }

    /// Remove all modifiers whose source matches the predicate.
    pub fn remove_where(&mut self, predicate: impl Fn(&ModifierSource) -> bool) {
        let before = self.modifiers.len();
        self.modifiers.retain(|m| !predicate(&m.source));
        if self.modifiers.len() != before {
            self.generation += 1;
        }
    }

    /// Returns a monotonically increasing generation counter that changes
    /// whenever modifiers are added or removed.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Resolve a stat using only this set's modifiers.
    #[must_use]
    pub fn resolve(&self, stat: StatId, base: f64) -> f64 {
        resolve_pipeline(stat, base, self.modifiers.iter())
    }

    /// Resolve a stat and truncate to `f32`. Convenience for call sites
    /// that operate in the sim's native `f32` precision.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn resolve_f32(&self, stat: StatId, base: f32) -> f32 {
        resolve_pipeline(stat, f64::from(base), self.modifiers.iter()) as f32
    }

    /// Resolve a stat by merging this set with a global/parent set.
    /// Both sets' modifiers are evaluated together in the 4-phase pipeline.
    #[must_use]
    pub fn resolve_with(&self, stat: StatId, base: f64, other: &ModifierSet) -> f64 {
        resolve_pipeline(
            stat,
            base,
            self.modifiers.iter().chain(other.modifiers.iter()),
        )
    }

    /// Resolve with merge, returning `f32`. Convenience for the sim's native precision.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn resolve_with_f32(&self, stat: StatId, base: f32, other: &ModifierSet) -> f32 {
        resolve_pipeline(
            stat,
            f64::from(base),
            self.modifiers.iter().chain(other.modifiers.iter()),
        ) as f32
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modifiers.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.modifiers.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Modifier> {
        self.modifiers.iter()
    }
}

// ---------------------------------------------------------------------------
// Resolution pipeline (shared implementation)
// ---------------------------------------------------------------------------

fn resolve_pipeline<'a>(
    stat: StatId,
    base: f64,
    modifiers: impl Iterator<Item = &'a Modifier>,
) -> f64 {
    let mut flat_sum: f64 = 0.0;
    let mut pct_add_sum: f64 = 0.0;
    let mut pct_mults: Vec<(ModifierSource, f64)> = Vec::new();
    let mut overrides: Vec<(ModifierSource, f64)> = Vec::new();

    for modifier in modifiers {
        if modifier.stat != stat {
            continue;
        }
        match modifier.op {
            ModifierOp::Flat => flat_sum += modifier.value,
            ModifierOp::PctAdditive => pct_add_sum += modifier.value,
            ModifierOp::PctMultiplicative => {
                pct_mults.push((modifier.source.clone(), modifier.value));
            }
            ModifierOp::Override => {
                overrides.push((modifier.source.clone(), modifier.value));
            }
        }
    }

    // Sort by source for deterministic ordering.
    pct_mults.sort_by(|a, b| a.0.cmp(&b.0));
    overrides.sort_by(|a, b| a.0.cmp(&b.0));

    // Phase 1: flat
    let mut result = base + flat_sum;
    // Phase 2: pct additive
    result *= 1.0 + pct_add_sum;
    // Phase 3: pct multiplicative (sequential)
    for (_source, value) in &pct_mults {
        result *= value;
    }
    // Phase 4: override (last by source order wins)
    if let Some((_source, value)) = overrides.last() {
        result = *value;
    }

    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Tolerance for floating-point comparison. TOL is too strict
    /// when chaining additions/multiplications of non-representable decimals.
    const TOL: f64 = 1e-10;

    fn make_modifier(stat: StatId, op: ModifierOp, value: f64, source: ModifierSource) -> Modifier {
        Modifier {
            stat,
            op,
            value,
            source,
            condition: None,
        }
    }

    #[test]
    fn resolve_no_modifiers_returns_base() {
        let set = ModifierSet::new();
        assert!((set.resolve(StatId::PowerOutput, 100.0) - 100.0).abs() < TOL);
    }

    #[test]
    fn resolve_flat() {
        let mut set = ModifierSet::new();
        set.add(make_modifier(
            StatId::PowerOutput,
            ModifierOp::Flat,
            10.0,
            ModifierSource::Tech("t1".into()),
        ));
        set.add(make_modifier(
            StatId::PowerOutput,
            ModifierOp::Flat,
            -3.0,
            ModifierSource::Tech("t2".into()),
        ));
        // 100 + 10 + (-3) = 107
        assert!((set.resolve(StatId::PowerOutput, 100.0) - 107.0).abs() < TOL);
    }

    #[test]
    fn resolve_pct_additive() {
        let mut set = ModifierSet::new();
        // +20% and +15% → sum = 0.35 → × 1.35
        set.add(make_modifier(
            StatId::ResearchSpeed,
            ModifierOp::PctAdditive,
            0.20,
            ModifierSource::Tech("t1".into()),
        ));
        set.add(make_modifier(
            StatId::ResearchSpeed,
            ModifierOp::PctAdditive,
            0.15,
            ModifierSource::Tech("t2".into()),
        ));
        let result = set.resolve(StatId::ResearchSpeed, 100.0);
        assert!((result - 135.0).abs() < TOL);
    }

    #[test]
    fn resolve_pct_multiplicative() {
        let mut set = ModifierSet::new();
        // ×0.75 then ×0.50 → 100 × 0.75 × 0.50 = 37.5
        set.add(make_modifier(
            StatId::ProcessingYield,
            ModifierOp::PctMultiplicative,
            0.75,
            ModifierSource::Wear,
        ));
        set.add(make_modifier(
            StatId::ProcessingYield,
            ModifierOp::PctMultiplicative,
            0.50,
            ModifierSource::Thermal,
        ));
        let result = set.resolve(StatId::ProcessingYield, 100.0);
        assert!((result - 37.5).abs() < TOL);
    }

    #[test]
    fn resolve_override() {
        let mut set = ModifierSet::new();
        set.add(make_modifier(
            StatId::PowerOutput,
            ModifierOp::Flat,
            50.0,
            ModifierSource::Tech("t1".into()),
        ));
        set.add(make_modifier(
            StatId::PowerOutput,
            ModifierOp::Override,
            42.0,
            ModifierSource::Tech("cap".into()),
        ));
        // Override replaces everything
        assert!((set.resolve(StatId::PowerOutput, 100.0) - 42.0).abs() < TOL);
    }

    #[test]
    fn resolve_combined_pipeline() {
        let mut set = ModifierSet::new();
        // Base: 100
        // Phase 1 (flat): +10 → 110
        set.add(make_modifier(
            StatId::PowerOutput,
            ModifierOp::Flat,
            10.0,
            ModifierSource::Tech("flat".into()),
        ));
        // Phase 2 (pct_add): +20% → 110 × 1.20 = 132
        set.add(make_modifier(
            StatId::PowerOutput,
            ModifierOp::PctAdditive,
            0.20,
            ModifierSource::Tech("pct".into()),
        ));
        // Phase 3 (pct_mult): ×0.75 → 132 × 0.75 = 99
        set.add(make_modifier(
            StatId::PowerOutput,
            ModifierOp::PctMultiplicative,
            0.75,
            ModifierSource::Wear,
        ));
        let result = set.resolve(StatId::PowerOutput, 100.0);
        assert!((result - 99.0).abs() < TOL);
    }

    #[test]
    fn resolve_ignores_other_stats() {
        let mut set = ModifierSet::new();
        set.add(make_modifier(
            StatId::ResearchSpeed,
            ModifierOp::Flat,
            999.0,
            ModifierSource::Tech("t1".into()),
        ));
        // Resolving PowerOutput should ignore ResearchSpeed modifier
        assert!((set.resolve(StatId::PowerOutput, 100.0) - 100.0).abs() < TOL);
    }

    #[test]
    fn resolve_deterministic_ordering() {
        // Same modifiers inserted in different order must produce the same result
        let modifiers = vec![
            make_modifier(
                StatId::WearRate,
                ModifierOp::PctMultiplicative,
                2.0,
                ModifierSource::Thermal,
            ),
            make_modifier(
                StatId::WearRate,
                ModifierOp::PctMultiplicative,
                0.5,
                ModifierSource::Environment,
            ),
            make_modifier(
                StatId::WearRate,
                ModifierOp::PctMultiplicative,
                1.5,
                ModifierSource::Wear,
            ),
        ];

        let mut set_a = ModifierSet::new();
        for m in modifiers.iter() {
            set_a.add(m.clone());
        }

        let mut set_b = ModifierSet::new();
        // Reverse insertion order
        for m in modifiers.iter().rev() {
            set_b.add(m.clone());
        }

        let result_a = set_a.resolve(StatId::WearRate, 1.0);
        let result_b = set_b.resolve(StatId::WearRate, 1.0);
        assert!(
            (result_a - result_b).abs() < TOL,
            "Different insertion order gave different results: {result_a} vs {result_b}"
        );
    }

    #[test]
    fn remove_by_source() {
        let mut set = ModifierSet::new();
        set.add(make_modifier(
            StatId::PowerOutput,
            ModifierOp::PctMultiplicative,
            0.75,
            ModifierSource::Wear,
        ));
        set.add(make_modifier(
            StatId::PowerOutput,
            ModifierOp::PctMultiplicative,
            0.80,
            ModifierSource::Thermal,
        ));
        set.add(make_modifier(
            StatId::WearRate,
            ModifierOp::PctMultiplicative,
            2.0,
            ModifierSource::Thermal,
        ));
        assert_eq!(set.len(), 3);

        set.remove_by_source(&ModifierSource::Thermal);
        assert_eq!(set.len(), 1);
        // Only Wear modifier remains
        assert!((set.resolve(StatId::PowerOutput, 100.0) - 75.0).abs() < TOL);
    }

    #[test]
    fn resolve_with_merges_sets() {
        let mut global = ModifierSet::new();
        global.add(make_modifier(
            StatId::ResearchSpeed,
            ModifierOp::PctAdditive,
            0.10,
            ModifierSource::Tech("t1".into()),
        ));

        let mut station = ModifierSet::new();
        station.add(make_modifier(
            StatId::ResearchSpeed,
            ModifierOp::PctAdditive,
            0.05,
            ModifierSource::Equipment("lab".into()),
        ));

        // Merged: +10% + +5% = +15% → 100 × 1.15 = 115
        let result = station.resolve_with(StatId::ResearchSpeed, 100.0, &global);
        assert!((result - 115.0).abs() < TOL);
    }

    #[test]
    fn serde_roundtrip() {
        let mut set = ModifierSet::new();
        set.add(Modifier {
            stat: StatId::ProcessingYield,
            op: ModifierOp::PctMultiplicative,
            value: 0.85,
            source: ModifierSource::Tech("tech_advanced_refining".into()),
            condition: Some(Condition::Station("station_alpha".into())),
        });

        let json = serde_json::to_string(&set).expect("serialize");
        let deserialized: ModifierSet = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(set, deserialized);
    }

    #[test]
    fn default_modifier_set_is_empty() {
        let set = ModifierSet::default();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
        // Resolving with empty set returns base
        assert!((set.resolve(StatId::PowerOutput, 42.0) - 42.0).abs() < TOL);
    }

    #[test]
    fn override_deterministic_with_multiple_sources() {
        // Multiple overrides from different sources — last by source order wins.
        let overrides = vec![
            make_modifier(
                StatId::PowerOutput,
                ModifierOp::Override,
                50.0,
                ModifierSource::Wear,
            ),
            make_modifier(
                StatId::PowerOutput,
                ModifierOp::Override,
                30.0,
                ModifierSource::Environment,
            ),
            make_modifier(
                StatId::PowerOutput,
                ModifierOp::Override,
                10.0,
                ModifierSource::Thermal,
            ),
        ];

        let mut set_a = ModifierSet::new();
        for m in overrides.iter() {
            set_a.add(m.clone());
        }

        let mut set_b = ModifierSet::new();
        for m in overrides.iter().rev() {
            set_b.add(m.clone());
        }

        let result_a = set_a.resolve(StatId::PowerOutput, 100.0);
        let result_b = set_b.resolve(StatId::PowerOutput, 100.0);
        assert!(
            (result_a - result_b).abs() < TOL,
            "Different insertion order gave different override results: {result_a} vs {result_b}"
        );
        // Wear sorts last (alphabetically after Thermal and Environment)
        assert!((result_a - 50.0).abs() < TOL);
    }
}
