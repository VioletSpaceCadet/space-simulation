//! Constants struct and default value functions.

use serde::{Deserialize, Serialize};

use crate::DEFAULT_AMBIENT_TEMP_MK;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constants {
    // -- Game-time fields (deserialized from JSON) --
    pub survey_scan_minutes: u64,
    pub deep_scan_minutes: u64,
    pub survey_tag_detection_probability: f32,
    pub asteroid_count_per_template: u32,
    pub asteroid_mass_min_kg: f32,
    pub asteroid_mass_max_kg: f32,
    pub ship_cargo_capacity_m3: f32,
    pub station_cargo_capacity_m3: f32,
    /// kg of raw ore extracted per game-time minute of mining
    pub mining_rate_kg_per_minute: f32,
    pub deposit_minutes: u64,
    pub station_power_available_per_minute: f32,
    /// H2O inventory (kg) below which autopilot prioritizes volatile-rich mining.
    #[serde(default = "default_autopilot_volatile_threshold_kg")]
    pub autopilot_volatile_threshold_kg: f32,
    /// Default refinery processing threshold (kg) set by autopilot on newly installed modules.
    pub autopilot_refinery_threshold_kg: f32,
    // Research system
    pub data_generation_peak: f32,
    pub data_generation_floor: f32,
    pub data_generation_decay_rate: f32,
    // Autopilot slag management
    /// Station storage usage % at which autopilot jettisons all slag.
    /// Default 0.75 (75%). Set to 1.0+ to disable auto-jettison.
    #[serde(default = "default_slag_jettison_pct")]
    pub autopilot_slag_jettison_pct: f32,
    // Wear system
    pub wear_band_degraded_threshold: f32,
    pub wear_band_critical_threshold: f32,
    pub wear_band_degraded_efficiency: f32,
    pub wear_band_critical_efficiency: f32,
    // Time scale
    /// Game-time minutes per simulation tick. Production = 60 (1 tick = 1 hour).
    /// Test fixtures use 1 to preserve existing assertions.
    pub minutes_per_tick: u32,
    // Autopilot export
    /// Max kg per material export command per tick. Prevents dumping entire stockpile at once.
    #[serde(default = "default_autopilot_export_batch_size_kg")]
    pub autopilot_export_batch_size_kg: f32,
    /// Skip exports that would yield less than this revenue. Avoids micro-transactions.
    #[serde(default = "default_autopilot_export_min_revenue")]
    pub autopilot_export_min_revenue: f64,
    /// LH2 inventory threshold for propellant pipeline management.
    /// Below this: ensure electrolysis enabled. Above 2x this: disable to save power.
    #[serde(default = "default_autopilot_lh2_threshold_kg")]
    pub autopilot_lh2_threshold_kg: f32,
    // Spatial system
    /// Max distance (micro-AU) for docking/deposit operations. Ships must be within this range.
    #[serde(default = "default_docking_range_au_um")]
    pub docking_range_au_um: u64,
    /// Ticks to cross 1 AU. Calibrate so Earth->Inner Belt ~ 2,880 ticks.
    #[serde(default = "default_ticks_per_au")]
    pub ticks_per_au: u64,
    /// Floor for short trips (e.g., same-zone travel).
    #[serde(default = "default_min_transit_ticks")]
    pub min_transit_ticks: u64,
    /// How often (in ticks) to check whether new scan sites should spawn.
    #[serde(default = "default_replenish_check_interval_ticks")]
    pub replenish_check_interval_ticks: u64,
    /// Target number of unscanned scan sites. Deficit is spawned each check.
    #[serde(default = "default_replenish_target_count")]
    pub replenish_target_count: u32,
    // Thermal system
    /// Ambient/radiator sink temperature in milli-Kelvin (20 C, not cosmic background).
    #[serde(default = "default_thermal_sink_temp_mk")]
    pub thermal_sink_temp_mk: u32,
    /// Offset above max operating temp that triggers overheat warning.
    #[serde(default = "default_thermal_overheat_warning_offset_mk")]
    pub thermal_overheat_warning_offset_mk: u32,
    /// Offset above max operating temp that triggers overheat critical.
    #[serde(default = "default_thermal_overheat_critical_offset_mk")]
    pub thermal_overheat_critical_offset_mk: u32,
    /// Offset above max operating temp that triggers overheat damage.
    #[serde(default = "default_thermal_overheat_damage_offset_mk")]
    pub thermal_overheat_damage_offset_mk: u32,
    /// Wear rate multiplier when module is in overheat warning zone.
    #[serde(default = "default_thermal_wear_multiplier_warning")]
    pub thermal_wear_multiplier_warning: f32,
    /// Wear rate multiplier when module is in overheat critical zone.
    #[serde(default = "default_thermal_wear_multiplier_critical")]
    pub thermal_wear_multiplier_critical: f32,
    // Misc sim_core constants (previously hardcoded)
    /// Hard ceiling temperature in milli-Kelvin to prevent unbounded growth (10,000 K).
    #[serde(default = "default_t_max_absolute_mk")]
    pub t_max_absolute_mk: u32,
    /// Minimum meaningful mass in kg; amounts below this are treated as zero.
    #[serde(default = "default_min_meaningful_kg")]
    pub min_meaningful_kg: f32,
    /// Number of scan sites to spawn per replenishment batch.
    #[serde(default = "default_replenish_batch_size")]
    pub replenish_batch_size: usize,
    /// Trade (import/export) unlocks after this many game-minutes (default: 1 year = 525,600).
    #[serde(default = "default_trade_unlock_delay_minutes")]
    pub trade_unlock_delay_minutes: u64,
    /// Autopilot won't spend more than this fraction of balance on a single import.
    #[serde(default = "default_autopilot_budget_cap_fraction")]
    pub autopilot_budget_cap_fraction: f64,
    /// Multiplier for LH2 abundance threshold (disable electrolysis when LH2 > threshold * this).
    #[serde(default = "default_autopilot_lh2_abundant_multiplier")]
    pub autopilot_lh2_abundant_multiplier: f32,
    /// Temperature offset above ambient for the "hot" boiloff multiplier zone (milli-Kelvin).
    #[serde(default = "default_boiloff_hot_offset_mk")]
    pub boiloff_hot_offset_mk: u32,
    // Sim events system
    /// Whether the sim events system is enabled.
    #[serde(default = "default_events_enabled")]
    pub events_enabled: bool,
    /// Global cooldown between any two sim events (ticks).
    #[serde(default = "default_event_global_cooldown_ticks")]
    pub event_global_cooldown_ticks: u64,
    /// Maximum number of fired events to keep in history ring buffer.
    #[serde(default = "default_event_history_capacity")]
    pub event_history_capacity: usize,
    // Propulsion system
    /// Fuel consumed (kg) per AU of travel for a reference-mass ship.
    #[serde(default = "default_fuel_cost_per_au")]
    pub fuel_cost_per_au: f32,
    /// Reference ship mass (kg) for fuel cost scaling. Ships heavier than this burn more.
    #[serde(default = "default_reference_mass_kg")]
    pub reference_mass_kg: f32,
    // Bottleneck detection (used by daemon analytics)
    /// Station storage usage fraction above which bottleneck detection flags `StorageFull`.
    #[serde(default = "default_bottleneck_storage_threshold_pct")]
    pub bottleneck_storage_threshold_pct: f32,
    /// Slag-to-material ratio above which bottleneck detection flags `SlagBackpressure`.
    #[serde(default = "default_bottleneck_slag_ratio_threshold")]
    pub bottleneck_slag_ratio_threshold: f32,
    /// Max module wear above which bottleneck detection flags `WearCritical`.
    #[serde(default = "default_bottleneck_wear_threshold")]
    pub bottleneck_wear_threshold: f32,

    // -- Derived tick fields (computed at load time, not in JSON) --
    #[serde(skip_deserializing, default)]
    pub survey_scan_ticks: u64,
    #[serde(skip_deserializing, default)]
    pub deep_scan_ticks: u64,
    #[serde(skip_deserializing, default)]
    pub mining_rate_kg_per_tick: f32,
    #[serde(skip_deserializing, default)]
    pub deposit_ticks: u64,
    #[serde(skip_deserializing, default)]
    pub station_power_available_per_tick: f32,
}

impl Constants {
    /// Convert a game-time duration in minutes to ticks, rounding up (ceil division).
    ///
    /// # Panics
    /// Debug-asserts that `minutes_per_tick > 0`.
    pub fn game_minutes_to_ticks(&self, minutes: u64) -> u64 {
        debug_assert!(self.minutes_per_tick > 0, "minutes_per_tick must be > 0");
        let mpt = u64::from(self.minutes_per_tick);
        if minutes == 0 {
            return 0;
        }
        minutes.div_ceil(mpt)
    }

    /// Convert a per-minute rate to a per-tick rate.
    pub fn rate_per_minute_to_per_tick(&self, rate_per_minute: f32) -> f32 {
        rate_per_minute * self.minutes_per_tick as f32
    }

    /// Convert a tick number to game-day (0-indexed). Each day is 1440 game-minutes.
    pub fn tick_to_game_day(&self, tick: u64) -> u64 {
        let total_minutes = tick * u64::from(self.minutes_per_tick);
        total_minutes / 1440
    }

    /// Convert a tick number to hour-of-day (0..23).
    pub fn tick_to_game_hour(&self, tick: u64) -> u64 {
        let total_minutes = tick * u64::from(self.minutes_per_tick);
        (total_minutes % 1440) / 60
    }

    /// Compute derived tick-based fields from game-time minutes fields.
    /// Must be called once after deserialization (in `load_content` / after overrides).
    pub fn derive_tick_values(&mut self) {
        self.survey_scan_ticks = self.game_minutes_to_ticks(self.survey_scan_minutes);
        self.deep_scan_ticks = self.game_minutes_to_ticks(self.deep_scan_minutes);
        self.deposit_ticks = self.game_minutes_to_ticks(self.deposit_minutes);
        self.mining_rate_kg_per_tick =
            self.rate_per_minute_to_per_tick(self.mining_rate_kg_per_minute);
        self.station_power_available_per_tick =
            self.rate_per_minute_to_per_tick(self.station_power_available_per_minute);
    }
}

// ---------------------------------------------------------------------------
// Default value functions (used by serde)
// ---------------------------------------------------------------------------

fn default_slag_jettison_pct() -> f32 {
    0.75
}
fn default_autopilot_volatile_threshold_kg() -> f32 {
    500.0
}

fn default_autopilot_export_batch_size_kg() -> f32 {
    500.0
}

fn default_autopilot_export_min_revenue() -> f64 {
    1_000.0
}

fn default_autopilot_lh2_threshold_kg() -> f32 {
    5000.0
}

fn default_thermal_sink_temp_mk() -> u32 {
    DEFAULT_AMBIENT_TEMP_MK
}
fn default_thermal_overheat_warning_offset_mk() -> u32 {
    200_000
}
fn default_thermal_overheat_critical_offset_mk() -> u32 {
    500_000
}
fn default_thermal_overheat_damage_offset_mk() -> u32 {
    800_000
}
fn default_thermal_wear_multiplier_warning() -> f32 {
    2.0
}
fn default_thermal_wear_multiplier_critical() -> f32 {
    4.0
}
fn default_docking_range_au_um() -> u64 {
    10_000 // ~1.5 million km
}
fn default_ticks_per_au() -> u64 {
    2_133 // calibrated so Earth->Inner Belt ~ 2,880 ticks
}
fn default_min_transit_ticks() -> u64 {
    1
}
fn default_replenish_check_interval_ticks() -> u64 {
    1 // check every tick (backward compat)
}
fn default_replenish_target_count() -> u32 {
    5 // matches legacy MIN_UNSCANNED_SITES
}
fn default_t_max_absolute_mk() -> u32 {
    10_000_000
}
fn default_min_meaningful_kg() -> f32 {
    1e-3
}
fn default_replenish_batch_size() -> usize {
    5
}
fn default_trade_unlock_delay_minutes() -> u64 {
    365 * 24 * 60
}
fn default_autopilot_budget_cap_fraction() -> f64 {
    0.05
}
fn default_autopilot_lh2_abundant_multiplier() -> f32 {
    2.0
}
fn default_boiloff_hot_offset_mk() -> u32 {
    100_000
}
fn default_events_enabled() -> bool {
    true
}
fn default_event_global_cooldown_ticks() -> u64 {
    200
}
fn default_event_history_capacity() -> usize {
    100
}
fn default_fuel_cost_per_au() -> f32 {
    500.0 // 500 kg LH2 per AU for a reference-mass ship
}
fn default_reference_mass_kg() -> f32 {
    15_000.0 // reference mass for fuel scaling
}
fn default_bottleneck_storage_threshold_pct() -> f32 {
    0.95
}
fn default_bottleneck_slag_ratio_threshold() -> f32 {
    0.5
}
fn default_bottleneck_wear_threshold() -> f32 {
    0.8
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod time_scale_tests {
    use crate::test_fixtures::base_content;

    #[test]
    fn game_minutes_to_ticks_exact_division() {
        let mut c = base_content();
        c.constants.minutes_per_tick = 60;
        assert_eq!(c.constants.game_minutes_to_ticks(120), 2);
    }

    #[test]
    fn game_minutes_to_ticks_rounds_up() {
        let mut c = base_content();
        c.constants.minutes_per_tick = 60;
        assert_eq!(c.constants.game_minutes_to_ticks(30), 1);
    }

    #[test]
    fn game_minutes_to_ticks_mpt_1() {
        let c = base_content();
        assert_eq!(c.constants.game_minutes_to_ticks(120), 120);
    }

    #[test]
    fn game_minutes_to_ticks_zero() {
        let c = base_content();
        assert_eq!(c.constants.game_minutes_to_ticks(0), 0);
    }

    #[test]
    fn rate_per_minute_to_per_tick_60() {
        let mut c = base_content();
        c.constants.minutes_per_tick = 60;
        let result = c.constants.rate_per_minute_to_per_tick(15.0);
        assert!((result - 900.0).abs() < f32::EPSILON);
    }

    #[test]
    fn rate_per_minute_to_per_tick_1() {
        let c = base_content();
        let result = c.constants.rate_per_minute_to_per_tick(15.0);
        assert!((result - 15.0).abs() < f32::EPSILON);
    }

    #[test]
    fn derive_tick_values_mpt_60() {
        let mut c = base_content();
        c.constants.minutes_per_tick = 60;
        c.constants.survey_scan_minutes = 120;
        c.constants.deep_scan_minutes = 480;
        c.constants.deposit_minutes = 120;
        c.constants.mining_rate_kg_per_minute = 15.0;
        c.constants.station_power_available_per_minute = 100.0;
        c.constants.derive_tick_values();

        assert_eq!(c.constants.survey_scan_ticks, 2);
        assert_eq!(c.constants.deep_scan_ticks, 8);
        assert_eq!(c.constants.deposit_ticks, 2);
        assert!((c.constants.mining_rate_kg_per_tick - 900.0).abs() < f32::EPSILON);
        assert!((c.constants.station_power_available_per_tick - 6000.0).abs() < f32::EPSILON);
    }

    #[test]
    fn derive_tick_values_mpt_1() {
        let mut c = base_content();
        // base_content uses minutes_per_tick=1 and all _minutes fields = 1
        c.constants.derive_tick_values();

        assert_eq!(c.constants.survey_scan_ticks, 1);
        assert_eq!(c.constants.deep_scan_ticks, 1);
        assert_eq!(c.constants.deposit_ticks, 1);
    }

    #[test]
    fn tick_to_game_day_mpt_60() {
        let mut c = base_content();
        c.constants.minutes_per_tick = 60;
        // 24 ticks * 60 min = 1440 min = 1 day
        assert_eq!(c.constants.tick_to_game_day(0), 0);
        assert_eq!(c.constants.tick_to_game_day(23), 0);
        assert_eq!(c.constants.tick_to_game_day(24), 1);
        assert_eq!(c.constants.tick_to_game_day(48), 2);
    }

    #[test]
    fn tick_to_game_hour_mpt_60() {
        let mut c = base_content();
        c.constants.minutes_per_tick = 60;
        assert_eq!(c.constants.tick_to_game_hour(0), 0);
        assert_eq!(c.constants.tick_to_game_hour(1), 1);
        assert_eq!(c.constants.tick_to_game_hour(23), 23);
        // Wraps at day boundary
        assert_eq!(c.constants.tick_to_game_hour(24), 0);
        assert_eq!(c.constants.tick_to_game_hour(25), 1);
    }

    #[test]
    fn tick_to_game_day_mpt_1() {
        let c = base_content();
        // minutes_per_tick = 1; 1440 ticks = 1 day
        assert_eq!(c.constants.tick_to_game_day(0), 0);
        assert_eq!(c.constants.tick_to_game_day(1439), 0);
        assert_eq!(c.constants.tick_to_game_day(1440), 1);
    }

    #[test]
    fn tick_to_game_hour_mpt_1() {
        let c = base_content();
        // minutes_per_tick = 1; 60 ticks = 1 hour
        assert_eq!(c.constants.tick_to_game_hour(0), 0);
        assert_eq!(c.constants.tick_to_game_hour(59), 0);
        assert_eq!(c.constants.tick_to_game_hour(60), 1);
        assert_eq!(c.constants.tick_to_game_hour(1439), 23);
        assert_eq!(c.constants.tick_to_game_hour(1440), 0);
    }
}
