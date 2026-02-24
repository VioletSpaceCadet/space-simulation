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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModuleKindState {
    Processor(ProcessorState),
    Storage,
    Maintenance(MaintenanceState),
    Assembler(AssemblerState),
    Lab(LabState),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationState {
    pub id: StationId,
    pub location_node: NodeId,
    pub inventory: Vec<InventoryItem>,
    pub cargo_capacity_m3: f32,
    pub power_available_per_tick: f32,
    pub modules: Vec<ModuleState>,
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
    pub module_defs: Vec<ModuleDef>,
    pub component_defs: Vec<ComponentDef>,
    pub pricing: PricingTable,
    pub constants: Constants,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblerDef {
    pub assembly_interval_ticks: u64,
    pub recipes: Vec<RecipeDef>,
    #[serde(default)]
    pub max_stock: HashMap<ComponentId, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceDef {
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
    pub research_interval_ticks: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessorDef {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constants {
    pub survey_scan_ticks: u64,
    pub deep_scan_ticks: u64,
    /// Ticks to travel one hop on the solar system graph.
    pub travel_ticks_per_hop: u64,
    pub survey_tag_detection_probability: f32,
    pub asteroid_count_per_template: u32,
    pub asteroid_mass_min_kg: f32,
    pub asteroid_mass_max_kg: f32,
    pub ship_cargo_capacity_m3: f32,
    pub station_cargo_capacity_m3: f32,
    /// kg of raw ore extracted per tick of mining
    pub mining_rate_kg_per_tick: f32,
    pub deposit_ticks: u64,
    pub station_power_available_per_tick: f32,
    /// Minimum IronRich tag confidence for autopilot to queue a deep scan.
    pub autopilot_iron_rich_confidence_threshold: f32,
    /// Default refinery processing threshold (kg) set by autopilot on newly installed modules.
    pub autopilot_refinery_threshold_kg: f32,
    // Research system
    pub research_roll_interval_ticks: u64,
    pub data_generation_peak: f32,
    pub data_generation_floor: f32,
    pub data_generation_decay_rate: f32,
    // Wear system
    pub wear_band_degraded_threshold: f32,
    pub wear_band_critical_threshold: f32,
    pub wear_band_degraded_efficiency: f32,
    pub wear_band_critical_efficiency: f32,
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
