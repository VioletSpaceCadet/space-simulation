//! Type definitions for `sim_core`.
//!
//! All public types, structs, enums, and ID newtypes used by the simulation.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

pub type ElementId = String;
pub type CompositionVec = HashMap<ElementId, f32>;

// ---------------------------------------------------------------------------
// Well-known element IDs
// ---------------------------------------------------------------------------

pub const ELEMENT_ORE: &str = "ore";
pub const ELEMENT_SLAG: &str = "slag";
pub const ELEMENT_FE: &str = "Fe";
pub const COMPONENT_REPAIR_KIT: &str = "repair_kit";
pub const COMPONENT_THRUSTER: &str = "thruster";

// ---------------------------------------------------------------------------
// Well-known anomaly tag IDs
// ---------------------------------------------------------------------------

pub const TAG_IRON_RICH: &str = "IronRich";
pub const TAG_VOLATILE_RICH: &str = "VolatileRich";

// ---------------------------------------------------------------------------
// Schema version
// ---------------------------------------------------------------------------

/// Current save-file schema version. Bump when state shape changes in a
/// backward-incompatible way (new required fields, removed fields, type changes).
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// ID newtypes
// ---------------------------------------------------------------------------

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub String);

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_id!(ShipId);
string_id!(AsteroidId);
string_id!(StationId);
string_id!(TechId);
string_id!(NodeId);
string_id!(BodyId);
string_id!(SiteId);
string_id!(CommandId);
string_id!(EventId);
string_id!(PrincipalId);
string_id!(LotId);
string_id!(ModuleItemId);
string_id!(ModuleInstanceId);
string_id!(ComponentId);
string_id!(RecipeId);
string_id!(HullId);
string_id!(SlotType);
string_id!(ModuleDefId);

// ---------------------------------------------------------------------------
// Core enums
// ---------------------------------------------------------------------------

/// Data-driven anomaly tag. Values come from content JSON (`asteroid_templates`).
/// Adding a new asteroid type = adding a JSON entry, not a code change.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnomalyTag(pub String);

impl AnomalyTag {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for AnomalyTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataKind {
    SurveyData,
    AssayData,
    ManufacturingData,
    TransitData,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResearchDomain {
    Survey,
    Materials,
    Manufacturing,
    Propulsion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainProgress {
    pub points: HashMap<ResearchDomain, f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventLevel {
    Normal,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Warning,
    Critical,
}

// ---------------------------------------------------------------------------
// Content-driven alert rules
// ---------------------------------------------------------------------------

/// A single alert rule definition loaded from `content/alerts.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRuleDef {
    pub id: String,
    pub severity: AlertSeverity,
    pub message: String,
    pub suggested_action: String,
    pub rule: AlertRuleType,
}

/// Parameterized rule evaluator type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AlertRuleType {
    /// Fires when a metric field on the latest snapshot exceeds a threshold.
    #[serde(rename = "threshold_latest")]
    ThresholdLatest {
        metric: String,
        condition: String,
        threshold: f64,
    },
    /// Fires when a per-element material value exceeds a threshold.
    #[serde(rename = "threshold_latest_element")]
    ThresholdLatestElement {
        element: String,
        condition: String,
        threshold: f64,
        #[serde(default)]
        min_value: Option<f64>,
    },
    /// Fires when a metric field is nonzero for N consecutive samples.
    #[serde(rename = "consecutive")]
    Consecutive { metric: String, min_samples: u32 },
    /// Complex rules kept as named Rust implementations.
    #[serde(rename = "builtin")]
    Builtin { name: String },
}

// ---------------------------------------------------------------------------
// State types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub meta: MetaState,
    /// Unscanned potential asteroid locations. Populated at world-gen; entries
    /// are removed when surveyed and replaced by a real `AsteroidState`.
    pub scan_sites: Vec<ScanSite>,
    pub asteroids: HashMap<AsteroidId, AsteroidState>,
    pub ships: HashMap<ShipId, ShipState>,
    pub stations: HashMap<StationId, StationState>,
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
    /// Cached absolute positions for orbital bodies. Not serialized — recomputed on load.
    #[serde(skip, default)]
    pub body_cache: std::collections::HashMap<BodyId, crate::spatial::BodyCache>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum InventoryItem {
    Ore {
        lot_id: LotId,
        asteroid_id: AsteroidId,
        kg: f32,
        composition: CompositionVec,
    },
    Slag {
        kg: f32,
        composition: CompositionVec,
    },
    Material {
        element: ElementId,
        kg: f32,
        quality: f32,
        /// Per-batch thermal properties. `None` for non-thermal materials.
        #[serde(default)]
        thermal: Option<MaterialThermalProps>,
    },
    Component {
        component_id: ComponentId,
        count: u32,
        quality: f32,
    },
    Module {
        item_id: ModuleItemId,
        module_def_id: String,
    },
}

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
    /// Manufacturing priority. Higher values run first within each behavior class.
    /// Used to control which modules consume shared inventory first. 0 = default.
    #[serde(default)]
    pub manufacturing_priority: u32,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsteroidState {
    pub id: AsteroidId,
    pub position: crate::Position,
    /// Ground truth — never exposed to the UI.
    pub true_composition: CompositionVec,
    pub anomaly_tags: Vec<AnomalyTag>,
    pub mass_kg: f32,
    pub knowledge: AsteroidKnowledge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsteroidKnowledge {
    pub tag_beliefs: Vec<(AnomalyTag, f32)>,
    /// Set after a deep scan. Exact composition — no uncertainty model.
    pub composition: Option<CompositionVec>,
}

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
}

fn default_hull_id() -> HullId {
    HullId("hull_general_purpose".to_string())
}

impl ShipState {
    /// Returns this ship's travel speed, falling back to the global default.
    pub fn ticks_per_au(&self, global_default: u64) -> u64 {
        self.speed_ticks_per_au.unwrap_or(global_default)
    }
}

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
    /// Computed fresh each tick — not persisted across ticks.
    #[serde(skip_deserializing, default)]
    pub power: PowerState,
    /// Cached inventory volume. Set to `None` when inventory changes;
    /// recomputed lazily via [`StationState::used_volume_m3`].
    #[serde(skip, default)]
    pub cached_inventory_volume_m3: Option<f32>,
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
}

/// Research distributes automatically to all eligible techs — no player allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchState {
    pub unlocked: HashSet<TechId>,
    pub data_pool: HashMap<DataKind, f32>,
    pub evidence: HashMap<TechId, DomainProgress>,
    #[serde(default)]
    pub action_counts: HashMap<String, u64>,
}

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
}

impl TaskKind {
    /// Task duration in ticks.
    pub fn duration(&self, constants: &Constants) -> u64 {
        match self {
            Self::Transit { total_ticks, .. } => *total_ticks,
            Self::Survey { .. } => constants.survey_scan_ticks,
            Self::DeepScan { .. } => constants.deep_scan_ticks,
            Self::Mine { duration_ticks, .. } => *duration_ticks,
            Self::Deposit { .. } => constants.deposit_ticks,
            Self::Idle => 0,
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
        }
    }

    /// Target entity ID (if any) for display/events.
    pub fn target(&self) -> Option<String> {
        match self {
            Self::Idle => None,
            Self::Transit { destination, .. } => Some(destination.parent_body.0.clone()),
            Self::Survey { site } => Some(site.0.clone()),
            Self::DeepScan { asteroid } | Self::Mine { asteroid, .. } => Some(asteroid.0.clone()),
            Self::Deposit { station, .. } => Some(station.0.clone()),
        }
    }
}

// ---------------------------------------------------------------------------
// Trade types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TradeItemSpec {
    Material {
        element: String,
        kg: f32,
    },
    Component {
        component_id: ComponentId,
        count: u32,
    },
    Module {
        module_def_id: String,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PricingEntry {
    pub base_price_per_unit: f64,
    pub importable: bool,
    pub exportable: bool,
    /// Item category for UI grouping: `material`, `component`, `module`, `raw_ore`, `byproduct`.
    #[serde(default)]
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingTable {
    pub import_surcharge_per_kg: f64,
    pub export_surcharge_per_kg: f64,
    pub items: HashMap<String, PricingEntry>,
}

impl Default for PricingTable {
    fn default() -> Self {
        Self {
            import_surcharge_per_kg: 0.0,
            export_surcharge_per_kg: 0.0,
            items: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Command types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandEnvelope {
    pub id: CommandId,
    pub issued_by: PrincipalId,
    pub issued_tick: u64,
    pub execute_at_tick: u64,
    pub command: Command,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    AssignShipTask {
        ship_id: ShipId,
        task_kind: TaskKind,
    },
    InstallModule {
        station_id: StationId,
        module_item_id: ModuleItemId,
    },
    UninstallModule {
        station_id: StationId,
        module_id: ModuleInstanceId,
    },
    SetModuleEnabled {
        station_id: StationId,
        module_id: ModuleInstanceId,
        enabled: bool,
    },
    SetModuleThreshold {
        station_id: StationId,
        module_id: ModuleInstanceId,
        threshold_kg: f32,
    },
    AssignLabTech {
        station_id: StationId,
        module_id: ModuleInstanceId,
        tech_id: Option<TechId>,
    },
    SetAssemblerCap {
        station_id: StationId,
        module_id: ModuleInstanceId,
        component_id: ComponentId,
        max_stock: u32,
    },
    Import {
        station_id: StationId,
        item_spec: TradeItemSpec,
    },
    Export {
        station_id: StationId,
        item_spec: TradeItemSpec,
    },
    JettisonSlag {
        station_id: StationId,
    },
    SelectRecipe {
        station_id: StationId,
        module_id: ModuleInstanceId,
        recipe_id: RecipeId,
    },
    SetManufacturingPriority {
        station_id: StationId,
        module_id: ModuleInstanceId,
        priority: u32,
    },
    FitShipModule {
        ship_id: ShipId,
        slot_index: usize,
        module_def_id: ModuleDefId,
        station_id: StationId,
    },
    UnfitShipModule {
        ship_id: ShipId,
        slot_index: usize,
        station_id: StationId,
    },
}

// ---------------------------------------------------------------------------
// Event types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub id: EventId,
    pub tick: u64,
    pub event: Event,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BehaviorType {
    Processor,
    Storage,
    Maintenance,
    Assembler,
    Lab,
    SensorArray,
    SolarArray,
    Battery,
    Radiator,
    Equipment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    TaskStarted {
        ship_id: ShipId,
        task_kind: String,
        target: Option<String>,
    },
    TaskCompleted {
        ship_id: ShipId,
        task_kind: String,
        target: Option<String>,
    },
    AsteroidDiscovered {
        asteroid_id: AsteroidId,
        position: crate::Position,
    },
    ScanResult {
        asteroid_id: AsteroidId,
        tags: Vec<(AnomalyTag, f32)>,
    },
    CompositionMapped {
        asteroid_id: AsteroidId,
        composition: CompositionVec,
    },
    DataGenerated {
        kind: DataKind,
        amount: f32,
    },
    PowerConsumed {
        station_id: StationId,
        amount: f32,
    },
    TechUnlocked {
        tech_id: TechId,
    },
    ShipArrived {
        ship_id: ShipId,
        position: crate::Position,
    },
    OreMined {
        ship_id: ShipId,
        asteroid_id: AsteroidId,
        ore_lot: InventoryItem,
        asteroid_remaining_kg: f32,
    },
    OreDeposited {
        ship_id: ShipId,
        station_id: StationId,
        items: Vec<InventoryItem>,
    },
    ModuleInstalled {
        station_id: StationId,
        module_id: ModuleInstanceId,
        module_item_id: ModuleItemId,
        module_def_id: String,
        behavior_type: BehaviorType,
    },
    ModuleUninstalled {
        station_id: StationId,
        module_id: ModuleInstanceId,
        module_item_id: ModuleItemId,
    },
    ModuleToggled {
        station_id: StationId,
        module_id: ModuleInstanceId,
        enabled: bool,
    },
    ModuleThresholdSet {
        station_id: StationId,
        module_id: ModuleInstanceId,
        threshold_kg: f32,
    },
    RefineryRan {
        station_id: StationId,
        module_id: ModuleInstanceId,
        ore_consumed_kg: f32,
        material_produced_kg: f32,
        material_quality: f32,
        slag_produced_kg: f32,
        material_element: ElementId,
    },
    ScanSiteSpawned {
        site_id: SiteId,
        position: crate::Position,
        template_id: String,
    },
    /// Only emitted at `EventLevel::Debug`.
    ResearchRoll {
        tech_id: TechId,
        evidence: f32,
        p: f32,
        rolled: f32,
    },
    AlertRaised {
        alert_id: String,
        severity: AlertSeverity,
        message: String,
        suggested_action: String,
    },
    AlertCleared {
        alert_id: String,
    },
    WearAccumulated {
        station_id: StationId,
        module_id: ModuleInstanceId,
        wear_before: f32,
        wear_after: f32,
    },
    ModuleAutoDisabled {
        station_id: StationId,
        module_id: ModuleInstanceId,
    },
    AssemblerRan {
        station_id: StationId,
        module_id: ModuleInstanceId,
        recipe_id: RecipeId,
        material_consumed_kg: f32,
        material_element: ElementId,
        component_produced_id: ComponentId,
        component_produced_count: u32,
        component_quality: f32,
    },
    AssemblerCapped {
        station_id: StationId,
        module_id: ModuleInstanceId,
    },
    AssemblerUncapped {
        station_id: StationId,
        module_id: ModuleInstanceId,
    },
    LabRan {
        station_id: StationId,
        module_id: ModuleInstanceId,
        tech_id: TechId,
        data_consumed: f32,
        points_produced: f32,
        domain: ResearchDomain,
    },
    LabStarved {
        station_id: StationId,
        module_id: ModuleInstanceId,
    },
    LabResumed {
        station_id: StationId,
        module_id: ModuleInstanceId,
    },
    MaintenanceRan {
        station_id: StationId,
        target_module_id: ModuleInstanceId,
        wear_before: f32,
        wear_after: f32,
        repair_kits_remaining: u32,
    },
    ModuleStalled {
        station_id: StationId,
        module_id: ModuleInstanceId,
        shortfall_m3: f32,
    },
    ModuleResumed {
        station_id: StationId,
        module_id: ModuleInstanceId,
    },
    DepositBlocked {
        ship_id: ShipId,
        station_id: StationId,
        shortfall_m3: f32,
    },
    DepositUnblocked {
        ship_id: ShipId,
        station_id: StationId,
    },
    ItemImported {
        station_id: StationId,
        item_spec: TradeItemSpec,
        cost: f64,
        balance_after: f64,
    },
    ItemExported {
        station_id: StationId,
        item_spec: TradeItemSpec,
        revenue: f64,
        balance_after: f64,
    },
    ShipConstructed {
        station_id: StationId,
        ship_id: ShipId,
        position: crate::Position,
        cargo_capacity_m3: f64,
        hull_id: HullId,
    },
    ShipModuleFitted {
        ship_id: ShipId,
        slot_index: usize,
        module_def_id: ModuleDefId,
        station_id: StationId,
    },
    ShipModuleUnfitted {
        ship_id: ShipId,
        slot_index: usize,
        module_def_id: ModuleDefId,
        station_id: StationId,
    },
    InsufficientFunds {
        station_id: StationId,
        action: String,
        required: f64,
        available: f64,
    },
    ModuleAwaitingTech {
        station_id: StationId,
        module_id: ModuleInstanceId,
        tech_id: TechId,
    },
    SlagJettisoned {
        station_id: StationId,
        kg: f32,
    },
    PowerStateUpdated {
        station_id: StationId,
        power: PowerState,
    },
    ProcessorTooCold {
        station_id: StationId,
        module_id: ModuleInstanceId,
        current_temp_mk: u32,
        required_temp_mk: u32,
    },
    OverheatWarning {
        station_id: StationId,
        module_id: ModuleInstanceId,
        temp_mk: u32,
        max_temp_mk: u32,
    },
    OverheatCritical {
        station_id: StationId,
        module_id: ModuleInstanceId,
        temp_mk: u32,
        max_temp_mk: u32,
    },
    OverheatCleared {
        station_id: StationId,
        module_id: ModuleInstanceId,
        temp_mk: u32,
    },
    OverheatDamage {
        station_id: StationId,
        module_id: ModuleInstanceId,
        temp_mk: u32,
        max_temp_mk: u32,
        wear_before: f32,
    },
    BoiloffLoss {
        station_id: StationId,
        element: ElementId,
        kg_lost: f32,
    },
    RecipeSelectionReset {
        station_id: StationId,
        module_id: ModuleInstanceId,
        old_recipe: RecipeId,
        new_recipe: RecipeId,
    },
    SimEventFired {
        event_def_id: crate::sim_events::EventDefId,
        target: crate::sim_events::ResolvedTarget,
        effects_applied: Vec<crate::sim_events::AppliedEffect>,
    },
    SimEventExpired {
        event_def_id: crate::sim_events::EventDefId,
    },
}

// ---------------------------------------------------------------------------
// Content types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameContent {
    pub content_version: String,
    pub techs: Vec<TechDef>,
    pub solar_system: SolarSystemDef,
    pub asteroid_templates: Vec<AsteroidTemplateDef>,
    pub elements: Vec<ElementDef>,
    pub module_defs: HashMap<String, ModuleDef>,
    pub component_defs: Vec<ComponentDef>,
    /// Recipe catalog loaded from `content/recipes.json`.
    #[serde(default)]
    pub recipes: BTreeMap<RecipeId, RecipeDef>,
    pub pricing: PricingTable,
    pub constants: Constants,
    /// Alert rules loaded from `content/alerts.json`. Empty if file is missing.
    #[serde(default)]
    pub alert_rules: Vec<AlertRuleDef>,
    /// Sim event definitions loaded from `content/events.json`. Empty if file is missing.
    #[serde(default)]
    pub events: Vec<crate::sim_events::SimEventDef>,
    /// Hull definitions loaded from `content/hull_defs.json`. Empty if file is missing.
    #[serde(default)]
    pub hulls: BTreeMap<HullId, HullDef>,
    /// Pre-computed element id → density (kg/m³) lookup. Populated by `init_caches()`.
    #[serde(skip)]
    pub density_map: HashMap<String, f32>,
}

impl GameContent {
    /// Populate derived caches from content data. Must be called after deserialization.
    pub fn init_caches(&mut self) {
        self.density_map = self
            .elements
            .iter()
            .map(|e| (e.id.clone(), e.density_kg_per_m3))
            .collect();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentDef {
    pub id: String,
    pub name: String,
    pub mass_kg: f32,
    pub volume_m3: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechDef {
    pub id: TechId,
    pub name: String,
    pub prereqs: Vec<TechId>,
    #[serde(default)]
    pub domain_requirements: HashMap<ResearchDomain, f32>,
    pub accepted_data: Vec<DataKind>,
    pub difficulty: f32,
    pub effects: Vec<TechEffect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TechEffect {
    EnableDeepScan,
    DeepScanCompositionNoise {
        sigma: f32,
    },
    EnableShipConstruction,
    /// Content-driven numeric modifier applied to a game stat when this tech unlocks.
    StatModifier {
        stat: crate::modifiers::StatId,
        op: crate::modifiers::ModifierOp,
        value: f64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolarSystemDef {
    /// Hierarchical body tree — the source of truth for spatial layout.
    pub bodies: Vec<OrbitalBodyDef>,
    /// Legacy node list — kept for backward compat until graph pathfinding is replaced.
    #[serde(default)]
    pub nodes: Vec<NodeDef>,
    /// Legacy edge list — kept for backward compat until graph pathfinding is replaced.
    #[serde(default)]
    pub edges: Vec<(NodeId, NodeId)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDef {
    pub id: NodeId,
    pub name: String,
    #[serde(default = "default_solar_intensity")]
    pub solar_intensity: f32,
}

fn default_solar_intensity() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BodyType {
    Star,
    Planet,
    Moon,
    Belt,
    Zone,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoneDef {
    pub radius_min_au_um: u64,
    pub radius_max_au_um: u64,
    pub angle_start_mdeg: u32,
    pub angle_span_mdeg: u32,
    pub resource_class: crate::spatial::ResourceClass,
    pub scan_site_weight: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrbitalBodyDef {
    pub id: BodyId,
    pub name: String,
    pub parent: Option<BodyId>,
    pub body_type: BodyType,
    pub radius_au_um: u64,
    pub angle_mdeg: u32,
    #[serde(default = "default_solar_intensity")]
    pub solar_intensity: f32,
    pub zone: Option<ZoneDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsteroidTemplateDef {
    pub id: String,
    pub anomaly_tags: Vec<AnomalyTag>,
    pub composition_ranges: HashMap<ElementId, (f32, f32)>,
    /// Preferred zone resource class for weighted template selection.
    /// match=3x, none=2x, mismatch=1x weight.
    #[serde(default)]
    pub preferred_class: Option<crate::spatial::ResourceClass>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementDef {
    pub id: ElementId,
    pub density_kg_per_m3: f32,
    pub display_name: String,
    #[serde(default)]
    pub refined_name: Option<String>,
    /// Category for UI grouping: `material`, `byproduct`, `raw_ore`. Defaults to `material`.
    #[serde(default = "default_element_category")]
    pub category: String,
    /// Melting point in milli-Kelvin. `None` for non-thermal elements (ore, slag).
    #[serde(default)]
    pub melting_point_mk: Option<u32>,
    /// Latent heat of fusion in J/kg. `None` for non-thermal elements.
    #[serde(default)]
    pub latent_heat_j_per_kg: Option<u32>,
    /// Specific heat capacity in J/(kg*K). `None` for non-thermal elements.
    #[serde(default)]
    pub specific_heat_j_per_kg_k: Option<u32>,
    /// Fractional boiloff loss per day at ambient (293K). `None` = no boiloff.
    #[serde(default)]
    pub boiloff_rate_per_day_at_293k: Option<f64>,
    /// Boiling point in milli-Kelvin, for temperature-dependent boiloff scaling.
    #[serde(default)]
    pub boiling_point_mk: Option<u32>,
}

fn default_element_category() -> String {
    "material".to_string()
}

/// Derive per-tick boiloff rate from per-day rate using compounding.
/// Tick-size-independent: changing `minutes_per_tick` doesn't break rates.
pub fn boiloff_rate_per_tick(rate_per_day: f64, minutes_per_tick: u32) -> f64 {
    let dt_days = f64::from(minutes_per_tick) * 60.0 / 86400.0;
    1.0 - (1.0 - rate_per_day).powf(dt_days)
}

/// Content-driven thermal properties for a module.
///
/// Defines heat capacity, passive cooling rate, maximum operating temperature,
/// and optional thermal group assignment for shared radiator cooling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThermalDef {
    /// Heat capacity in J/K. Higher values mean slower temperature changes.
    pub heat_capacity_j_per_k: f32,
    /// Passive cooling coefficient (W/K). Scales heat loss via Newton's law of
    /// cooling: `Q_loss = coeff * dt_s * (T - T_sink) / 1000`.
    pub passive_cooling_coefficient: f32,
    /// Maximum safe temperature in milli-Kelvin before overheat consequences.
    pub max_temp_mk: u32,
    /// Minimum operating temperature in milli-Kelvin. `None` means no lower bound.
    #[serde(default)]
    pub operating_min_mk: Option<u32>,
    /// Maximum operating temperature in milli-Kelvin. `None` means no upper bound.
    #[serde(default)]
    pub operating_max_mk: Option<u32>,
    /// Thermal group for shared radiator cooling. Modules in the same group share radiators.
    #[serde(default)]
    pub thermal_group: Option<ThermalGroupId>,
    /// Idle heat generation in Watts. Applied every tick when the module is enabled,
    /// regardless of whether a recipe ran. Allows thermal modules to preheat from
    /// ambient temperature without needing recipe inputs.
    #[serde(default)]
    pub idle_heat_generation_w: Option<f32>,
}

// ---------------------------------------------------------------------------
// Hull + slot types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HullDef {
    pub id: HullId,
    pub name: String,
    pub mass_kg: f32,
    pub cargo_capacity_m3: f32,
    pub base_speed_ticks_per_au: u64,
    pub base_propellant_capacity_kg: f32,
    pub slots: Vec<SlotDef>,
    #[serde(default)]
    pub bonuses: Vec<crate::modifiers::Modifier>,
    #[serde(default)]
    pub required_tech: Option<TechId>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotDef {
    pub slot_type: SlotType,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FittedModule {
    pub slot_index: usize,
    pub module_def_id: ModuleDefId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDef {
    pub id: String,
    pub name: String,
    pub mass_kg: f32,
    pub volume_m3: f32,
    pub power_consumption_per_run: f32,
    #[serde(default)]
    pub wear_per_run: f32,
    pub behavior: ModuleBehaviorDef,
    /// Thermal properties. `None` for modules with no thermal behavior.
    #[serde(default)]
    pub thermal: Option<ThermalDef>,
    /// Slot types this module can be fitted into on a ship hull. Empty = station-only.
    #[serde(default)]
    pub compatible_slots: Vec<SlotType>,
    /// Modifiers applied to a ship when this module is fitted.
    #[serde(default)]
    pub ship_modifiers: Vec<crate::modifiers::Modifier>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModuleBehaviorDef {
    Processor(ProcessorDef),
    Storage {
        capacity_m3: f32,
    },
    Maintenance(MaintenanceDef),
    Assembler(AssemblerDef),
    Lab(LabDef),
    SensorArray(SensorArrayDef),
    SolarArray(SolarArrayDef),
    Battery(BatteryDef),
    Radiator(RadiatorDef),
    /// Passive stat provider for ship fitting. No tick behavior.
    Equipment,
}

impl ModuleBehaviorDef {
    /// Returns the tick interval for ticking module types, or `None` for passive modules.
    pub fn interval_ticks(&self) -> Option<u64> {
        match self {
            Self::Processor(p) => Some(p.processing_interval_ticks),
            Self::Assembler(a) => Some(a.assembly_interval_ticks),
            Self::SensorArray(s) => Some(s.scan_interval_ticks),
            Self::Lab(l) => Some(l.research_interval_ticks),
            Self::Maintenance(m) => Some(m.repair_interval_ticks),
            Self::Storage { .. }
            | Self::SolarArray(_)
            | Self::Battery(_)
            | Self::Radiator(_)
            | Self::Equipment => None,
        }
    }

    /// Returns the power-stall priority for ticking modules. Lower = stalled first.
    /// Passive modules (solar, storage, battery, radiator) return `None`.
    pub fn power_priority(&self) -> Option<u8> {
        match self {
            Self::SensorArray(_) => Some(0),
            Self::Lab(_) => Some(1),
            Self::Assembler(_) => Some(2),
            Self::Processor(_) => Some(3),
            Self::Maintenance(_) => Some(4),
            Self::Storage { .. }
            | Self::SolarArray(_)
            | Self::Battery(_)
            | Self::Radiator(_)
            | Self::Equipment => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblerDef {
    pub assembly_interval_minutes: u64,
    #[serde(skip_deserializing, default)]
    pub assembly_interval_ticks: u64,
    pub recipes: Vec<RecipeId>,
    #[serde(default)]
    pub max_stock: HashMap<ComponentId, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceDef {
    pub repair_interval_minutes: u64,
    #[serde(skip_deserializing, default)]
    pub repair_interval_ticks: u64,
    pub wear_reduction_per_run: f32,
    pub repair_kit_cost: u32,
    /// Minimum wear level before the bay will consume a kit. Defaults to 0.0 (no threshold).
    #[serde(default)]
    pub repair_threshold: f32,
    /// Component ID consumed for repairs. Defaults to `repair_kit`.
    #[serde(default = "default_maintenance_component_id")]
    pub maintenance_component_id: String,
}

fn default_maintenance_component_id() -> String {
    crate::COMPONENT_REPAIR_KIT.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabDef {
    pub domain: ResearchDomain,
    pub data_consumption_per_run: f32,
    pub research_points_per_run: f32,
    pub accepted_data: Vec<DataKind>,
    pub research_interval_minutes: u64,
    #[serde(skip_deserializing, default)]
    pub research_interval_ticks: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorArrayDef {
    pub data_kind: DataKind,
    pub action_key: String,
    pub scan_interval_minutes: u64,
    #[serde(skip_deserializing, default)]
    pub scan_interval_ticks: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolarArrayDef {
    pub base_output_kw: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryDef {
    /// Maximum energy storage capacity in kWh.
    pub capacity_kwh: f32,
    /// Maximum charge rate in kW (how fast surplus can be stored).
    pub charge_rate_kw: f32,
    /// Maximum discharge rate in kW (how fast energy can be released).
    pub discharge_rate_kw: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadiatorDef {
    /// Maximum cooling capacity in Watts.
    pub cooling_capacity_w: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessorDef {
    pub processing_interval_minutes: u64,
    #[serde(skip_deserializing, default)]
    pub processing_interval_ticks: u64,
    pub recipes: Vec<RecipeId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeThermalReq {
    /// Below this temperature the processor stalls (`TooCold`).
    pub min_temp_mk: u32,
    /// Between `min_temp_mk` and `optimal_min_mk`: efficiency ramps 80%→100%.
    pub optimal_min_mk: u32,
    /// Between `optimal_min_mk` and `optimal_max_mk`: 100% efficiency, 100% quality.
    pub optimal_max_mk: u32,
    /// Between `optimal_max_mk` and `max_temp_mk`: quality degrades 100%→60%.
    pub max_temp_mk: u32,
    /// Heat generated (positive = exothermic) or absorbed (negative = endothermic) per run, in Joules.
    pub heat_per_run_j: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeDef {
    pub id: RecipeId,
    pub inputs: Vec<RecipeInput>,
    pub outputs: Vec<OutputSpec>,
    pub efficiency: f32,
    #[serde(default)]
    pub thermal_req: Option<RecipeThermalReq>,
    #[serde(default)]
    pub required_tech: Option<TechId>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeInput {
    pub filter: InputFilter,
    pub amount: InputAmount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputFilter {
    ItemKind(ItemKind),
    Element(ElementId),
    ElementWithMinQuality {
        element: ElementId,
        min_quality: f32,
    },
    Component(ComponentId),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ItemKind {
    Ore,
    Slag,
    Material,
    Component,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputAmount {
    Kg(f32),
    Count(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputSpec {
    Material {
        element: ElementId,
        yield_formula: YieldFormula,
        quality_formula: QualityFormula,
    },
    Slag {
        yield_formula: YieldFormula,
    },
    Component {
        component_id: ComponentId,
        quality_formula: QualityFormula,
    },
    Ship {
        hull_id: HullId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum YieldFormula {
    ElementFraction { element: ElementId },
    FixedFraction(f32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QualityFormula {
    ElementFractionTimesMultiplier { element: ElementId, multiplier: f32 },
    Fixed(f32),
}

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
    /// Minimum `IronRich` tag confidence for autopilot to queue a deep scan.
    pub autopilot_iron_rich_confidence_threshold: f32,
    /// Minimum `VolatileRich` tag confidence for autopilot to queue a deep scan.
    #[serde(default = "default_autopilot_volatile_confidence_threshold")]
    pub autopilot_volatile_confidence_threshold: f32,
    /// H2O inventory (kg) below which autopilot prioritizes volatile-rich mining.
    #[serde(default = "default_autopilot_volatile_threshold_kg")]
    pub autopilot_volatile_threshold_kg: f32,
    /// Default refinery processing threshold (kg) set by autopilot on newly installed modules.
    pub autopilot_refinery_threshold_kg: f32,
    // Research system
    pub research_roll_interval_minutes: u64,
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
    /// Minimum repair kits to keep for maintenance before exporting surplus.
    #[serde(default = "default_autopilot_repair_kit_reserve")]
    pub autopilot_repair_kit_reserve: u32,
    /// Fe (kg) reserved for shipyard recipe + assembler buffer. Surplus above this may be exported.
    #[serde(default = "default_autopilot_fe_reserve_kg")]
    pub autopilot_fe_reserve_kg: f32,
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
    /// Ticks to cross 1 AU. Calibrate so Earth→Inner Belt ≈ 2,880 ticks.
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
    #[serde(skip_deserializing, default)]
    pub research_roll_interval_ticks: u64,
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
        self.research_roll_interval_ticks =
            self.game_minutes_to_ticks(self.research_roll_interval_minutes);
        self.mining_rate_kg_per_tick =
            self.rate_per_minute_to_per_tick(self.mining_rate_kg_per_minute);
        self.station_power_available_per_tick =
            self.rate_per_minute_to_per_tick(self.station_power_available_per_minute);
    }
}

// ---------------------------------------------------------------------------
// Module tick derivation
// ---------------------------------------------------------------------------

/// Compute derived tick-based interval fields on all module behavior defs.
/// Must be called once after deserialization / after overrides.
///
/// Reuses `Constants::game_minutes_to_ticks` for the conversion.
#[allow(clippy::implicit_hasher)]
pub fn derive_module_tick_values(
    module_defs: &mut HashMap<String, ModuleDef>,
    constants: &Constants,
) {
    for def in module_defs.values_mut() {
        match &mut def.behavior {
            ModuleBehaviorDef::Processor(p) => {
                p.processing_interval_ticks =
                    constants.game_minutes_to_ticks(p.processing_interval_minutes);
            }
            ModuleBehaviorDef::Assembler(a) => {
                a.assembly_interval_ticks =
                    constants.game_minutes_to_ticks(a.assembly_interval_minutes);
            }
            ModuleBehaviorDef::Maintenance(m) => {
                m.repair_interval_ticks =
                    constants.game_minutes_to_ticks(m.repair_interval_minutes);
            }
            ModuleBehaviorDef::Lab(l) => {
                l.research_interval_ticks =
                    constants.game_minutes_to_ticks(l.research_interval_minutes);
            }
            ModuleBehaviorDef::SensorArray(s) => {
                s.scan_interval_ticks = constants.game_minutes_to_ticks(s.scan_interval_minutes);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Wear system
// ---------------------------------------------------------------------------

/// Standalone wear state, embedded wherever wear applies.
/// Generic — used by station modules now, ships later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WearState {
    pub wear: f32,
}

impl Default for WearState {
    fn default() -> Self {
        Self { wear: 0.0 }
    }
}

fn default_slag_jettison_pct() -> f32 {
    0.75
}
fn default_autopilot_volatile_confidence_threshold() -> f32 {
    0.7
}
fn default_autopilot_volatile_threshold_kg() -> f32 {
    500.0
}

fn default_autopilot_repair_kit_reserve() -> u32 {
    10
}

fn default_autopilot_fe_reserve_kg() -> f32 {
    12_000.0
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

/// 20 °C in milli-Kelvin — shared default for ambient/sink temperature.
pub const DEFAULT_AMBIENT_TEMP_MK: u32 = 293_000;

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
    2_133 // calibrated so Earth→Inner Belt ≈ 2,880 ticks
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
fn default_events_enabled() -> bool {
    true
}
fn default_event_global_cooldown_ticks() -> u64 {
    200
}
fn default_event_history_capacity() -> usize {
    100
}

// ---------------------------------------------------------------------------
// Thermal system
// ---------------------------------------------------------------------------

/// String alias for grouping modules into thermal groups.
/// Modules in the same group share radiator cooling.
pub type ThermalGroupId = String;

/// Overheat zone classification for a thermal module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OverheatZone {
    /// Below `max_temp_mk` — normal operation.
    #[default]
    Nominal,
    /// Above `max_temp_mk` + warning offset — accelerated wear (2x default).
    Warning,
    /// Above `max_temp_mk` + critical offset — auto-stall + accelerated wear (4x default).
    Critical,
    /// Above `max_temp_mk` + damage offset — wear jumps to `wear_band_critical_threshold`, auto-disable.
    Damage,
}

/// Per-module thermal state, tracked in milli-Kelvin for deterministic integer arithmetic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThermalState {
    /// Temperature in milli-Kelvin (e.g. `293_000` = 20 C ambient).
    pub temp_mk: u32,
    /// Which thermal group this module belongs to (shared with `ThermalDef`).
    pub thermal_group: Option<ThermalGroupId>,
    /// Current overheat zone — used for transition detection and wear multiplier.
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

/// Phase of a material batch (solid or liquid).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Solid,
    Liquid,
}

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
        c.constants.research_roll_interval_minutes = 60;
        c.constants.mining_rate_kg_per_minute = 15.0;
        c.constants.station_power_available_per_minute = 100.0;
        c.constants.derive_tick_values();

        assert_eq!(c.constants.survey_scan_ticks, 2);
        assert_eq!(c.constants.deep_scan_ticks, 8);
        assert_eq!(c.constants.deposit_ticks, 2);
        assert_eq!(c.constants.research_roll_interval_ticks, 1);
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
