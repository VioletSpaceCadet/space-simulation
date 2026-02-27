//! Content/schema validation tests for JSON game data (VIO-194).
//!
//! These tests load the actual `content/*.json` files and validate:
//! 1. Schema validity — all files deserialize without error
//! 2. Range constraints — no negative prices, no zero durations, no empty IDs
//! 3. Cross-reference integrity — all inter-file references resolve
//! 4. Content invariants — the game world is playable
//! 5. Balance sanity checks — flag extreme outliers

use sim_core::{
    ComponentId, GameContent, InputFilter, ModuleBehaviorDef, OutputSpec, TechEffect, TechId,
};
use sim_world::load_content;
use std::collections::HashSet;
use std::sync::OnceLock;

/// Helper: resolve the content directory relative to the workspace root.
/// Integration tests run from the crate directory, so we go up two levels.
fn content_dir() -> String {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    format!("{manifest}/../../content")
}

/// Shared content loaded once across all tests in this module.
fn load_test_content() -> &'static GameContent {
    static CONTENT: OnceLock<GameContent> = OnceLock::new();
    CONTENT.get_or_init(|| {
        load_content(&content_dir()).expect("load_content should succeed for production content")
    })
}

// =========================================================================
// 1. Schema validation — deserialization succeeds
// =========================================================================

#[test]
fn content_loads_successfully() {
    let _content = load_test_content();
}

// =========================================================================
// 2. Range constraints
// =========================================================================

#[test]
fn element_ids_are_non_empty() {
    let content = load_test_content();
    for element in &content.elements {
        assert!(!element.id.is_empty(), "element has empty id");
    }
}

#[test]
fn element_densities_are_positive() {
    let content = load_test_content();
    for element in &content.elements {
        assert!(
            element.density_kg_per_m3 > 0.0,
            "element '{}' has non-positive density: {}",
            element.id,
            element.density_kg_per_m3
        );
    }
}

#[test]
fn tech_ids_are_non_empty() {
    let content = load_test_content();
    for tech in &content.techs {
        assert!(!tech.id.0.is_empty(), "tech has empty id");
    }
}

#[test]
fn tech_difficulties_are_positive() {
    let content = load_test_content();
    for tech in &content.techs {
        assert!(
            tech.difficulty > 0.0,
            "tech '{}' has non-positive difficulty: {}",
            tech.id.0,
            tech.difficulty
        );
    }
}

#[test]
fn tech_domain_requirements_are_positive() {
    let content = load_test_content();
    for tech in &content.techs {
        for (domain, points) in &tech.domain_requirements {
            assert!(
                *points > 0.0,
                "tech '{}' domain {:?} has non-positive requirement: {}",
                tech.id.0,
                domain,
                points
            );
        }
    }
}

#[test]
fn module_ids_are_non_empty() {
    let content = load_test_content();
    for module_id in content.module_defs.keys() {
        assert!(!module_id.is_empty(), "module has empty id");
    }
}

#[test]
fn module_masses_are_positive() {
    let content = load_test_content();
    for module_def in content.module_defs.values() {
        assert!(
            module_def.mass_kg > 0.0,
            "module '{}' has non-positive mass: {}",
            module_def.id,
            module_def.mass_kg
        );
    }
}

#[test]
fn module_volumes_are_positive() {
    let content = load_test_content();
    for module_def in content.module_defs.values() {
        assert!(
            module_def.volume_m3 > 0.0,
            "module '{}' has non-positive volume: {}",
            module_def.id,
            module_def.volume_m3
        );
    }
}

#[test]
fn module_wear_per_run_is_non_negative() {
    let content = load_test_content();
    for module_def in content.module_defs.values() {
        assert!(
            module_def.wear_per_run >= 0.0,
            "module '{}' has negative wear_per_run: {}",
            module_def.id,
            module_def.wear_per_run
        );
    }
}

#[test]
fn module_power_consumption_is_non_negative() {
    let content = load_test_content();
    for module_def in content.module_defs.values() {
        assert!(
            module_def.power_consumption_per_run >= 0.0,
            "module '{}' has negative power_consumption_per_run: {}",
            module_def.id,
            module_def.power_consumption_per_run
        );
    }
}

#[test]
fn storage_capacity_is_positive() {
    let content = load_test_content();
    for module_def in content.module_defs.values() {
        if let ModuleBehaviorDef::Storage { capacity_m3 } = &module_def.behavior {
            assert!(
                *capacity_m3 > 0.0,
                "module '{}' storage has non-positive capacity_m3: {}",
                module_def.id,
                capacity_m3
            );
        }
    }
}

#[test]
fn component_ids_are_non_empty() {
    let content = load_test_content();
    for component in &content.component_defs {
        assert!(!component.id.is_empty(), "component has empty id");
    }
}

#[test]
fn component_masses_are_positive() {
    let content = load_test_content();
    for component in &content.component_defs {
        assert!(
            component.mass_kg > 0.0,
            "component '{}' has non-positive mass: {}",
            component.id,
            component.mass_kg
        );
    }
}

#[test]
fn component_volumes_are_positive() {
    let content = load_test_content();
    for component in &content.component_defs {
        assert!(
            component.volume_m3 > 0.0,
            "component '{}' has non-positive volume: {}",
            component.id,
            component.volume_m3
        );
    }
}

#[test]
fn pricing_base_prices_are_positive() {
    let content = load_test_content();
    for (item_id, entry) in &content.pricing.items {
        assert!(
            entry.base_price_per_unit > 0.0,
            "pricing item '{}' has non-positive base_price_per_unit: {}",
            item_id,
            entry.base_price_per_unit
        );
    }
}

#[test]
fn pricing_surcharges_are_non_negative() {
    let content = load_test_content();
    assert!(
        content.pricing.import_surcharge_per_kg >= 0.0,
        "import_surcharge_per_kg is negative: {}",
        content.pricing.import_surcharge_per_kg
    );
    assert!(
        content.pricing.export_surcharge_per_kg >= 0.0,
        "export_surcharge_per_kg is negative: {}",
        content.pricing.export_surcharge_per_kg
    );
}

#[test]
fn constants_durations_are_positive() {
    let content = load_test_content();
    let c = &content.constants;
    assert!(c.minutes_per_tick > 0, "minutes_per_tick must be > 0");
    assert!(c.survey_scan_minutes > 0, "survey_scan_minutes must be > 0");
    assert!(c.deep_scan_minutes > 0, "deep_scan_minutes must be > 0");
    assert!(
        c.travel_minutes_per_hop > 0,
        "travel_minutes_per_hop must be > 0"
    );
    assert!(c.deposit_minutes > 0, "deposit_minutes must be > 0");
    assert!(
        c.research_roll_interval_minutes > 0,
        "research_roll_interval_minutes must be > 0"
    );
}

#[test]
fn constants_capacities_are_positive() {
    let content = load_test_content();
    let c = &content.constants;
    assert!(
        c.ship_cargo_capacity_m3 > 0.0,
        "ship_cargo_capacity_m3 must be > 0"
    );
    assert!(
        c.station_cargo_capacity_m3 > 0.0,
        "station_cargo_capacity_m3 must be > 0"
    );
}

#[test]
fn constants_rates_are_positive() {
    let content = load_test_content();
    let c = &content.constants;
    assert!(
        c.mining_rate_kg_per_minute > 0.0,
        "mining_rate_kg_per_minute must be > 0"
    );
}

#[test]
fn constants_probabilities_are_valid() {
    let content = load_test_content();
    let c = &content.constants;
    assert!(
        (0.0..=1.0).contains(&c.survey_tag_detection_probability),
        "survey_tag_detection_probability {} out of range [0, 1]",
        c.survey_tag_detection_probability
    );
}

#[test]
fn solar_system_node_ids_are_non_empty() {
    let content = load_test_content();
    for node in &content.solar_system.nodes {
        assert!(!node.id.0.is_empty(), "solar system node has empty id");
    }
}

#[test]
fn solar_system_solar_intensities_are_non_negative() {
    let content = load_test_content();
    for node in &content.solar_system.nodes {
        assert!(
            node.solar_intensity >= 0.0,
            "node '{}' has negative solar_intensity: {}",
            node.id.0,
            node.solar_intensity
        );
    }
}

#[test]
fn no_duplicate_element_ids() {
    let content = load_test_content();
    let mut seen = HashSet::new();
    for element in &content.elements {
        assert!(
            seen.insert(&element.id),
            "duplicate element id: '{}'",
            element.id
        );
    }
}

#[test]
fn no_duplicate_tech_ids() {
    let content = load_test_content();
    let mut seen = HashSet::new();
    for tech in &content.techs {
        assert!(seen.insert(&tech.id), "duplicate tech id: '{}'", tech.id.0);
    }
}

#[test]
fn no_duplicate_node_ids() {
    let content = load_test_content();
    let mut seen = HashSet::new();
    for node in &content.solar_system.nodes {
        assert!(seen.insert(&node.id), "duplicate node id: '{}'", node.id.0);
    }
}

#[test]
fn no_duplicate_component_ids() {
    let content = load_test_content();
    let mut seen = HashSet::new();
    for component in &content.component_defs {
        assert!(
            seen.insert(&component.id),
            "duplicate component id: '{}'",
            component.id
        );
    }
}

// =========================================================================
// 3. Cross-reference integrity
// =========================================================================

#[test]
fn tech_prereqs_reference_known_techs() {
    let content = load_test_content();
    let tech_ids: HashSet<&TechId> = content.techs.iter().map(|t| &t.id).collect();
    for tech in &content.techs {
        for prereq in &tech.prereqs {
            assert!(
                tech_ids.contains(prereq),
                "tech '{}' prereq '{}' is not a known tech",
                tech.id.0,
                prereq.0
            );
        }
    }
}

#[test]
fn pricing_keys_reference_valid_items() {
    let content = load_test_content();
    let element_ids: HashSet<&str> = content.elements.iter().map(|e| e.id.as_str()).collect();
    let component_ids: HashSet<&str> = content
        .component_defs
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    let module_ids: HashSet<&str> = content.module_defs.keys().map(String::as_str).collect();

    for pricing_key in content.pricing.items.keys() {
        let is_known = element_ids.contains(pricing_key.as_str())
            || component_ids.contains(pricing_key.as_str())
            || module_ids.contains(pricing_key.as_str());
        assert!(
            is_known,
            "pricing key '{pricing_key}' does not match any element, component, or module id",
        );
    }
}

#[test]
fn assembler_component_inputs_reference_known_components() {
    let content = load_test_content();
    let component_ids: HashSet<&str> = content
        .component_defs
        .iter()
        .map(|c| c.id.as_str())
        .collect();

    for module_def in content.module_defs.values() {
        if let ModuleBehaviorDef::Assembler(assembler) = &module_def.behavior {
            for recipe in &assembler.recipes {
                for input in &recipe.inputs {
                    if let InputFilter::Component(ComponentId(component_id)) = &input.filter {
                        assert!(
                            component_ids.contains(component_id.as_str()),
                            "module '{}' recipe '{}' input component '{}' is not a known component",
                            module_def.id,
                            recipe.id,
                            component_id
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn assembler_component_outputs_reference_known_components() {
    let content = load_test_content();
    let component_ids: HashSet<&str> = content
        .component_defs
        .iter()
        .map(|c| c.id.as_str())
        .collect();

    for module_def in content.module_defs.values() {
        if let ModuleBehaviorDef::Assembler(assembler) = &module_def.behavior {
            for recipe in &assembler.recipes {
                for output in &recipe.outputs {
                    if let OutputSpec::Component { component_id, .. } = output {
                        assert!(
                            component_ids.contains(component_id.0.as_str()),
                            "module '{}' recipe '{}' output component '{}' is not a known component",
                            module_def.id,
                            recipe.id,
                            component_id.0
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn assembler_max_stock_keys_reference_known_components() {
    let content = load_test_content();
    let component_ids: HashSet<&str> = content
        .component_defs
        .iter()
        .map(|c| c.id.as_str())
        .collect();

    for module_def in content.module_defs.values() {
        if let ModuleBehaviorDef::Assembler(assembler) = &module_def.behavior {
            for stock_key in assembler.max_stock.keys() {
                assert!(
                    component_ids.contains(stock_key.0.as_str()),
                    "module '{}' max_stock key '{}' is not a known component",
                    module_def.id,
                    stock_key.0
                );
            }
        }
    }
}

#[test]
fn processor_recipe_element_inputs_reference_known_elements() {
    let content = load_test_content();
    let element_ids: HashSet<&str> = content.elements.iter().map(|e| e.id.as_str()).collect();

    for module_def in content.module_defs.values() {
        if let ModuleBehaviorDef::Processor(processor) = &module_def.behavior {
            for recipe in &processor.recipes {
                for input in &recipe.inputs {
                    match &input.filter {
                        InputFilter::Element(element_id) => {
                            assert!(
                                element_ids.contains(element_id.as_str()),
                                "module '{}' recipe '{}' input element '{}' is not a known element",
                                module_def.id,
                                recipe.id,
                                element_id
                            );
                        }
                        InputFilter::ElementWithMinQuality { element, .. } => {
                            assert!(
                                element_ids.contains(element.as_str()),
                                "module '{}' recipe '{}' input element '{}' is not a known element",
                                module_def.id,
                                recipe.id,
                                element
                            );
                        }
                        InputFilter::Component(ComponentId(component_id)) => {
                            let component_ids: HashSet<&str> = content
                                .component_defs
                                .iter()
                                .map(|c| c.id.as_str())
                                .collect();
                            assert!(
                                component_ids.contains(component_id.as_str()),
                                "module '{}' recipe '{}' input component '{}' is not a known component",
                                module_def.id,
                                recipe.id,
                                component_id
                            );
                        }
                        InputFilter::ItemKind(_) => {} // ItemKind is an enum, always valid
                    }
                }
            }
        }
    }
}

#[test]
fn solar_system_edges_reference_known_nodes() {
    let content = load_test_content();
    let node_ids: HashSet<&str> = content
        .solar_system
        .nodes
        .iter()
        .map(|n| n.id.0.as_str())
        .collect();

    for (from, to) in &content.solar_system.edges {
        assert!(
            node_ids.contains(from.0.as_str()),
            "edge references unknown node '{}'",
            from.0
        );
        assert!(
            node_ids.contains(to.0.as_str()),
            "edge references unknown node '{}'",
            to.0
        );
    }
}

#[test]
fn asteroid_template_compositions_reference_known_elements() {
    let content = load_test_content();
    let element_ids: HashSet<&str> = content.elements.iter().map(|e| e.id.as_str()).collect();

    for template in &content.asteroid_templates {
        for element_id in template.composition_ranges.keys() {
            assert!(
                element_ids.contains(element_id.as_str()),
                "template '{}' composition key '{}' is not a known element",
                template.id,
                element_id
            );
        }
    }
}

// =========================================================================
// 4. Content invariants — the game world is playable
// =========================================================================

#[test]
fn at_least_one_scannable_body_exists() {
    let content = load_test_content();
    assert!(
        !content.solar_system.nodes.is_empty(),
        "solar system has no nodes — game is unplayable"
    );
}

#[test]
fn at_least_one_asteroid_template_exists() {
    let content = load_test_content();
    assert!(
        !content.asteroid_templates.is_empty(),
        "no asteroid templates — mining is impossible"
    );
}

#[test]
fn at_least_one_mineable_ore_element_exists() {
    // "ore" is the hardcoded element ID that the mining system produces when ships mine asteroids.
    // Without this element, mined material has no type and the refinery pipeline breaks.
    let content = load_test_content();
    assert!(
        content.elements.iter().any(|e| e.id == "ore"),
        "no 'ore' element — mining produces nothing"
    );
}

#[test]
fn research_tree_has_reachable_techs() {
    // At least one tech must have no prerequisites (entry point to the tree)
    let content = load_test_content();
    let has_entry_point = content.techs.iter().any(|t| t.prereqs.is_empty());
    assert!(
        has_entry_point,
        "no tech has empty prereqs — research tree has no entry point"
    );
}

#[test]
fn no_circular_tech_dependencies() {
    let content = load_test_content();
    let tech_ids: HashSet<&str> = content.techs.iter().map(|t| t.id.0.as_str()).collect();

    // Build adjacency: tech -> prereqs
    let prereq_map: std::collections::HashMap<&str, Vec<&str>> = content
        .techs
        .iter()
        .map(|t| {
            (
                t.id.0.as_str(),
                t.prereqs.iter().map(|p| p.0.as_str()).collect(),
            )
        })
        .collect();

    // DFS cycle detection
    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();

    #[allow(clippy::items_after_statements)]
    fn has_cycle<'a>(
        node: &'a str,
        prereq_map: &std::collections::HashMap<&'a str, Vec<&'a str>>,
        visited: &mut HashSet<&'a str>,
        in_stack: &mut HashSet<&'a str>,
    ) -> bool {
        if in_stack.contains(node) {
            return true;
        }
        if visited.contains(node) {
            return false;
        }
        visited.insert(node);
        in_stack.insert(node);
        if let Some(prereqs) = prereq_map.get(node) {
            for prereq in prereqs {
                if has_cycle(prereq, prereq_map, visited, in_stack) {
                    return true;
                }
            }
        }
        in_stack.remove(node);
        false
    }

    for tech_id in &tech_ids {
        assert!(
            !has_cycle(tech_id, &prereq_map, &mut visited, &mut in_stack),
            "circular dependency detected involving tech '{tech_id}'"
        );
    }
}

#[test]
fn solar_system_graph_is_connected() {
    let content = load_test_content();
    let nodes: Vec<&str> = content
        .solar_system
        .nodes
        .iter()
        .map(|n| n.id.0.as_str())
        .collect();
    if nodes.is_empty() {
        return;
    }

    // Build adjacency list (undirected)
    let mut adjacency: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    for node in &nodes {
        adjacency.entry(node).or_default();
    }
    for (from, to) in &content.solar_system.edges {
        adjacency
            .entry(from.0.as_str())
            .or_default()
            .push(to.0.as_str());
        adjacency
            .entry(to.0.as_str())
            .or_default()
            .push(from.0.as_str());
    }

    // BFS from first node
    let mut visited = HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(nodes[0]);
    visited.insert(nodes[0]);
    while let Some(current) = queue.pop_front() {
        if let Some(neighbors) = adjacency.get(current) {
            for neighbor in neighbors {
                if visited.insert(neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
    }

    assert_eq!(
        visited.len(),
        nodes.len(),
        "solar system graph is not fully connected — {} of {} nodes reachable",
        visited.len(),
        nodes.len()
    );
}

#[test]
fn at_least_one_processor_module_exists() {
    let content = load_test_content();
    let has_processor = content
        .module_defs
        .values()
        .any(|m| matches!(&m.behavior, ModuleBehaviorDef::Processor(_)));
    assert!(
        has_processor,
        "no processor module — refining is impossible"
    );
}

#[test]
fn at_least_one_lab_module_exists() {
    let content = load_test_content();
    let has_lab = content
        .module_defs
        .values()
        .any(|m| matches!(&m.behavior, ModuleBehaviorDef::Lab(_)));
    assert!(has_lab, "no lab module — research is impossible");
}

#[test]
fn at_least_one_sensor_module_exists() {
    let content = load_test_content();
    let has_sensor = content
        .module_defs
        .values()
        .any(|m| matches!(&m.behavior, ModuleBehaviorDef::SensorArray(_)));
    assert!(
        has_sensor,
        "no sensor array module — data generation is impossible"
    );
}

#[test]
fn each_research_domain_has_a_lab() {
    use sim_core::ResearchDomain;
    let content = load_test_content();

    let domains_with_labs: HashSet<&ResearchDomain> = content
        .module_defs
        .values()
        .filter_map(|m| {
            if let ModuleBehaviorDef::Lab(lab) = &m.behavior {
                Some(&lab.domain)
            } else {
                None
            }
        })
        .collect();

    // Check that all domains used in tech requirements have labs
    for tech in &content.techs {
        for domain in tech.domain_requirements.keys() {
            assert!(
                domains_with_labs.contains(domain),
                "tech '{}' requires domain {:?} but no lab module covers it",
                tech.id.0,
                domain
            );
        }
    }
}

#[test]
fn deep_scan_tech_exists_if_deep_scan_effect_is_used() {
    let content = load_test_content();
    let has_deep_scan_tech = content.techs.iter().any(|t| {
        t.effects
            .iter()
            .any(|e| matches!(e, TechEffect::EnableDeepScan))
    });
    // Deep scan is a core gameplay mechanic — at least one tech should enable it
    assert!(
        has_deep_scan_tech,
        "no tech has EnableDeepScan effect — deep scanning is locked forever"
    );
}

#[test]
fn processor_recipe_efficiencies_are_valid() {
    let content = load_test_content();
    for module_def in content.module_defs.values() {
        if let ModuleBehaviorDef::Processor(processor) = &module_def.behavior {
            for recipe in &processor.recipes {
                assert!(
                    recipe.efficiency > 0.0 && recipe.efficiency <= 1.0,
                    "module '{}' recipe '{}' has invalid efficiency: {} (expected 0 < e <= 1)",
                    module_def.id,
                    recipe.id,
                    recipe.efficiency
                );
            }
        }
    }
}

#[test]
fn assembler_recipe_efficiencies_are_valid() {
    let content = load_test_content();
    for module_def in content.module_defs.values() {
        if let ModuleBehaviorDef::Assembler(assembler) = &module_def.behavior {
            for recipe in &assembler.recipes {
                assert!(
                    recipe.efficiency > 0.0 && recipe.efficiency <= 1.0,
                    "module '{}' recipe '{}' has invalid efficiency: {} (expected 0 < e <= 1)",
                    module_def.id,
                    recipe.id,
                    recipe.efficiency
                );
            }
        }
    }
}

#[test]
fn processor_intervals_are_positive() {
    let content = load_test_content();
    for module_def in content.module_defs.values() {
        if let ModuleBehaviorDef::Processor(processor) = &module_def.behavior {
            assert!(
                processor.processing_interval_minutes > 0,
                "module '{}' has zero processing_interval_minutes",
                module_def.id
            );
        }
    }
}

#[test]
fn assembler_intervals_are_positive() {
    let content = load_test_content();
    for module_def in content.module_defs.values() {
        if let ModuleBehaviorDef::Assembler(assembler) = &module_def.behavior {
            assert!(
                assembler.assembly_interval_minutes > 0,
                "module '{}' has zero assembly_interval_minutes",
                module_def.id
            );
        }
    }
}

#[test]
fn lab_intervals_are_positive() {
    let content = load_test_content();
    for module_def in content.module_defs.values() {
        if let ModuleBehaviorDef::Lab(lab) = &module_def.behavior {
            assert!(
                lab.research_interval_minutes > 0,
                "module '{}' has zero research_interval_minutes",
                module_def.id
            );
        }
    }
}

#[test]
fn sensor_intervals_are_positive() {
    let content = load_test_content();
    for module_def in content.module_defs.values() {
        if let ModuleBehaviorDef::SensorArray(sensor) = &module_def.behavior {
            assert!(
                sensor.scan_interval_minutes > 0,
                "module '{}' has zero scan_interval_minutes",
                module_def.id
            );
        }
    }
}

#[test]
fn maintenance_intervals_are_positive() {
    let content = load_test_content();
    for module_def in content.module_defs.values() {
        if let ModuleBehaviorDef::Maintenance(maint) = &module_def.behavior {
            assert!(
                maint.repair_interval_minutes > 0,
                "module '{}' has zero repair_interval_minutes",
                module_def.id
            );
        }
    }
}

// =========================================================================
// 5. Balance sanity checks
// =========================================================================

#[test]
fn no_extreme_pricing_outliers_within_category() {
    // Compare prices within categories: raw materials (elements) vs manufactured (modules/components).
    // Cross-category price differences are expected (ore $5 vs module $10M is normal).
    let content = load_test_content();
    let element_ids: HashSet<&str> = content.elements.iter().map(|e| e.id.as_str()).collect();
    let module_ids: HashSet<&str> = content.module_defs.keys().map(String::as_str).collect();

    let mut element_prices = Vec::new();
    let mut module_prices = Vec::new();
    let mut component_prices = Vec::new();

    for (item_id, entry) in &content.pricing.items {
        if element_ids.contains(item_id.as_str()) {
            element_prices.push((item_id.as_str(), entry.base_price_per_unit));
        } else if module_ids.contains(item_id.as_str()) {
            module_prices.push((item_id.as_str(), entry.base_price_per_unit));
        } else {
            component_prices.push((item_id.as_str(), entry.base_price_per_unit));
        }
    }

    #[allow(clippy::items_after_statements)]
    fn check_category(category: &str, prices: &[(&str, f64)]) {
        if prices.len() < 2 {
            return;
        }
        let mut values: Vec<f64> = prices.iter().map(|(_, p)| *p).collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let min = values[0];
        let max = values[values.len() - 1];
        // Within a category, a 10,000x spread is suspicious
        assert!(
            max / min < 10_000.0,
            "{category} category has extreme price spread: min={min:.2}, max={max:.2} ({:.0}x)",
            max / min,
        );
    }

    check_category("element", &element_prices);
    check_category("module", &module_prices);
    check_category("component", &component_prices);
}

#[test]
fn lab_data_consumption_is_positive() {
    let content = load_test_content();
    for module_def in content.module_defs.values() {
        if let ModuleBehaviorDef::Lab(lab) = &module_def.behavior {
            assert!(
                lab.data_consumption_per_run > 0.0,
                "module '{}' lab has zero data_consumption_per_run",
                module_def.id
            );
            assert!(
                lab.research_points_per_run > 0.0,
                "module '{}' lab has zero research_points_per_run",
                module_def.id
            );
        }
    }
}

#[test]
fn solar_array_output_is_positive() {
    let content = load_test_content();
    for module_def in content.module_defs.values() {
        if let ModuleBehaviorDef::SolarArray(solar) = &module_def.behavior {
            assert!(
                solar.base_output_kw > 0.0,
                "module '{}' solar array has zero base_output_kw",
                module_def.id
            );
        }
    }
}

#[test]
fn battery_values_are_positive() {
    let content = load_test_content();
    for module_def in content.module_defs.values() {
        if let ModuleBehaviorDef::Battery(battery) = &module_def.behavior {
            assert!(
                battery.capacity_kwh > 0.0,
                "module '{}' battery has zero capacity_kwh",
                module_def.id
            );
            assert!(
                battery.charge_rate_kw > 0.0,
                "module '{}' battery has zero charge_rate_kw",
                module_def.id
            );
            assert!(
                battery.discharge_rate_kw > 0.0,
                "module '{}' battery has zero discharge_rate_kw",
                module_def.id
            );
        }
    }
}

#[test]
fn asteroid_template_composition_ranges_are_valid() {
    let content = load_test_content();
    for template in &content.asteroid_templates {
        for (element_id, (min, max)) in &template.composition_ranges {
            assert!(
                *min >= 0.0,
                "template '{}' element '{}' has negative min: {}",
                template.id,
                element_id,
                min
            );
            assert!(
                *max <= 1.0,
                "template '{}' element '{}' has max > 1.0: {}",
                template.id,
                element_id,
                max
            );
            assert!(
                min <= max,
                "template '{}' element '{}' has min ({}) > max ({})",
                template.id,
                element_id,
                min,
                max
            );
        }
    }
}

#[test]
fn wear_thresholds_are_ordered() {
    let content = load_test_content();
    let c = &content.constants;
    assert!(
        c.wear_band_degraded_threshold < c.wear_band_critical_threshold,
        "degraded threshold ({}) >= critical threshold ({})",
        c.wear_band_degraded_threshold,
        c.wear_band_critical_threshold
    );
    assert!(
        c.wear_band_degraded_threshold > 0.0,
        "degraded threshold must be > 0"
    );
    assert!(
        c.wear_band_critical_threshold < 1.0,
        "critical threshold must be < 1.0"
    );
}

#[test]
fn wear_efficiencies_are_valid() {
    let content = load_test_content();
    let c = &content.constants;
    assert!(
        c.wear_band_degraded_efficiency > 0.0 && c.wear_band_degraded_efficiency <= 1.0,
        "degraded efficiency {} out of range (0, 1]",
        c.wear_band_degraded_efficiency
    );
    assert!(
        c.wear_band_critical_efficiency > 0.0 && c.wear_band_critical_efficiency <= 1.0,
        "critical efficiency {} out of range (0, 1]",
        c.wear_band_critical_efficiency
    );
    assert!(
        c.wear_band_degraded_efficiency >= c.wear_band_critical_efficiency,
        "degraded efficiency ({}) < critical efficiency ({})",
        c.wear_band_degraded_efficiency,
        c.wear_band_critical_efficiency
    );
}

#[test]
fn maintenance_repair_threshold_is_valid() {
    let content = load_test_content();
    for module_def in content.module_defs.values() {
        if let ModuleBehaviorDef::Maintenance(maint) = &module_def.behavior {
            assert!(
                (0.0..=1.0).contains(&maint.repair_threshold),
                "module '{}' repair_threshold {} out of range [0, 1]",
                module_def.id,
                maint.repair_threshold
            );
        }
    }
}
