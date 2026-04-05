//! Command types: `CommandEnvelope`, `Command`.

use serde::{Deserialize, Serialize};

use crate::{
    CommandId, ComponentId, CrewRole, FacilityId, GroundFacilityId, LaunchPayload, ModuleDefId,
    ModuleInstanceId, ModuleItemId, Position, PrincipalId, RecipeId, ShipId, StationId, TaskKind,
    TechId, TradeItemSpec,
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
    /// Deploy a station kit at a target position. The ship is routed
    /// through `Transit` → `ConstructStation`; on completion an empty
    /// `StationState` with the kit's frame is created. The kit is consumed
    /// from the ship's cargo by the command handler before transit starts
    /// so a ship cannot deploy the same kit twice.
    DeployStation {
        ship_id: ShipId,
        /// Inventory index of the station kit component in the ship's cargo.
        /// Must reference an `InventoryItem::Component { component_id, .. }`
        /// whose `ComponentDef.deploys_frame` is `Some(frame_id)`.
        kit_item_index: usize,
        /// Where to build the station. The ship will transit here first.
        target_position: Position,
    },
    InstallModule {
        facility_id: FacilityId,
        module_item_id: ModuleItemId,
        /// Target slot on the station frame. `None` = auto-find first
        /// compatible unoccupied slot (autopilot's default path). Ignored
        /// for frameless stations and ground facilities, which fall back
        /// to the legacy unlimited-slot behavior.
        #[serde(default)]
        slot_index: Option<usize>,
    },
    UninstallModule {
        facility_id: FacilityId,
        module_id: ModuleInstanceId,
    },
    SetModuleEnabled {
        facility_id: FacilityId,
        module_id: ModuleInstanceId,
        enabled: bool,
    },
    SetModuleThreshold {
        facility_id: FacilityId,
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
        facility_id: FacilityId,
        item_spec: TradeItemSpec,
    },
    Export {
        facility_id: FacilityId,
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
    /// Launch a rocket from a ground facility to deliver payload to orbit.
    Launch {
        facility_id: GroundFacilityId,
        rocket_def_id: String,
        payload: LaunchPayload,
        destination: Position,
    },
    /// Transfer molten material along a thermal link.
    TransferMolten {
        station_id: StationId,
        from_module_id: ModuleInstanceId,
        to_module_id: ModuleInstanceId,
        element: String,
        kg: f32,
    },
    /// Deploy a satellite from an orbital station's inventory into the same zone.
    DeploySatellite {
        station_id: StationId,
        /// Matches `SatelliteDef.id` and `ComponentId` of the satellite product.
        satellite_def_id: String,
    },
    /// Replace `GameState.strategy_config` with a new `StrategyConfig`. Full
    /// replacement semantics (not merge). Applied at a tick boundary via the
    /// command queue so runtime strategy changes remain deterministic.
    SetStrategyConfig {
        config: crate::StrategyConfig,
    },
    /// VIO-595: Move inventory items from one orbital station to another
    /// via a ship. The command handler builds a chained task
    /// `Transit(src) → Pickup → Transit(dst) → Deposit` and assigns it
    /// to the ship. Materials and Components may be partially filled
    /// (split by mass/count); Modules are atomic. `items` with Crew
    /// variants are ignored (crew transfer is not supported by this
    /// command — use Import/Export via trade instead).
    TransferItems {
        ship_id: ShipId,
        from_station: StationId,
        to_station: StationId,
        items: Vec<TradeItemSpec>,
    },
}
