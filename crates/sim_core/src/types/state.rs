//! Runtime state types: `GameState`, ships, stations, asteroids, modules, tasks.

use super::AHashMap;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

use crate::{
    AnomalyTag, AsteroidId, BodyId, ComponentId, CompositionVec, Constants, CrewRole, DataKind,
    DomainProgress, GameContent, HullId, InventoryItem, LeaderId, ModuleDefId, ModuleInstanceId,
    OverheatZone, Phase, PrincipalId, RecipeId, ShipId, SiteId, StationId, TechId, ThermalGroupId,
    DEFAULT_AMBIENT_TEMP_MK,
};

// ---------------------------------------------------------------------------
// Game state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub meta: MetaState,
    /// Unscanned potential asteroid locations. Populated at world-gen; entries
    /// are removed when surveyed and replaced by a real `AsteroidState`.
    pub scan_sites: Vec<ScanSite>,
    pub asteroids: std::collections::BTreeMap<AsteroidId, AsteroidState>,
    pub ships: std::collections::BTreeMap<ShipId, ShipState>,
    pub stations: std::collections::BTreeMap<StationId, StationState>,
    pub research: ResearchState,
    #[serde(default)]
    pub balance: f64,
    /// Cumulative export revenue since simulation start.
    #[serde(default)]
    pub export_revenue_total: f64,
    /// Total number of export transactions since simulation start.
    #[serde(default)]
    pub export_count: u32,
    pub counters: Counters,
    /// Global modifiers (from research, game-wide buffs).
    #[serde(default)]
    pub modifiers: crate::modifiers::ModifierSet,
    /// Sim events system runtime state.
    #[serde(default)]
    pub events: crate::sim_events::SimEventState,
    /// Cached absolute positions for orbital bodies. Not serialized -- recomputed on load.
    #[serde(skip, default)]
    pub body_cache: AHashMap<BodyId, crate::spatial::BodyCache>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaState {
    pub tick: u64,
    pub seed: u64,
    pub schema_version: u32,
    pub content_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSite {
    pub id: SiteId,
    pub position: crate::Position,
    /// References an `AsteroidTemplateDef` id in `GameContent`.
    pub template_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Counters {
    pub next_event_id: u64,
    pub next_command_id: u64,
    pub next_asteroid_id: u64,
    pub next_lot_id: u64,
    pub next_module_instance_id: u64,
}

// ---------------------------------------------------------------------------
// Module state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleState {
    pub id: ModuleInstanceId,
    pub def_id: String,
    pub enabled: bool,
    pub kind_state: ModuleKindState,
    pub wear: WearState,
    /// Per-module thermal state. None for non-thermal modules.
    #[serde(default)]
    pub thermal: Option<ThermalState>,
    /// Set each tick by power budget computation. Stalled modules skip their tick.
    #[serde(skip, default)]
    pub power_stalled: bool,
    /// Module priority. Higher values run first within each behavior class.
    /// Used to control which modules consume shared inventory first, crew assignment,
    /// and power allocation. 0 = default.
    #[serde(default, alias = "manufacturing_priority")]
    pub module_priority: u32,
    /// Crew assigned to this module, by role. Empty = no crew assigned.
    #[serde(default)]
    pub assigned_crew: BTreeMap<CrewRole, u32>,
    /// Whether all `crew_requirement` roles are met by `assigned_crew`.
    /// Recomputed each tick — not persisted.
    #[serde(skip, default = "default_crew_satisfied")]
    pub crew_satisfied: bool,
}

fn default_crew_satisfied() -> bool {
    true
}

/// Check if assigned crew meets the crew requirement for a module.
/// Empty requirement = always satisfied.
pub fn is_crew_satisfied(
    assigned: &BTreeMap<CrewRole, u32>,
    requirement: &BTreeMap<CrewRole, u32>,
) -> bool {
    requirement
        .iter()
        .all(|(role, &needed)| assigned.get(role).copied().unwrap_or(0) >= needed)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModuleKindState {
    Processor(ProcessorState),
    Storage,
    Maintenance(MaintenanceState),
    Assembler(AssemblerState),
    Lab(LabState),
    SensorArray(SensorArrayState),
    SolarArray(SolarArrayState),
    Battery(BatteryState),
    Radiator(RadiatorState),
    Equipment,
}

impl ModuleKindState {
    /// Returns `true` if this module's kind state indicates it is stalled.
    /// Only Processor and Assembler have a stalled concept; all others return `false`.
    pub fn is_stalled(&self) -> bool {
        match self {
            Self::Processor(s) => s.stalled,
            Self::Assembler(s) => s.stalled,
            _ => false,
        }
    }

    /// Returns a mutable reference to the tick timer, or `None` for non-ticking modules.
    pub fn ticks_since_last_run_mut(&mut self) -> Option<&mut u64> {
        match self {
            Self::Processor(s) => Some(&mut s.ticks_since_last_run),
            Self::Assembler(s) => Some(&mut s.ticks_since_last_run),
            Self::SensorArray(s) => Some(&mut s.ticks_since_last_run),
            Self::Lab(s) => Some(&mut s.ticks_since_last_run),
            Self::Maintenance(s) => Some(&mut s.ticks_since_last_run),
            Self::Storage
            | Self::SolarArray(_)
            | Self::Battery(_)
            | Self::Radiator(_)
            | Self::Equipment => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SensorArrayState {
    pub ticks_since_last_run: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SolarArrayState {
    pub ticks_since_last_run: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryState {
    /// Current stored energy in kWh.
    pub charge_kwh: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RadiatorState {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessorState {
    pub threshold_kg: f32,
    pub ticks_since_last_run: u64,
    #[serde(default)]
    pub stalled: bool,
    #[serde(default)]
    pub selected_recipe: Option<RecipeId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceState {
    pub ticks_since_last_run: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblerState {
    pub ticks_since_last_run: u64,
    #[serde(default)]
    pub stalled: bool,
    #[serde(default)]
    pub capped: bool,
    #[serde(default)]
    pub cap_override: HashMap<ComponentId, u32>,
    #[serde(default)]
    pub selected_recipe: Option<RecipeId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabState {
    pub ticks_since_last_run: u64,
    pub assigned_tech: Option<TechId>,
    #[serde(default)]
    pub starved: bool,
}

// ---------------------------------------------------------------------------
// Asteroid state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsteroidState {
    pub id: AsteroidId,
    pub position: crate::Position,
    /// Ground truth -- never exposed to the UI.
    pub true_composition: CompositionVec,
    pub anomaly_tags: Vec<AnomalyTag>,
    pub mass_kg: f32,
    pub knowledge: AsteroidKnowledge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsteroidKnowledge {
    pub tag_beliefs: Vec<(AnomalyTag, f32)>,
    /// Set after a deep scan. Exact composition -- no uncertainty model.
    pub composition: Option<CompositionVec>,
}

// ---------------------------------------------------------------------------
// Ship state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipState {
    pub id: ShipId,
    pub position: crate::Position,
    pub owner: PrincipalId,
    pub inventory: Vec<InventoryItem>,
    pub cargo_capacity_m3: f32,
    pub task: Option<TaskState>,
    /// Per-ship travel speed. If `None`, uses the global `constants.ticks_per_au`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_ticks_per_au: Option<u64>,
    /// Per-ship modifiers (from equipment, buffs).
    #[serde(default)]
    pub modifiers: crate::modifiers::ModifierSet,
    /// Hull class. Determines base stats, slot layout, and bonuses.
    #[serde(default = "default_hull_id")]
    pub hull_id: HullId,
    /// Modules fitted into hull slots.
    #[serde(default)]
    pub fitted_modules: Vec<FittedModule>,
    /// Current propellant level (kg). Consumed during transit, refueled at stations.
    #[serde(default)]
    pub propellant_kg: f32,
    /// Cached propellant capacity (kg). Recomputed from hull + tank module modifiers.
    #[serde(default)]
    pub propellant_capacity_kg: f32,
    /// Ship crew roster.
    #[serde(default)]
    pub crew: BTreeMap<CrewRole, u32>,
    /// Ship leaders (reserved for Phase 2 leader system).
    #[serde(default)]
    pub leaders: Vec<LeaderId>,
}

fn default_hull_id() -> HullId {
    HullId("hull_general_purpose".to_string())
}

impl ShipState {
    /// Returns this ship's travel speed, falling back to the global default.
    pub fn ticks_per_au(&self, global_default: u64) -> u64 {
        self.speed_ticks_per_au.unwrap_or(global_default)
    }

    /// Ship mass without propellant or cargo: hull + fitted module masses.
    pub fn dry_mass_kg(&self, content: &GameContent) -> f32 {
        let hull_mass = content.hulls.get(&self.hull_id).map_or(0.0, |h| h.mass_kg);
        let module_mass: f32 = self
            .fitted_modules
            .iter()
            .filter_map(|fm| content.module_defs.get(fm.module_def_id.0.as_str()))
            .map(|def| def.mass_kg)
            .sum();
        hull_mass + module_mass
    }

    /// Total ship mass: dry mass + propellant + cargo.
    pub fn total_mass_kg(&self, content: &GameContent) -> f32 {
        self.dry_mass_kg(content)
            + self.propellant_kg
            + crate::tasks::inventory_mass_kg(&self.inventory)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FittedModule {
    pub slot_index: usize,
    pub module_def_id: ModuleDefId,
}

// ---------------------------------------------------------------------------
// Station state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PowerState {
    pub generated_kw: f32,
    pub consumed_kw: f32,
    pub deficit_kw: f32,
    /// Power discharged from batteries this tick (kW).
    pub battery_discharge_kw: f32,
    /// Power stored into batteries this tick (kW).
    pub battery_charge_kw: f32,
    /// Total energy stored across all batteries (kWh).
    pub battery_stored_kwh: f32,
}

/// Cached power generation/consumption summary. Avoids re-iterating all modules
/// and looking up defs every tick when nothing has changed. Battery buffering and
/// stall logic still run every tick using the cached values.
#[derive(Debug, Clone, Default)]
pub struct PowerBudgetCache {
    /// When false, the cache must be rebuilt before use.
    pub(crate) valid: bool,
    /// Sum of solar generation (after wear + solar intensity + modifiers).
    pub generated_kw: f32,
    /// Sum of all enabled modules' `power_consumption_per_run`.
    pub consumed_kw: f32,
    /// Whether any solar array or battery module exists (gates stall logic).
    pub has_power_infrastructure: bool,
    /// `(module_index, priority, consumption_kw)` for stall ordering.
    pub consumers: Vec<(usize, u8, f32)>,
    /// `(module_index, battery_def, efficiency)` for battery buffering.
    pub battery_entries: Vec<(usize, crate::BatteryDef, f32)>,
    /// `(module_index, wear_per_run)` for solar array wear application.
    pub solar_wear_targets: Vec<(usize, f32)>,
    /// Snapshot of wear bands for power-related modules. If any band changes,
    /// the cache is automatically invalidated.
    pub(crate) wear_band_snapshot: Vec<(usize, u8)>,
    /// Snapshot of global modifier generation at cache time.
    pub(crate) global_modifier_generation: u64,
    /// Snapshot of `(module_count, enabled_count)` at cache time. Detects direct
    /// state mutations that bypass command handlers.
    pub(crate) module_enabled_snapshot: (usize, usize),
}

impl PowerBudgetCache {
    /// Returns true if the cache is valid and can be reused.
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    /// Mark the cache as needing rebuild.
    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    /// Mark the cache as freshly built.
    pub fn mark_valid(&mut self) {
        self.valid = true;
    }
}

/// Pre-computed index of module indices by subsystem type.
/// Rebuilt on module install/uninstall. Each subsystem iterates only its
/// matching indices instead of scanning all modules every tick.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModuleTypeIndex {
    /// False until `rebuild_module_index` has been called at least once.
    initialized: bool,
    pub processors: Vec<usize>,
    pub assemblers: Vec<usize>,
    pub sensors: Vec<usize>,
    pub labs: Vec<usize>,
    pub maintenance: Vec<usize>,
    /// Modules with a `ThermalDef` (cross-cutting, any behavior type).
    pub thermal: Vec<usize>,
}

impl ModuleTypeIndex {
    /// Returns true if the index has been built at least once.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationState {
    pub id: StationId,
    pub position: crate::Position,
    pub inventory: Vec<InventoryItem>,
    pub cargo_capacity_m3: f32,
    pub power_available_per_tick: f32,
    pub modules: Vec<ModuleState>,
    /// Per-station modifiers (from equipment, location).
    #[serde(default)]
    pub modifiers: crate::modifiers::ModifierSet,
    /// Station crew roster: how many of each role are available.
    #[serde(default)]
    pub crew: BTreeMap<CrewRole, u32>,
    /// Station leaders (reserved for Phase 2 leader system).
    #[serde(default)]
    pub leaders: Vec<LeaderId>,
    /// Computed fresh each tick -- not persisted across ticks.
    #[serde(skip_deserializing, default)]
    pub power: PowerState,
    /// Cached inventory volume. Set to `None` when inventory changes;
    /// recomputed lazily via [`StationState::used_volume_m3`].
    #[serde(skip, default)]
    pub cached_inventory_volume_m3: Option<f32>,
    /// Pre-computed module-type → indices mapping. Rebuilt on install/uninstall.
    #[serde(skip, default)]
    pub module_type_index: ModuleTypeIndex,
    /// Cached power generation/consumption values. Avoids re-iterating modules
    /// when nothing power-relevant has changed.
    #[serde(skip, default)]
    pub power_budget_cache: PowerBudgetCache,
}

impl StationState {
    /// Get the cached inventory volume, computing and caching if needed.
    pub fn used_volume_m3(&mut self, content: &GameContent) -> f32 {
        if let Some(vol) = self.cached_inventory_volume_m3 {
            return vol;
        }
        let vol = crate::tasks::inventory_volume_m3(&self.inventory, content);
        self.cached_inventory_volume_m3 = Some(vol);
        vol
    }

    /// Invalidate the cached volume. Call after any inventory mutation.
    pub fn invalidate_volume_cache(&mut self) {
        self.cached_inventory_volume_m3 = None;
    }

    /// Invalidate the power budget cache. Call after module enable/disable,
    /// install/uninstall, or any change that affects power generation or consumption.
    pub fn invalidate_power_cache(&mut self) {
        self.power_budget_cache.invalidate();
    }

    /// Rebuild the module type index from the current modules list and content defs.
    /// Call after install/uninstall or initial station construction.
    /// Returns how many crew of a given role are available (not assigned to modules).
    pub fn available_crew(&self, role: &CrewRole) -> u32 {
        let total = self.crew.get(role).copied().unwrap_or(0);
        let assigned: u32 = self
            .modules
            .iter()
            .map(|m| m.assigned_crew.get(role).copied().unwrap_or(0))
            .sum();
        total.saturating_sub(assigned)
    }

    /// Initialize `crew_satisfied` on all modules based on content requirements.
    /// Call after loading state to avoid spurious transition events on first tick.
    pub fn init_crew_satisfaction(&mut self, content: &GameContent) {
        for module in &mut self.modules {
            if let Some(def) = content.module_defs.get(&module.def_id) {
                module.crew_satisfied =
                    is_crew_satisfied(&module.assigned_crew, &def.crew_requirement);
            }
        }
    }

    pub fn rebuild_module_index(&mut self, content: &GameContent) {
        let idx = &mut self.module_type_index;
        idx.initialized = true;
        idx.processors.clear();
        idx.assemblers.clear();
        idx.sensors.clear();
        idx.labs.clear();
        idx.maintenance.clear();
        idx.thermal.clear();

        for (i, module) in self.modules.iter().enumerate() {
            if let Some(def) = content.module_defs.get(&module.def_id) {
                match &def.behavior {
                    crate::ModuleBehaviorDef::Processor(_) => idx.processors.push(i),
                    crate::ModuleBehaviorDef::Assembler(_) => idx.assemblers.push(i),
                    crate::ModuleBehaviorDef::SensorArray(_) => idx.sensors.push(i),
                    crate::ModuleBehaviorDef::Lab(_) => idx.labs.push(i),
                    crate::ModuleBehaviorDef::Maintenance(_) => idx.maintenance.push(i),
                    _ => {}
                }
                if def.thermal.is_some() {
                    idx.thermal.push(i);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Research state
// ---------------------------------------------------------------------------

/// Research distributes automatically to all eligible techs -- no player allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchState {
    pub unlocked: HashSet<TechId>,
    pub data_pool: AHashMap<DataKind, f32>,
    pub evidence: AHashMap<TechId, DomainProgress>,
    #[serde(default)]
    pub action_counts: AHashMap<String, u64>,
}

// ---------------------------------------------------------------------------
// Task state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub kind: TaskKind,
    pub started_tick: u64,
    pub eta_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskKind {
    Idle,
    /// Ship is in transit. On arrival it will immediately start `then`.
    Transit {
        destination: crate::Position,
        /// Pre-computed total travel ticks.
        total_ticks: u64,
        then: Box<TaskKind>,
    },
    Survey {
        site: SiteId,
    },
    DeepScan {
        asteroid: AsteroidId,
    },
    Mine {
        asteroid: AsteroidId,
        /// Pre-computed mining duration (ticks), computed at task assignment.
        duration_ticks: u64,
    },
    Deposit {
        station: StationId,
        #[serde(default)]
        blocked: bool,
    },
    /// Ship is refueling at a station. Ongoing task — resolved every tick, no fixed eta.
    Refuel {
        station_id: StationId,
        target_kg: f32,
    },
}

impl TaskKind {
    /// Task duration in ticks. Returns 0 for ongoing tasks (Refuel).
    pub fn duration(&self, constants: &Constants) -> u64 {
        match self {
            Self::Transit { total_ticks, .. } => *total_ticks,
            Self::Survey { .. } => constants.survey_scan_ticks,
            Self::DeepScan { .. } => constants.deep_scan_ticks,
            Self::Mine { duration_ticks, .. } => *duration_ticks,
            Self::Deposit { .. } => constants.deposit_ticks,
            Self::Idle | Self::Refuel { .. } => 0,
        }
    }

    /// Human-readable label for this task kind.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Transit { .. } => "Transit",
            Self::Survey { .. } => "Survey",
            Self::DeepScan { .. } => "DeepScan",
            Self::Mine { .. } => "Mine",
            Self::Deposit { .. } => "Deposit",
            Self::Refuel { .. } => "Refuel",
        }
    }

    /// Target entity ID (if any) for display/events.
    pub fn target(&self) -> Option<String> {
        match self {
            Self::Idle => None,
            Self::Transit { destination, .. } => Some(destination.parent_body.0.clone()),
            Self::Survey { site } => Some(site.0.clone()),
            Self::DeepScan { asteroid } | Self::Mine { asteroid, .. } => Some(asteroid.0.clone()),
            Self::Deposit { station, .. }
            | Self::Refuel {
                station_id: station,
                ..
            } => Some(station.0.clone()),
        }
    }
}

// ---------------------------------------------------------------------------
// Wear system
// ---------------------------------------------------------------------------

/// Standalone wear state, embedded wherever wear applies.
/// Generic -- used by station modules now, ships later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WearState {
    pub wear: f32,
}

impl Default for WearState {
    fn default() -> Self {
        Self { wear: 0.0 }
    }
}

// ---------------------------------------------------------------------------
// Thermal state
// ---------------------------------------------------------------------------

/// Per-module thermal state, tracked in milli-Kelvin for deterministic integer arithmetic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThermalState {
    /// Temperature in milli-Kelvin (e.g. `293_000` = 20 C ambient).
    pub temp_mk: u32,
    /// Which thermal group this module belongs to (shared with `ThermalDef`).
    pub thermal_group: Option<ThermalGroupId>,
    /// Current overheat zone -- used for transition detection and wear multiplier.
    #[serde(default)]
    pub overheat_zone: OverheatZone,
    /// Whether this module was auto-disabled by the overheat system.
    /// Used to distinguish overheat-disabled from player-disabled or wear-disabled.
    #[serde(default)]
    pub overheat_disabled: bool,
}

impl Default for ThermalState {
    fn default() -> Self {
        Self {
            temp_mk: DEFAULT_AMBIENT_TEMP_MK,
            thermal_group: None,
            overheat_zone: OverheatZone::default(),
            overheat_disabled: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Material thermal properties
// ---------------------------------------------------------------------------

/// Thermal properties attached to a `Material` inventory item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialThermalProps {
    /// Temperature in milli-Kelvin.
    pub temp_mk: u32,
    /// Current phase of the material batch.
    pub phase: Phase,
    /// Latent heat buffer in joules (tracks energy absorbed/released during phase change).
    pub latent_heat_buffer_j: i64,
}

impl Default for MaterialThermalProps {
    fn default() -> Self {
        Self {
            temp_mk: DEFAULT_AMBIENT_TEMP_MK,
            phase: Phase::Solid,
            latent_heat_buffer_j: 0,
        }
    }
}
