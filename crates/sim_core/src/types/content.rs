//! Content definition types: `GameContent`, modules, recipes, techs, solar system.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::{
    AlertSeverity, AnomalyTag, AssemblerState, BatteryState, BehaviorType, BodyId, ComponentId,
    Constants, DataKind, ElementId, FittedModule, HullId, ItemKind, LabState, MaintenanceState,
    ModuleKindState, NodeId, PricingTable, ProcessorState, RadiatorState, RecipeId, ResearchDomain,
    SensorArrayState, SlotType, SolarArrayState, TechId, ThermalGroupId,
};

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
// Game content
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
    /// Default fitting loadouts per hull from `content/fitting_templates.json`.
    #[serde(default)]
    pub fitting_templates: BTreeMap<HullId, Vec<FittedModule>>,
    /// Pre-computed element id -> density (kg/m3) lookup. Populated by `init_caches()`.
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

// ---------------------------------------------------------------------------
// Component definitions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentDef {
    pub id: String,
    pub name: String,
    pub mass_kg: f32,
    pub volume_m3: f32,
}

// ---------------------------------------------------------------------------
// Tech definitions
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Solar system definitions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolarSystemDef {
    /// Hierarchical body tree -- the source of truth for spatial layout.
    pub bodies: Vec<OrbitalBodyDef>,
    /// Legacy node list -- kept for backward compat until graph pathfinding is replaced.
    #[serde(default)]
    pub nodes: Vec<NodeDef>,
    /// Legacy edge list -- kept for backward compat until graph pathfinding is replaced.
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

// ---------------------------------------------------------------------------
// Asteroid template definitions
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Element definitions
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Thermal content definition
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Module definitions
// ---------------------------------------------------------------------------

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
    /// Returns a stable lowercase name for the behavior type, used as the key
    /// in `per_module_metrics` and as a column name prefix in CSV/Parquet.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Processor(_) => "processor",
            Self::Storage { .. } => "storage",
            Self::Maintenance(_) => "maintenance",
            Self::Assembler(_) => "assembler",
            Self::Lab(_) => "lab",
            Self::SensorArray(_) => "sensor_array",
            Self::SolarArray(_) => "solar_array",
            Self::Battery(_) => "battery",
            Self::Radiator(_) => "radiator",
            Self::Equipment => "equipment",
        }
    }

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

    /// Returns the default runtime state and behavior type tag for this module behavior.
    pub fn default_state(&self) -> (ModuleKindState, BehaviorType) {
        match self {
            Self::Processor(_) => (
                ModuleKindState::Processor(ProcessorState {
                    threshold_kg: 0.0,
                    ticks_since_last_run: 0,
                    stalled: false,
                    selected_recipe: None,
                }),
                BehaviorType::Processor,
            ),
            Self::Storage { .. } => (ModuleKindState::Storage, BehaviorType::Storage),
            Self::Maintenance(_) => (
                ModuleKindState::Maintenance(MaintenanceState {
                    ticks_since_last_run: 0,
                }),
                BehaviorType::Maintenance,
            ),
            Self::Assembler(_) => (
                ModuleKindState::Assembler(AssemblerState {
                    ticks_since_last_run: 0,
                    stalled: false,
                    capped: false,
                    cap_override: std::collections::HashMap::new(),
                    selected_recipe: None,
                }),
                BehaviorType::Assembler,
            ),
            Self::Lab(_) => (
                ModuleKindState::Lab(LabState {
                    ticks_since_last_run: 0,
                    assigned_tech: None,
                    starved: false,
                }),
                BehaviorType::Lab,
            ),
            Self::SensorArray(_) => (
                ModuleKindState::SensorArray(SensorArrayState::default()),
                BehaviorType::SensorArray,
            ),
            Self::SolarArray(_) => (
                ModuleKindState::SolarArray(SolarArrayState::default()),
                BehaviorType::SolarArray,
            ),
            Self::Battery(_) => (
                ModuleKindState::Battery(BatteryState { charge_kwh: 0.0 }),
                BehaviorType::Battery,
            ),
            Self::Radiator(_) => (
                ModuleKindState::Radiator(RadiatorState::default()),
                BehaviorType::Radiator,
            ),
            Self::Equipment => (ModuleKindState::Equipment, BehaviorType::Equipment),
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

// ---------------------------------------------------------------------------
// Behavior sub-definitions
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Recipe definitions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeThermalReq {
    /// Below this temperature the processor stalls (`TooCold`).
    pub min_temp_mk: u32,
    /// Between `min_temp_mk` and `optimal_min_mk`: efficiency ramps 80%->100%.
    pub optimal_min_mk: u32,
    /// Between `optimal_min_mk` and `optimal_max_mk`: 100% efficiency, 100% quality.
    pub optimal_max_mk: u32,
    /// Between `optimal_max_mk` and `max_temp_mk`: quality degrades 100%->60%.
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
