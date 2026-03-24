//! Inventory, trade, and pricing types.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{
    AsteroidId, ComponentId, CompositionVec, ElementId, GameContent, LotId, MaterialThermalProps,
    ModuleItemId,
};

// ---------------------------------------------------------------------------
// Inventory items
// ---------------------------------------------------------------------------

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

impl InventoryItem {
    /// Mass in kg for mass-bearing variants (Ore, Slag, Material). Returns 0 for others.
    pub fn mass_kg(&self) -> f32 {
        match self {
            Self::Ore { kg, .. } | Self::Slag { kg, .. } | Self::Material { kg, .. } => *kg,
            Self::Component { .. } | Self::Module { .. } => 0.0,
        }
    }

    /// Mutable reference to mass for mass-bearing variants. Returns `None` for others.
    pub fn mass_kg_mut(&mut self) -> Option<&mut f32> {
        match self {
            Self::Ore { kg, .. } | Self::Slag { kg, .. } | Self::Material { kg, .. } => Some(kg),
            Self::Component { .. } | Self::Module { .. } => None,
        }
    }

    pub fn is_ore(&self) -> bool {
        matches!(self, Self::Ore { .. })
    }

    pub fn is_slag(&self) -> bool {
        matches!(self, Self::Slag { .. })
    }

    pub fn is_material(&self) -> bool {
        matches!(self, Self::Material { .. })
    }

    pub fn is_component(&self) -> bool {
        matches!(self, Self::Component { .. })
    }

    pub fn is_module(&self) -> bool {
        matches!(self, Self::Module { .. })
    }

    /// Element ID for Material variants; `None` for others.
    pub fn element_id(&self) -> Option<&str> {
        match self {
            Self::Material { element, .. } => Some(element),
            _ => None,
        }
    }

    /// Component ID for Component variants; `None` for others.
    pub fn component_id(&self) -> Option<&str> {
        match self {
            Self::Component { component_id, .. } => Some(&component_id.0),
            _ => None,
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

impl TradeItemSpec {
    /// Return the pricing lookup key for this trade item.
    pub fn pricing_key(&self) -> &str {
        match self {
            Self::Material { element, .. } => element.as_str(),
            Self::Component { component_id, .. } => component_id.0.as_str(),
            Self::Module { module_def_id } => module_def_id.as_str(),
        }
    }

    /// Compute total mass in kg. Returns `None` if def not found in content.
    pub fn compute_mass(&self, content: &GameContent) -> Option<f64> {
        match self {
            Self::Material { kg, .. } => Some(f64::from(*kg)),
            Self::Component {
                component_id,
                count,
            } => {
                let def = content
                    .component_defs
                    .iter()
                    .find(|d| d.id == component_id.0)?;
                Some(f64::from(def.mass_kg) * f64::from(*count))
            }
            Self::Module { module_def_id } => {
                let def = content.module_defs.get(module_def_id.as_str())?;
                Some(f64::from(def.mass_kg))
            }
        }
    }

    /// Compute the quantity (unit count) for pricing calculation.
    pub fn quantity(&self) -> f64 {
        match self {
            Self::Material { kg, .. } => f64::from(*kg),
            Self::Component { count, .. } => f64::from(*count),
            Self::Module { .. } => 1.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Pricing
// ---------------------------------------------------------------------------

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
