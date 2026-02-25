//! Trade helpers for import/export pricing and inventory manipulation.

use crate::{GameContent, InventoryItem, ModuleItemId, PricingEntry, PricingTable, TradeItemSpec};
use rand::Rng;

/// Return the pricing lookup key for a trade item spec.
pub fn pricing_key(item_spec: &TradeItemSpec) -> &str {
    match item_spec {
        TradeItemSpec::Material { element, .. } => element.as_str(),
        TradeItemSpec::Component { component_id, .. } => component_id.0.as_str(),
        TradeItemSpec::Module { module_def_id } => module_def_id.as_str(),
    }
}

/// Compute total mass in kg for a trade item spec.
/// Returns `None` if a component or module def is not found in content.
pub fn compute_mass(item_spec: &TradeItemSpec, content: &GameContent) -> Option<f64> {
    match item_spec {
        TradeItemSpec::Material { kg, .. } => Some(*kg as f64),
        TradeItemSpec::Component {
            component_id,
            count,
        } => {
            let def = content
                .component_defs
                .iter()
                .find(|d| d.id == component_id.0)?;
            Some(def.mass_kg as f64 * *count as f64)
        }
        TradeItemSpec::Module { module_def_id } => {
            let def = content
                .module_defs
                .iter()
                .find(|d| d.id == *module_def_id)?;
            Some(def.mass_kg as f64)
        }
    }
}

/// Compute the quantity (unit count) for pricing calculation.
fn quantity(item_spec: &TradeItemSpec) -> f64 {
    match item_spec {
        TradeItemSpec::Material { kg, .. } => *kg as f64,
        TradeItemSpec::Component { count, .. } => *count as f64,
        TradeItemSpec::Module { .. } => 1.0,
    }
}

/// Compute the import cost for a trade item.
/// Returns `None` if pricing entry not found, item not importable, or mass can't be computed.
pub fn compute_import_cost(
    item_spec: &TradeItemSpec,
    pricing: &PricingTable,
    content: &GameContent,
) -> Option<f64> {
    let key = pricing_key(item_spec);
    let entry: &PricingEntry = pricing.items.get(key)?;
    if !entry.importable {
        return None;
    }
    let mass = compute_mass(item_spec, content)?;
    let cost =
        entry.base_price_per_unit * quantity(item_spec) + mass * pricing.import_surcharge_per_kg;
    Some(cost)
}

/// Compute the export revenue for a trade item.
/// Returns `None` if pricing entry not found, item not exportable, or mass can't be computed.
pub fn compute_export_revenue(
    item_spec: &TradeItemSpec,
    pricing: &PricingTable,
    content: &GameContent,
) -> Option<f64> {
    let key = pricing_key(item_spec);
    let entry: &PricingEntry = pricing.items.get(key)?;
    if !entry.exportable {
        return None;
    }
    let mass = compute_mass(item_spec, content)?;
    let revenue = (entry.base_price_per_unit * quantity(item_spec)
        - mass * pricing.export_surcharge_per_kg)
        .max(0.0);
    Some(revenue)
}

/// Create inventory items for an import operation.
/// For modules, generates a unique `ModuleItemId` using the RNG.
pub fn create_inventory_items(item_spec: &TradeItemSpec, rng: &mut impl Rng) -> Vec<InventoryItem> {
    match item_spec {
        TradeItemSpec::Material { element, kg } => {
            vec![InventoryItem::Material {
                element: element.clone(),
                kg: *kg,
                quality: 1.0,
            }]
        }
        TradeItemSpec::Component {
            component_id,
            count,
        } => {
            vec![InventoryItem::Component {
                component_id: component_id.clone(),
                count: *count,
                quality: 1.0,
            }]
        }
        TradeItemSpec::Module { module_def_id } => {
            let uuid = crate::generate_uuid(rng);
            vec![InventoryItem::Module {
                item_id: ModuleItemId(format!("module_item_{uuid}")),
                module_def_id: module_def_id.clone(),
            }]
        }
    }
}

/// Check whether the station inventory has enough items to fulfill an export.
pub fn has_enough_for_export(inventory: &[InventoryItem], item_spec: &TradeItemSpec) -> bool {
    match item_spec {
        TradeItemSpec::Material { element, kg } => {
            let available: f32 = inventory
                .iter()
                .filter_map(|item| match item {
                    InventoryItem::Material {
                        element: el, kg, ..
                    } if el == element => Some(*kg),
                    _ => None,
                })
                .sum();
            available >= *kg
        }
        TradeItemSpec::Component {
            component_id,
            count,
        } => {
            let available: u32 = inventory
                .iter()
                .filter_map(|item| match item {
                    InventoryItem::Component {
                        component_id: cid,
                        count,
                        ..
                    } if cid == component_id => Some(*count),
                    _ => None,
                })
                .sum();
            available >= *count
        }
        TradeItemSpec::Module { module_def_id } => inventory.iter().any(|item| {
            matches!(item, InventoryItem::Module { module_def_id: def_id, .. } if def_id == module_def_id)
        }),
    }
}

/// Remove items from inventory for an export. Returns true if successful.
/// Materials: reduces kg from matching elements FIFO, removes entries with 0 kg.
/// Components: reduces count FIFO, removes entries with 0 count.
/// Modules: removes first matching module.
pub fn remove_inventory_items(
    inventory: &mut Vec<InventoryItem>,
    item_spec: &TradeItemSpec,
) -> bool {
    match item_spec {
        TradeItemSpec::Material { element, kg } => {
            let mut remaining = *kg;
            for item in inventory.iter_mut() {
                if remaining <= 0.0 {
                    break;
                }
                if let InventoryItem::Material {
                    element: el,
                    kg: item_kg,
                    ..
                } = item
                {
                    if el == element {
                        let take = item_kg.min(remaining);
                        *item_kg -= take;
                        remaining -= take;
                    }
                }
            }
            // Remove zero-kg entries
            inventory
                .retain(|item| !matches!(item, InventoryItem::Material { kg, .. } if *kg <= 0.0));
            remaining <= 0.0
        }
        TradeItemSpec::Component {
            component_id,
            count,
        } => {
            let mut remaining = *count;
            for item in inventory.iter_mut() {
                if remaining == 0 {
                    break;
                }
                if let InventoryItem::Component {
                    component_id: cid,
                    count: item_count,
                    ..
                } = item
                {
                    if cid == component_id {
                        let take = (*item_count).min(remaining);
                        *item_count -= take;
                        remaining -= take;
                    }
                }
            }
            // Remove zero-count entries
            inventory.retain(
                |item| !matches!(item, InventoryItem::Component { count, .. } if *count == 0),
            );
            remaining == 0
        }
        TradeItemSpec::Module { module_def_id } => {
            let pos = inventory.iter().position(|item| {
                matches!(item, InventoryItem::Module { module_def_id: def_id, .. } if def_id == module_def_id)
            });
            if let Some(pos) = pos {
                inventory.remove(pos);
                true
            } else {
                false
            }
        }
    }
}

/// Merge imported items into existing inventory.
/// Materials merge with existing entries of the same element and quality.
/// Components merge with existing entries of the same component_id and quality.
/// Modules are appended directly.
pub fn merge_into_inventory(inventory: &mut Vec<InventoryItem>, new_items: Vec<InventoryItem>) {
    for new_item in new_items {
        match &new_item {
            InventoryItem::Material {
                element,
                kg,
                quality,
            } => {
                let existing = inventory.iter_mut().find(|item| {
                    matches!(item, InventoryItem::Material { element: el, quality: q, .. }
                        if el == element && (*q - quality).abs() < f32::EPSILON)
                });
                if let Some(InventoryItem::Material {
                    kg: existing_kg, ..
                }) = existing
                {
                    *existing_kg += kg;
                } else {
                    inventory.push(new_item);
                }
            }
            InventoryItem::Component {
                component_id,
                count,
                quality,
            } => {
                let existing = inventory.iter_mut().find(|item| {
                    matches!(item, InventoryItem::Component { component_id: cid, quality: q, .. }
                        if cid == component_id && (*q - quality).abs() < f32::EPSILON)
                });
                if let Some(InventoryItem::Component {
                    count: existing_count,
                    ..
                }) = existing
                {
                    *existing_count += count;
                } else {
                    inventory.push(new_item);
                }
            }
            InventoryItem::Module { .. } => {
                inventory.push(new_item);
            }
            _ => {
                inventory.push(new_item);
            }
        }
    }
}
