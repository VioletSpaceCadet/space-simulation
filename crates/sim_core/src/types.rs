//! Type definitions for `sim_core`.
//!
//! All public types, structs, enums, and ID newtypes used by the simulation.

use std::collections::{HashMap, HashSet};

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

// ---------------------------------------------------------------------------
// ID newtypes
// ---------------------------------------------------------------------------

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
string_id!(SiteId);
string_id!(CommandId);
string_id!(EventId);
string_id!(PrincipalId);
string_id!(LotId);
string_id!(ModuleItemId);
string_id!(ModuleInstanceId);
string_id!(ComponentId);

// ---------------------------------------------------------------------------
// Core enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnomalyTag {
    IronRich,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataKind {
    ScanData,
    MiningData,
    EngineeringData,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResearchDomain {
    Materials,
    Exploration,
    Engineering,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Warning,
    Critical,
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
    pub counters: Counters,
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
    pub node: NodeId,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessorState {
    pub threshold_kg: f32,
    pub ticks_since_last_run: u64,
    #[serde(default)]
    pub stalled: bool,
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
    pub location_node: NodeId,
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
    pub location_node: NodeId,
    pub owner: PrincipalId,
    pub inventory: Vec<InventoryItem>,
    pub cargo_capacity_m3: f32,
    pub task: Option<TaskState>,
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
    pub location_node: NodeId,
    pub inventory: Vec<InventoryItem>,
    pub cargo_capacity_m3: f32,
    pub power_available_per_tick: f32,
    pub modules: Vec<ModuleState>,
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
        destination: NodeId,
        /// Pre-computed total travel ticks (`hop_count` × `travel_ticks_per_hop`).
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingEntry {
    pub base_price_per_unit: f64,
    pub importable: bool,
    pub exportable: bool,
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
        location_node: NodeId,
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
        node: NodeId,
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
        node: NodeId,
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
        recipe_id: String,
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
        location_node: NodeId,
        cargo_capacity_m3: f64,
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
    pub pricing: PricingTable,
    pub constants: Constants,
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
    DeepScanCompositionNoise { sigma: f32 },
    EnableShipConstruction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolarSystemDef {
    pub nodes: Vec<NodeDef>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsteroidTemplateDef {
    pub id: String,
    pub anomaly_tags: Vec<AnomalyTag>,
    pub composition_ranges: HashMap<ElementId, (f32, f32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementDef {
    pub id: ElementId,
    pub density_kg_per_m3: f32,
    pub display_name: String,
    #[serde(default)]
    pub refined_name: Option<String>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModuleBehaviorDef {
    Processor(ProcessorDef),
    Storage { capacity_m3: f32 },
    Maintenance(MaintenanceDef),
    Assembler(AssemblerDef),
    Lab(LabDef),
    SensorArray(SensorArrayDef),
    SolarArray(SolarArrayDef),
    Battery(BatteryDef),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblerDef {
    pub assembly_interval_minutes: u64,
    #[serde(skip_deserializing, default)]
    pub assembly_interval_ticks: u64,
    pub recipes: Vec<RecipeDef>,
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
pub struct ProcessorDef {
    pub processing_interval_minutes: u64,
    #[serde(skip_deserializing, default)]
    pub processing_interval_ticks: u64,
    pub recipes: Vec<RecipeDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeDef {
    pub id: String,
    pub inputs: Vec<RecipeInput>,
    pub outputs: Vec<OutputSpec>,
    pub efficiency: f32,
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
        cargo_capacity_m3: f32,
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
    /// Game-time minutes to travel one hop on the solar system graph.
    pub travel_minutes_per_hop: u64,
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

    // -- Derived tick fields (computed at load time, not in JSON) --
    #[serde(skip_deserializing, default)]
    pub survey_scan_ticks: u64,
    #[serde(skip_deserializing, default)]
    pub deep_scan_ticks: u64,
    #[serde(skip_deserializing, default)]
    pub travel_ticks_per_hop: u64,
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

    /// Compute derived tick-based fields from game-time minutes fields.
    /// Must be called once after deserialization (in `load_content` / after overrides).
    pub fn derive_tick_values(&mut self) {
        self.survey_scan_ticks = self.game_minutes_to_ticks(self.survey_scan_minutes);
        self.deep_scan_ticks = self.game_minutes_to_ticks(self.deep_scan_minutes);
        self.travel_ticks_per_hop = self.game_minutes_to_ticks(self.travel_minutes_per_hop);
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

// ---------------------------------------------------------------------------
// Thermal system
// ---------------------------------------------------------------------------

/// String alias for grouping modules into thermal groups.
/// Modules in the same group share radiator cooling.
pub type ThermalGroupId = String;

/// Per-module thermal state, tracked in milli-Kelvin for deterministic integer arithmetic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThermalState {
    /// Temperature in milli-Kelvin (e.g. `293_000` = 20 C ambient).
    pub temp_mk: u32,
    /// Which thermal group this module belongs to (shared with `ThermalDef`).
    pub thermal_group: Option<ThermalGroupId>,
}

impl Default for ThermalState {
    fn default() -> Self {
        Self {
            temp_mk: 293_000, // 20°C ambient
            thermal_group: None,
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
            temp_mk: 293_000, // 20°C ambient
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
        c.constants.travel_minutes_per_hop = 2880;
        c.constants.deposit_minutes = 120;
        c.constants.research_roll_interval_minutes = 60;
        c.constants.mining_rate_kg_per_minute = 15.0;
        c.constants.station_power_available_per_minute = 100.0;
        c.constants.derive_tick_values();

        assert_eq!(c.constants.survey_scan_ticks, 2);
        assert_eq!(c.constants.deep_scan_ticks, 8);
        assert_eq!(c.constants.travel_ticks_per_hop, 48);
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
        assert_eq!(c.constants.travel_ticks_per_hop, 1);
        assert_eq!(c.constants.deposit_ticks, 1);
    }
}
