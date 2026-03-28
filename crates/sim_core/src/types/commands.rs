//! Command types: `CommandEnvelope`, `Command`.

use serde::{Deserialize, Serialize};

use crate::{
    CommandId, ComponentId, CrewRole, ModuleDefId, ModuleInstanceId, ModuleItemId, PrincipalId,
    RecipeId, ShipId, StationId, TaskKind, TechId, TradeItemSpec,
};

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
    #[serde(alias = "SetManufacturingPriority")]
    SetModulePriority {
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
    AssignCrew {
        station_id: StationId,
        module_id: ModuleInstanceId,
        role: CrewRole,
        count: u32,
    },
    UnassignCrew {
        station_id: StationId,
        module_id: ModuleInstanceId,
        role: CrewRole,
        count: u32,
    },
    CreateThermalLink {
        station_id: StationId,
        from_module_id: ModuleInstanceId,
        from_port_id: String,
        to_module_id: ModuleInstanceId,
        to_port_id: String,
    },
    RemoveThermalLink {
        station_id: StationId,
        from_module_id: ModuleInstanceId,
        from_port_id: String,
        to_module_id: ModuleInstanceId,
        to_port_id: String,
    },
    /// Transfer molten material along a thermal link.
    TransferMolten {
        station_id: StationId,
        from_module_id: ModuleInstanceId,
        to_module_id: ModuleInstanceId,
        element: String,
        kg: f32,
    },
}
