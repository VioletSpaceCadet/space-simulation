//! Type definitions for `sim_core`.
//!
//! All public types, structs, enums, and ID newtypes used by the simulation.
//! Organized into focused submodules; everything is re-exported for backward
//! compatibility.

mod commands;
mod constants;
mod content;
mod events;
mod inventory;
mod progression;
mod state;

pub use commands::*;
pub use constants::*;
pub use content::*;
pub use events::*;
pub use inventory::*;
pub use progression::*;
pub use state::*;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

/// Fast non-cryptographic `HashMap` for hot-path lookups. Uses `ahash` instead of
/// `SipHash` — safe for game sim internals where `DoS` resistance is unnecessary.
pub type AHashMap<K, V> = HashMap<K, V, ahash::RandomState>;

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
// Ambient temperature constant
// ---------------------------------------------------------------------------

/// 20 C in milli-Kelvin -- shared default for ambient/sink temperature.
pub const DEFAULT_AMBIENT_TEMP_MK: u32 = 293_000;

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

/// Numeric ID types — cheap to create (no heap allocation).
macro_rules! numeric_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        pub struct $name(pub u64);

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
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
numeric_id!(CommandId);
numeric_id!(EventId);
string_id!(PrincipalId);
string_id!(LotId);
string_id!(GroundFacilityId);

/// A reference to either a station or a ground facility. Used by commands
/// that apply to both entity types (`Import`, `Export`, `InstallModule`, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FacilityId {
    Station(StationId),
    Ground(GroundFacilityId),
}

impl From<StationId> for FacilityId {
    fn from(id: StationId) -> Self {
        Self::Station(id)
    }
}

impl From<GroundFacilityId> for FacilityId {
    fn from(id: GroundFacilityId) -> Self {
        Self::Ground(id)
    }
}

impl std::fmt::Display for FacilityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Station(id) => write!(f, "{id}"),
            Self::Ground(id) => write!(f, "{id}"),
        }
    }
}
string_id!(ModuleItemId);
string_id!(ModuleInstanceId);
string_id!(ComponentId);
string_id!(RecipeId);
string_id!(HullId);
string_id!(SlotType);
string_id!(ModuleDefId);
string_id!(CrewRole);
string_id!(LeaderId);

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

/// Data-driven data kind. Values come from content JSON (sensor defs, lab defs, techs).
/// Adding a new sensor/data type = adding a JSON entry, not a code change.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DataKind(pub String);

impl DataKind {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for DataKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Well-known data kind constants for the 4 original data types.
impl DataKind {
    pub const SURVEY: &str = "SurveyData";
    pub const ASSAY: &str = "AssayData";
    pub const MANUFACTURING: &str = "ManufacturingData";
    pub const TRANSIT: &str = "TransitData";
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Warning,
    Critical,
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
    ThermalContainer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ItemKind {
    Ore,
    Slag,
    Material,
    Component,
}

// ---------------------------------------------------------------------------
// Thermal primitives
// ---------------------------------------------------------------------------

/// String alias for grouping modules into thermal groups.
/// Modules in the same group share radiator cooling.
pub type ThermalGroupId = String;

/// Overheat zone classification for a thermal module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OverheatZone {
    /// Below `max_temp_mk` -- normal operation.
    #[default]
    Nominal,
    /// Above `max_temp_mk` + warning offset -- accelerated wear (2x default).
    Warning,
    /// Above `max_temp_mk` + critical offset -- auto-stall + accelerated wear (4x default).
    Critical,
    /// Above `max_temp_mk` + damage offset -- wear jumps to `wear_band_critical_threshold`, auto-disable.
    Damage,
}

/// Phase of a material batch (solid or liquid).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Solid,
    Liquid,
}
