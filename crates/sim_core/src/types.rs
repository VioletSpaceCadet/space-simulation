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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventLevel {
    Normal,
    Debug,
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
    pub task: Option<TaskState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationState {
    pub id: StationId,
    pub location_node: NodeId,
    pub power_available_per_tick: f32,
    pub facilities: FacilitiesState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacilitiesState {
    pub compute_units_total: u32,
    pub power_per_compute_unit_per_tick: f32,
    /// Evidence produced per compute-unit per tick. Baseline 1.0.
    pub efficiency: f32,
}

/// Research distributes automatically to all eligible techs — no player allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchState {
    pub unlocked: HashSet<TechId>,
    pub data_pool: HashMap<DataKind, f32>,
    pub evidence: HashMap<TechId, f32>,
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
        quality: f32,
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
    /// Only emitted at `EventLevel::Debug`.
    ResearchRoll {
        tech_id: TechId,
        evidence: f32,
        p: f32,
        rolled: f32,
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
    pub constants: Constants,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechDef {
    pub id: TechId,
    pub name: String,
    pub prereqs: Vec<TechId>,
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
pub struct Constants {
    pub survey_scan_ticks: u64,
    pub deep_scan_ticks: u64,
    /// Ticks to travel one hop on the solar system graph.
    pub travel_ticks_per_hop: u64,
    pub survey_scan_data_amount: f32,
    pub survey_scan_data_quality: f32,
    pub deep_scan_data_amount: f32,
    pub deep_scan_data_quality: f32,
    pub survey_tag_detection_probability: f32,
    pub asteroid_count_per_template: u32,
    pub asteroid_mass_min_kg: f32,
    pub asteroid_mass_max_kg: f32,
    pub station_compute_units_total: u32,
    pub station_power_per_compute_unit_per_tick: f32,
    pub station_efficiency: f32,
    pub station_power_available_per_tick: f32,
}
