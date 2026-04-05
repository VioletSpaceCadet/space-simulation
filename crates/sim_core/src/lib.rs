//! `sim_core` — deterministic simulation tick.
//!
//! No IO, no network. All randomness via the passed-in Rng.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub(crate) mod commands;
mod composition;
mod engine;
mod id;
pub mod instrumentation;
pub mod metrics;
pub mod milestone;
pub mod modifiers;
pub mod propulsion;
mod research;
pub(crate) mod satellite;
pub mod scoring;
pub mod sim_events;
pub mod spatial;
mod station;
pub(crate) mod tasks;
pub mod thermal;
pub mod trade;
mod types;
pub mod wear;

pub use commands::recompute_ship_stats;
pub use engine::{tick, trade_unlock_tick};
pub use id::generate_uuid;
pub use instrumentation::{compute_step_stats, StepStats, TickTimings};
pub use metrics::{
    append_metrics_row, compute_metrics, content_behavior_types, content_element_ids,
    write_metrics_csv, write_metrics_header, MetricType, MetricValue, MetricsFileWriter,
    MetricsSnapshot, ModuleStatusMetrics, OreElementStats, METRICS_VERSION,
};
pub use satellite::{zone_comm_tier, zone_nav_bonus};
pub use spatial::{
    build_body_cache, compute_entity_absolute, integer_sqrt, is_co_located, pick_template_biased,
    pick_zone_weighted, polar_to_cart, random_angle_in_span, random_position_in_zone,
    random_radius_in_band, travel_ticks, AbsolutePos, AngleMilliDeg, BodyCache, EntityCache,
    Position, RadiusAuMicro, ResourceClass, FULL_CIRCLE, METERS_PER_AU, METERS_PER_MICRO_AU,
};
pub use tasks::{inventory_mass_kg, inventory_volume_m3, mine_duration};
// -- types: ID newtypes --
pub use types::{
    AsteroidId, BodyId, CommandId, ComponentId, CrewRole, EventId, FacilityId, FrameId,
    GroundFacilityId, HullId, LeaderId, LotId, ModuleDefId, ModuleInstanceId, ModuleItemId, NodeId,
    PrincipalId, RecipeId, SatelliteId, ShipId, SiteId, SlotType, StationId, TechId,
};
// -- types: type aliases & constants --
pub use types::{
    AHashMap, CompositionVec, ElementId, ThermalGroupId, COMPONENT_REPAIR_KIT, COMPONENT_THRUSTER,
    CURRENT_SCHEMA_VERSION, DEFAULT_AMBIENT_TEMP_MK, ELEMENT_FE, ELEMENT_ORE, ELEMENT_SLAG,
    TAG_IRON_RICH, TAG_VOLATILE_RICH,
};
// -- types: core enums --
pub use types::{
    AlertSeverity, AnomalyTag, BehaviorType, CommTier, DataKind, DomainProgress, ItemKind,
    OverheatZone, Phase, ResearchDomain,
};
// -- types: game state --
pub use types::{
    AsteroidKnowledge, AsteroidState, Counters, FacilityCore, GameState, GroundFacilityState,
    LaunchPayload, LaunchTransitState, MetaState, ModuleTypeIndex, PowerBudgetCache, PowerState,
    ResearchState, SatelliteState, ScanSite, StationState, TaskState, ThermalLink,
};
// -- types: ship state --
pub use types::{FittedModule, ShipState, TaskKind};
// -- types: module state --
pub use types::{
    compute_crew_factor, compute_module_efficiency, is_crew_satisfied, AssemblerState,
    BatteryState, LabState, LaunchPadState, MaintenanceState, ModuleKindState, ModuleState,
    ProcessorState, RadiatorState, SensorArrayState, SolarArrayState, ThermalContainerState,
    WearState,
};
// -- types: thermal state --
pub use types::{MaterialThermalProps, ThermalState};
// -- types: content definitions --
pub use types::{
    AlertRuleDef, AlertRuleType, AsteroidTemplateDef, AutopilotConfig, BodyType, BoiloffCurveDef,
    ComponentDef, CrewRoleDef, DeepScanTargetConfig, ElementDef, ExportComponentConfig,
    ExportElementConfig, FrameDef, GameContent, HullDef, InitialComponent, InitialMaterial,
    InitialStationDef, NodeDef, OrbitalBodyDef, RocketDef, SatelliteDef, SlotDef, SolarSystemDef,
    TechDef, TechEffect, ThermalDef, ZoneDef,
};
// -- types: module & recipe definitions --
pub use types::{
    AssemblerDef, BatteryDef, InputAmount, InputFilter, LabDef, LaunchPadDef, MaintenanceDef,
    ModuleBehaviorDef, ModuleDef, ModulePort, OutputSpec, PortDirection, PortFilter, ProcessorDef,
    QualityFormula, RadiatorDef, RecipeDef, RecipeInput, RecipeThermalReq, SensorArrayDef,
    SolarArrayDef, ThermalContainerDef, YieldFormula,
};
// -- types: progression --
pub use types::{
    GamePhase, GrantRecord, MilestoneCondition, MilestoneDef, MilestoneReward, ProgressionState,
    TradeTier,
};
// -- types: commands & events --
pub use types::{Command, CommandEnvelope, Event, EventEnvelope};
// -- types: inventory & trade --
pub use types::{InventoryItem, PricingEntry, PricingTable, TradeItemSpec};
// -- types: constants & functions --
pub use scoring::{
    compute_run_score, validate_scoring_config, DimensionDef, DimensionScore, RunScore,
    ScoringConfig, ThresholdDef,
};
pub use types::{boiloff_rate_per_tick, derive_module_tick_values, Constants};
pub use wear::wear_efficiency;

pub(crate) fn emit(counters: &mut Counters, tick: u64, event: Event) -> EventEnvelope {
    let id = EventId(counters.next_event_id);
    counters.next_event_id += 1;
    EventEnvelope { id, tick, event }
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_fixtures;
#[cfg(test)]
mod tests;
