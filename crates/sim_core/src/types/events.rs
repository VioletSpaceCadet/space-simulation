//! Event types: `EventEnvelope`, `Event`.

use serde::{Deserialize, Serialize};

use crate::{
    AlertSeverity, AnomalyTag, AsteroidId, ComponentId, CompositionVec, DataKind, ElementId,
    EventId, HullId, InventoryItem, ModuleDefId, ModuleInstanceId, ModuleItemId, PowerState,
    RecipeId, ResearchDomain, ShipId, SiteId, StationId, TechId, TradeItemSpec,
};

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
        behavior_type: crate::BehaviorType,
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
