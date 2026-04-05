//! Runtime state types: `GameState`, ships, stations, asteroids, modules, tasks.

use super::AHashMap;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

use crate::{
    AnomalyTag, AsteroidId, BodyId, ComponentId, CompositionVec, Constants, CrewRole, DataKind,
    DomainProgress, FrameId, GameContent, HullId, InventoryItem, LeaderId, ModuleDefId,
    ModuleInstanceId, OverheatZone, Phase, PrincipalId, RecipeId, SatelliteId, ShipId, SiteId,
    StationId, TechId, ThermalGroupId, DEFAULT_AMBIENT_TEMP_MK,
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
    /// Earth-based (or planetary surface) operations centers.
    #[serde(default)]
    pub ground_facilities: std::collections::BTreeMap<crate::GroundFacilityId, GroundFacilityState>,
    /// Deployed satellites (survey, comm relay, nav beacon, science platform).
    #[serde(default)]
    pub satellites: std::collections::BTreeMap<SatelliteId, SatelliteState>,
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
    /// Cumulative propellant consumed (kg) since simulation start.
    #[serde(default)]
    pub propellant_consumed_total: f64,
    /// Progression system state (milestones, phase, grants, trade tier).
    #[serde(default)]
    pub progression: crate::ProgressionState,
    /// Strategic configuration that shapes high-level autopilot behavior.
    /// Seeded from `GameContent.default_strategy` at world-gen and can be
    /// replaced at runtime via `Command::SetStrategyConfig` (VIO-483).
    #[serde(default)]
    pub strategy_config: crate::StrategyConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Counters {
    pub next_event_id: u64,
    pub next_command_id: u64,
    pub next_asteroid_id: u64,
    pub next_lot_id: u64,
    pub next_module_instance_id: u64,
    /// Stations deployed from ground facility launches (`StationKit` payload).
    #[serde(default)]
    pub stations_deployed: u64,
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
    /// Index into the station frame's `slots` vec. `None` for frameless
    /// (legacy) stations and for modules fitted on ships.
    #[serde(default)]
    pub slot_index: Option<usize>,
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
    /// Combined efficiency multiplier (0.0–1.0). Product of power, crew, and
    /// wear factors. Recomputed each tick — not persisted.
    #[serde(skip, default = "default_efficiency")]
    pub efficiency: f32,
    /// Tracks previous crew satisfaction for transition event detection.
    #[serde(skip, default = "default_prev_crew_satisfied")]
    pub prev_crew_satisfied: bool,
}

fn default_efficiency() -> f32 {
    1.0
}

fn default_prev_crew_satisfied() -> bool {
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

/// Compute crew factor as min(assigned/required) across all roles, capped at 1.0.
/// Empty requirement = 1.0. Returns 0.0 if any required role has zero assigned.
pub fn compute_crew_factor(
    assigned: &BTreeMap<CrewRole, u32>,
    requirement: &BTreeMap<CrewRole, u32>,
) -> f32 {
    if requirement.is_empty() {
        return 1.0;
    }
    let mut min_factor = 1.0f32;
    for (role, &needed) in requirement {
        if needed == 0 {
            continue;
        }
        let have = assigned.get(role).copied().unwrap_or(0);
        min_factor = min_factor.min(have as f32 / needed as f32);
    }
    min_factor.min(1.0)
}

/// Compute the combined efficiency multiplier for a module.
/// Product of: power factor (0 if stalled), crew factor, wear factor.
pub fn compute_module_efficiency(
    module: &ModuleState,
    def: &crate::ModuleDef,
    constants: &crate::Constants,
) -> f32 {
    let power_factor = if module.power_stalled { 0.0 } else { 1.0 };
    let crew_factor = compute_crew_factor(&module.assigned_crew, &def.crew_requirement);
    let wear_factor = crate::wear::wear_efficiency(module.wear.wear, constants);
    power_factor * crew_factor * wear_factor
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
    LaunchPad(LaunchPadState),
    Equipment,
    ThermalContainer(ThermalContainerState),
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
            | Self::LaunchPad(_)
            | Self::Equipment
            | Self::ThermalContainer(_) => None,
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

/// Runtime state for a launch pad module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchPadState {
    /// Whether the pad is available for a launch. False during recovery.
    #[serde(default = "default_true")]
    pub available: bool,
    /// Ticks remaining until the pad is available after a launch.
    #[serde(default)]
    pub recovery_ticks_remaining: u64,
    /// Total launches completed on this pad.
    #[serde(default)]
    pub launches_count: u64,
}

impl Default for LaunchPadState {
    fn default() -> Self {
        Self {
            available: true,
            recovery_ticks_remaining: 0,
            launches_count: 0,
        }
    }
}

fn default_true() -> bool {
    true
}

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

/// Runtime state for a thermal container module (crucible).
/// Holds molten material inventory separate from station inventory.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThermalContainerState {
    /// Material held in the container with thermal properties.
    #[serde(default)]
    pub held_items: Vec<InventoryItem>,
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
    /// Role name → module indices. Rebuilt on module install/uninstall.
    pub roles: BTreeMap<String, Vec<usize>>,
}

impl ModuleTypeIndex {
    /// Returns true if the index has been built at least once.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

/// An explicit connection between two module ports for directed material flow.
///
/// Links are station-level config with deterministic ordering by
/// `(from_module_id, to_module_id)`. No routing or pathfinding — just direct
/// point-to-point connections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThermalLink {
    /// Module providing the output.
    pub from_module_id: ModuleInstanceId,
    /// Port ID on the source module (must be Output direction).
    pub from_port_id: String,
    /// Module receiving the input.
    pub to_module_id: ModuleInstanceId,
    /// Port ID on the destination module (must be Input direction).
    pub to_port_id: String,
}

/// Shared substrate for any entity that hosts modules, inventory, crew, and power.
/// Both `StationState` (orbital) and `GroundFacilityState` (surface) compose this.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FacilityCore {
    pub inventory: Vec<InventoryItem>,
    pub cargo_capacity_m3: f32,
    pub power_available_per_tick: f32,
    pub modules: Vec<ModuleState>,
    /// Per-facility modifiers (from equipment, location).
    #[serde(default)]
    pub modifiers: crate::modifiers::ModifierSet,
    /// Crew roster: how many of each role are available.
    #[serde(default)]
    pub crew: BTreeMap<CrewRole, u32>,
    /// Explicit module-to-module connections for material flow.
    #[serde(default)]
    pub thermal_links: Vec<ThermalLink>,
    /// Computed fresh each tick -- not persisted across ticks.
    #[serde(skip_deserializing, default)]
    pub power: PowerState,
    /// Cached inventory volume. Set to `None` when inventory changes;
    /// recomputed lazily via [`FacilityCore::used_volume_m3`].
    #[serde(skip, default)]
    pub cached_inventory_volume_m3: Option<f32>,
    /// Pre-computed module-type → indices mapping. Rebuilt on install/uninstall.
    #[serde(skip, default)]
    pub module_type_index: ModuleTypeIndex,
    /// Pre-computed module instance ID → index mapping. Rebuilt on install/uninstall.
    #[serde(skip, default)]
    pub module_id_index: HashMap<ModuleInstanceId, usize>,
    /// Cached power generation/consumption values. Avoids re-iterating modules
    /// when nothing power-relevant has changed.
    #[serde(skip, default)]
    pub power_budget_cache: PowerBudgetCache,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationState {
    pub id: StationId,
    pub position: crate::Position,
    /// Shared module-hosting fields.
    #[serde(flatten)]
    pub core: FacilityCore,
    /// Station frame. Determines slot layout and contributes frame bonuses
    /// via the modifier pipeline. `None` = legacy frameless station
    /// (unlimited slots, no frame bonuses).
    #[serde(default)]
    pub frame_id: Option<FrameId>,
    /// Station leaders (reserved for Phase 2 leader system).
    #[serde(default)]
    pub leaders: Vec<LeaderId>,
}

// ---------------------------------------------------------------------------
// Ground facility state
// ---------------------------------------------------------------------------

/// An Earth-based (or planetary surface) operations center.
/// Hosts modules via `FacilityCore` but cannot dock ships. Uses launch
/// mechanics to deliver payloads to orbit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundFacilityState {
    pub id: crate::GroundFacilityId,
    pub name: String,
    pub position: crate::Position,
    /// Shared module-hosting fields (same substrate as `StationState`).
    #[serde(flatten)]
    pub core: FacilityCore,
    /// In-flight launches from this facility.
    #[serde(default)]
    pub launch_transits: Vec<LaunchTransitState>,
}

/// A payload currently in transit from a ground facility launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchTransitState {
    /// Which rocket was used.
    pub rocket_def_id: String,
    /// What is being delivered.
    pub payload: LaunchPayload,
    /// Destination position.
    pub destination: crate::Position,
    /// Tick at which the payload arrives.
    pub arrival_tick: u64,
}

/// The payload of a launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LaunchPayload {
    /// Deliver supplies to an existing orbital station.
    Supplies(Vec<InventoryItem>),
    /// Deploy a new orbital station.
    StationKit,
    /// Deploy a satellite into orbit. The satellite component must exist in
    /// the facility inventory and is consumed on launch.
    Satellite {
        /// Matches `SatelliteDef.id` in content (e.g. `sat_survey`).
        satellite_def_id: String,
    },
}

// ---------------------------------------------------------------------------
// Satellites
// ---------------------------------------------------------------------------

/// A deployed satellite in orbit. Content-driven: `satellite_type` is a string
/// matching a `SatelliteDef.satellite_type` (not an enum).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SatelliteState {
    pub id: SatelliteId,
    /// References `SatelliteDef.id` in content.
    pub def_id: String,
    pub name: String,
    pub position: crate::Position,
    /// Tick at which this satellite was deployed.
    pub deployed_tick: u64,
    /// Wear level 0.0 (pristine) to 1.0 (failed). Accumulates each tick.
    /// f64 (not f32 like module `WearState`) because satellite wear rates are very
    /// low (e.g. 0.00008/tick) and accumulate over 10,000+ tick lifespans where
    /// f32 would lose meaningful precision.
    pub wear: f64,
    pub enabled: bool,
    /// Content-driven type string: "survey", "communication", "navigation", "`science_platform`".
    pub satellite_type: String,
    /// Optional type-specific configuration (e.g. target sensor type for science platforms).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_config: Option<String>,
}

impl FacilityCore {
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

    /// Initialize module efficiency based on crew and wear factors.
    /// Call after loading state to avoid spurious transition events on first tick.
    pub fn init_module_efficiency(&mut self, content: &GameContent) {
        for module in &mut self.modules {
            if let Some(def) = content.module_defs.get(&module.def_id) {
                module.efficiency = compute_module_efficiency(module, def, &content.constants);
                module.prev_crew_satisfied =
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
        idx.roles.clear();

        self.module_id_index.clear();

        for (i, module) in self.modules.iter().enumerate() {
            self.module_id_index.insert(module.id.clone(), i);

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
                for role in &def.roles {
                    idx.roles.entry(role.clone()).or_default().push(i);
                }
            }
        }
    }

    /// Look up a module's index by its instance ID. Returns `None` if the
    /// module is not installed or the index has not been built.
    pub fn module_index_by_id(&self, id: &ModuleInstanceId) -> Option<usize> {
        self.module_id_index.get(id).copied()
    }

    /// Returns true if any installed module has the given role.
    pub fn has_role(&self, role: &str) -> bool {
        self.module_type_index
            .roles
            .get(role)
            .is_some_and(|indices| !indices.is_empty())
    }

    /// Returns module indices that have the given role.
    pub fn modules_with_role(&self, role: &str) -> &[usize] {
        self.module_type_index
            .roles
            .get(role)
            .map_or(&[], |v| v.as_slice())
    }
}

impl StationState {
    // Delegation methods — forward to self.core so external callers don't all
    // need updating in this refactoring ticket.

    pub fn used_volume_m3(&mut self, content: &GameContent) -> f32 {
        self.core.used_volume_m3(content)
    }

    pub fn invalidate_volume_cache(&mut self) {
        self.core.invalidate_volume_cache();
    }

    pub fn invalidate_power_cache(&mut self) {
        self.core.invalidate_power_cache();
    }

    pub fn available_crew(&self, role: &CrewRole) -> u32 {
        self.core.available_crew(role)
    }

    pub fn init_module_efficiency(&mut self, content: &GameContent) {
        self.core.init_module_efficiency(content);
    }

    pub fn rebuild_module_index(&mut self, content: &GameContent) {
        self.core.rebuild_module_index(content);
    }

    pub fn module_index_by_id(&self, id: &ModuleInstanceId) -> Option<usize> {
        self.core.module_index_by_id(id)
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.core.has_role(role)
    }

    pub fn modules_with_role(&self, role: &str) -> &[usize] {
        self.core.modules_with_role(role)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{base_content, base_state, test_station_id};

    #[test]
    fn module_id_index_returns_correct_position() {
        let content = base_content();
        let mut state = base_state(&content);
        let station = state.stations.get_mut(&test_station_id()).unwrap();

        // Push two modules
        station.core.modules.push(ModuleState {
            id: ModuleInstanceId("mod_alpha".to_string()),
            def_id: "nonexistent".to_string(),
            enabled: true,
            kind_state: ModuleKindState::Storage,
            wear: WearState::default(),
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
            thermal: None,
            slot_index: None,
        });
        station.core.modules.push(ModuleState {
            id: ModuleInstanceId("mod_beta".to_string()),
            def_id: "nonexistent".to_string(),
            enabled: true,
            kind_state: ModuleKindState::Storage,
            wear: WearState::default(),
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
            thermal: None,
            slot_index: None,
        });

        station.rebuild_module_index(&content);

        let alpha_idx = station.module_index_by_id(&ModuleInstanceId("mod_alpha".to_string()));
        let beta_idx = station.module_index_by_id(&ModuleInstanceId("mod_beta".to_string()));
        let missing = station.module_index_by_id(&ModuleInstanceId("mod_missing".to_string()));

        assert_eq!(alpha_idx, Some(station.core.modules.len() - 2));
        assert_eq!(beta_idx, Some(station.core.modules.len() - 1));
        assert_eq!(missing, None);
    }

    #[test]
    fn compute_crew_factor_empty_requirement() {
        let assigned = BTreeMap::new();
        let requirement = BTreeMap::new();
        assert!((compute_crew_factor(&assigned, &requirement) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn compute_crew_factor_half_staffed() {
        let assigned = BTreeMap::from([(CrewRole("op".to_string()), 1)]);
        let requirement = BTreeMap::from([(CrewRole("op".to_string()), 2)]);
        assert!((compute_crew_factor(&assigned, &requirement) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn compute_crew_factor_fully_staffed() {
        let assigned = BTreeMap::from([(CrewRole("op".to_string()), 2)]);
        let requirement = BTreeMap::from([(CrewRole("op".to_string()), 2)]);
        assert!((compute_crew_factor(&assigned, &requirement) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn compute_module_efficiency_combines_factors() {
        let content = base_content();
        let mut module = ModuleState {
            id: ModuleInstanceId("test".to_string()),
            def_id: "nonexistent".to_string(),
            enabled: true,
            kind_state: ModuleKindState::Storage,
            wear: WearState::default(),
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
            thermal: None,
            slot_index: None,
        };
        let def = crate::test_fixtures::ModuleDefBuilder::new("test")
            .crew("operator", 2)
            .build();

        // No crew assigned, requires 2 operators → crew_factor = 0.0
        let eff = compute_module_efficiency(&module, &def, &content.constants);
        assert!((eff - 0.0).abs() < f32::EPSILON, "no crew = 0 efficiency");

        // 1/2 crew → crew_factor = 0.5
        module
            .assigned_crew
            .insert(CrewRole("operator".to_string()), 1);
        let eff = compute_module_efficiency(&module, &def, &content.constants);
        assert!(
            (eff - 0.5).abs() < f32::EPSILON,
            "half crew = 0.5 efficiency"
        );

        // Power stalled → 0.0 regardless of crew
        module.power_stalled = true;
        let eff = compute_module_efficiency(&module, &def, &content.constants);
        assert!(
            (eff - 0.0).abs() < f32::EPSILON,
            "power stalled = 0 efficiency"
        );
    }

    // ----------------------------------------------------------------------
    // SF-01: frame_id + slot_index serde backward compat
    // ----------------------------------------------------------------------

    #[test]
    fn station_state_deserializes_without_frame_id_field() {
        // Old save JSON has no `frame_id` — must deserialize with `None`.
        let json = r#"{
            "id": "station_earth_orbit",
            "position": {
                "parent_body": "test_body",
                "radius_au_um": 0,
                "angle_mdeg": 0
            },
            "inventory": [],
            "cargo_capacity_m3": 10000.0,
            "power_available_per_tick": 100.0,
            "modules": [],
            "modifiers": {"modifiers": []},
            "crew": {},
            "thermal_links": [],
            "leaders": []
        }"#;

        let station: StationState = serde_json::from_str(json).expect("legacy save must load");
        assert_eq!(
            station.frame_id, None,
            "missing frame_id should default to None"
        );
    }

    #[test]
    fn module_state_deserializes_without_slot_index_field() {
        // Old save JSON has no `slot_index` — must deserialize with `None`.
        let json = r#"{
            "id": "inst_legacy",
            "def_id": "module_legacy",
            "enabled": true,
            "kind_state": "Storage",
            "wear": {"wear": 0.0}
        }"#;

        let module: ModuleState =
            serde_json::from_str(json).expect("legacy module state must load");
        assert_eq!(
            module.slot_index, None,
            "missing slot_index should default to None"
        );
    }

    #[test]
    fn station_state_serde_roundtrip_with_frame_id() {
        // Build a station, assign a frame, serialize, deserialize, verify.
        let station = StationState {
            id: StationId("s_roundtrip".to_string()),
            position: crate::Position {
                parent_body: BodyId("test_body".to_string()),
                radius_au_um: crate::RadiusAuMicro(0),
                angle_mdeg: crate::AngleMilliDeg(0),
            },
            core: FacilityCore {
                inventory: vec![],
                cargo_capacity_m3: 500.0,
                power_available_per_tick: 0.0,
                modules: vec![],
                modifiers: crate::modifiers::ModifierSet::default(),
                crew: Default::default(),
                thermal_links: Vec::new(),
                power: crate::PowerState::default(),
                cached_inventory_volume_m3: None,
                module_type_index: crate::ModuleTypeIndex::default(),
                module_id_index: std::collections::HashMap::new(),
                power_budget_cache: crate::PowerBudgetCache::default(),
            },
            frame_id: Some(FrameId("frame_outpost".to_string())),
            leaders: Vec::new(),
        };

        let json = serde_json::to_string(&station).expect("serialize");
        let decoded: StationState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.frame_id, Some(FrameId("frame_outpost".to_string())));
    }

    #[test]
    fn module_state_serde_roundtrip_with_slot_index() {
        let module = ModuleState {
            id: ModuleInstanceId("m_roundtrip".to_string()),
            def_id: "m_def".to_string(),
            enabled: true,
            kind_state: ModuleKindState::Storage,
            wear: WearState::default(),
            thermal: None,
            slot_index: Some(3),
            power_stalled: false,
            module_priority: 0,
            assigned_crew: Default::default(),
            efficiency: 1.0,
            prev_crew_satisfied: true,
        };
        let json = serde_json::to_string(&module).expect("serialize");
        let decoded: ModuleState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.slot_index, Some(3));
    }
}
