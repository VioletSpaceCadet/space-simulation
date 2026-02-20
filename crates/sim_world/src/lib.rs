//! World generation and content loading shared between sim_cli and sim_daemon.

use anyhow::{Context, Result};
use rand::Rng;
use serde::Deserialize;
use sim_core::{
    AsteroidTemplateDef, Constants, Counters, ElementDef, FacilitiesState, GameContent, GameState,
    InputFilter, MetaState, ModuleBehaviorDef, ModuleDef, NodeId, OutputSpec, PrincipalId,
    QualityFormula, ResearchState, ScanSite, ShipId, ShipState, SiteId, SolarSystemDef, StationId,
    StationState, TechDef, TechId, YieldFormula,
};
use std::collections::HashSet;
use std::path::Path;

#[derive(Deserialize)]
struct TechsFile {
    content_version: String,
    techs: Vec<TechDef>,
}

#[derive(Deserialize)]
struct AsteroidTemplatesFile {
    templates: Vec<AsteroidTemplateDef>,
}

#[derive(Deserialize)]
struct ElementsFile {
    elements: Vec<ElementDef>,
}

/// Validates cross-references in loaded content, panicking on any authoring error.
///
/// Catches mistakes like: referencing an unknown element in a recipe, a tech
/// prereq that doesn't exist, or a solar-system edge pointing at an unknown node.
pub fn validate_content(content: &GameContent) {
    let element_ids: HashSet<&str> = content.elements.iter().map(|e| e.id.as_str()).collect();
    let tech_ids: HashSet<&TechId> = content.techs.iter().map(|t| &t.id).collect();
    let node_ids: HashSet<&str> = content
        .solar_system
        .nodes
        .iter()
        .map(|n| n.id.0.as_str())
        .collect();

    // Validate tech prereqs.
    for tech in &content.techs {
        for prereq in &tech.prereqs {
            assert!(
                tech_ids.contains(prereq),
                "tech '{}' prereq '{}' is not a known tech id",
                tech.id.0,
                prereq.0,
            );
        }
    }

    // Validate solar system edges reference known nodes.
    for (from, to) in &content.solar_system.edges {
        assert!(
            node_ids.contains(from.0.as_str()),
            "solar system edge references unknown node '{}'",
            from.0,
        );
        assert!(
            node_ids.contains(to.0.as_str()),
            "solar system edge references unknown node '{}'",
            to.0,
        );
    }

    // Validate asteroid template composition element IDs.
    for template in &content.asteroid_templates {
        for element_id in template.composition_ranges.keys() {
            assert!(
                element_ids.contains(element_id.as_str()),
                "asteroid template '{}' composition key '{}' is not a known element",
                template.id,
                element_id,
            );
        }
    }

    // Validate module recipe element references.
    for module_def in &content.module_defs {
        if let ModuleBehaviorDef::Processor(processor) = &module_def.behavior {
            for recipe in &processor.recipes {
                // Validate inputs.
                for input in &recipe.inputs {
                    if let InputFilter::Element(element_id) = &input.filter {
                        assert!(
                            element_ids.contains(element_id.as_str()),
                            "module '{}' recipe '{}' input element '{}' is not a known element",
                            module_def.id,
                            recipe.id,
                            element_id,
                        );
                    }
                }
                // Validate outputs.
                for output in &recipe.outputs {
                    match output {
                        OutputSpec::Material {
                            element,
                            yield_formula,
                            quality_formula,
                        } => {
                            assert!(
                                element_ids.contains(element.as_str()),
                                "module '{}' recipe '{}' output element '{}' is not a known element",
                                module_def.id,
                                recipe.id,
                                element,
                            );
                            if let YieldFormula::ElementFraction { element: fe } = yield_formula {
                                assert!(
                                    element_ids.contains(fe.as_str()),
                                    "module '{}' recipe '{}' YieldFormula element '{}' is not a known element",
                                    module_def.id,
                                    recipe.id,
                                    fe,
                                );
                            }
                            if let QualityFormula::ElementFractionTimesMultiplier {
                                element: fe,
                                ..
                            } = quality_formula
                            {
                                assert!(
                                    element_ids.contains(fe.as_str()),
                                    "module '{}' recipe '{}' QualityFormula element '{}' is not a known element",
                                    module_def.id,
                                    recipe.id,
                                    fe,
                                );
                            }
                        }
                        OutputSpec::Slag { .. } | OutputSpec::Component { .. } => {}
                    }
                }
            }
        }
    }
}

pub fn load_content(content_dir: &str) -> Result<GameContent> {
    let dir = Path::new(content_dir);
    let constants: Constants = serde_json::from_str(
        &std::fs::read_to_string(dir.join("constants.json")).context("reading constants.json")?,
    )
    .context("parsing constants.json")?;
    let techs_file: TechsFile = serde_json::from_str(
        &std::fs::read_to_string(dir.join("techs.json")).context("reading techs.json")?,
    )
    .context("parsing techs.json")?;
    let solar_system: SolarSystemDef = serde_json::from_str(
        &std::fs::read_to_string(dir.join("solar_system.json"))
            .context("reading solar_system.json")?,
    )
    .context("parsing solar_system.json")?;
    let templates_file: AsteroidTemplatesFile = serde_json::from_str(
        &std::fs::read_to_string(dir.join("asteroid_templates.json"))
            .context("reading asteroid_templates.json")?,
    )
    .context("parsing asteroid_templates.json")?;
    let elements_file: ElementsFile = serde_json::from_str(
        &std::fs::read_to_string(dir.join("elements.json")).context("reading elements.json")?,
    )
    .context("parsing elements.json")?;
    let module_defs: Vec<ModuleDef> = serde_json::from_str(
        &std::fs::read_to_string(dir.join("module_defs.json"))
            .context("reading module_defs.json")?,
    )
    .context("parsing module_defs.json")?;
    let content = GameContent {
        content_version: techs_file.content_version,
        techs: techs_file.techs,
        solar_system,
        asteroid_templates: templates_file.templates,
        elements: elements_file.elements,
        module_defs,
        constants,
    };
    validate_content(&content);
    Ok(content)
}

pub fn build_initial_state(content: &GameContent, seed: u64, rng: &mut impl Rng) -> GameState {
    let earth_orbit = NodeId("node_earth_orbit".to_string());
    let c = &content.constants;
    let station_id = StationId("station_earth_orbit".to_string());
    let station = StationState {
        id: station_id.clone(),
        location_node: earth_orbit.clone(),
        inventory: vec![],
        cargo_capacity_m3: c.station_cargo_capacity_m3,
        power_available_per_tick: c.station_power_available_per_tick,
        facilities: FacilitiesState {
            compute_units_total: c.station_compute_units_total,
            power_per_compute_unit_per_tick: c.station_power_per_compute_unit_per_tick,
            efficiency: c.station_efficiency,
        },
        modules: vec![],
    };
    let ship_id = ShipId("ship_0001".to_string());
    let owner = PrincipalId("principal_autopilot".to_string());
    let ship = ShipState {
        id: ship_id.clone(),
        location_node: earth_orbit.clone(),
        owner,
        inventory: vec![],
        cargo_capacity_m3: c.ship_cargo_capacity_m3,
        task: None,
    };
    let node_ids: Vec<&NodeId> = content.solar_system.nodes.iter().map(|n| &n.id).collect();
    let mut scan_sites = Vec::new();
    for template in &content.asteroid_templates {
        for _ in 0..c.asteroid_count_per_template {
            let node = node_ids[rng.gen_range(0..node_ids.len())].clone();
            let uuid = sim_core::generate_uuid(rng);
            scan_sites.push(ScanSite {
                id: SiteId(format!("site_{uuid}")),
                node,
                template_id: template.id.clone(),
            });
        }
    }
    GameState {
        meta: MetaState {
            tick: 0,
            seed,
            schema_version: 1,
            content_version: content.content_version.clone(),
        },
        scan_sites,
        asteroids: std::collections::HashMap::new(),
        ships: std::collections::HashMap::from([(ship_id, ship)]),
        stations: std::collections::HashMap::from([(station_id, station)]),
        research: ResearchState {
            unlocked: std::collections::HashSet::new(),
            data_pool: std::collections::HashMap::new(),
            evidence: std::collections::HashMap::new(),
        },
        counters: Counters {
            next_event_id: 0,
            next_command_id: 0,
            next_asteroid_id: 0,
            next_lot_id: 0,
            next_module_instance_id: 0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::{
        AsteroidTemplateDef, Constants, ElementDef, GameContent, InputAmount, InputFilter,
        ItemKind, ModuleBehaviorDef, ModuleDef, NodeDef, NodeId, OutputSpec, ProcessorDef,
        QualityFormula, RecipeDef, RecipeInput, SolarSystemDef, TechDef, TechId, YieldFormula,
    };
    use std::collections::HashMap;

    fn minimal_content() -> GameContent {
        GameContent {
            content_version: "test".to_string(),
            techs: vec![],
            solar_system: SolarSystemDef {
                nodes: vec![],
                edges: vec![],
            },
            asteroid_templates: vec![],
            elements: vec![ElementDef {
                id: "Fe".to_string(),
                density_kg_per_m3: 7874.0,
                display_name: "Iron".to_string(),
                refined_name: None,
            }],
            module_defs: vec![],
            constants: Constants {
                survey_scan_ticks: 1,
                deep_scan_ticks: 1,
                travel_ticks_per_hop: 1,
                survey_scan_data_amount: 1.0,
                survey_scan_data_quality: 1.0,
                deep_scan_data_amount: 1.0,
                deep_scan_data_quality: 1.0,
                survey_tag_detection_probability: 1.0,
                asteroid_count_per_template: 0,
                station_compute_units_total: 0,
                station_power_per_compute_unit_per_tick: 0.0,
                station_efficiency: 1.0,
                station_power_available_per_tick: 0.0,
                asteroid_mass_min_kg: 100.0,
                asteroid_mass_max_kg: 100.0,
                ship_cargo_capacity_m3: 20.0,
                station_cargo_capacity_m3: 1000.0,
                mining_rate_kg_per_tick: 50.0,
                deposit_ticks: 1,
                autopilot_iron_rich_confidence_threshold: 0.7,
                autopilot_refinery_threshold_kg: 500.0,
            },
        }
    }

    #[test]
    fn test_valid_content_passes_validation() {
        let content = minimal_content();
        validate_content(&content); // should not panic
    }

    #[test]
    #[should_panic(expected = "is not a known tech id")]
    fn test_tech_prereq_unknown_panics() {
        let mut content = minimal_content();
        content.techs.push(TechDef {
            id: TechId("tech_a".to_string()),
            name: "A".to_string(),
            prereqs: vec![TechId("tech_nonexistent".to_string())],
            accepted_data: vec![],
            difficulty: 1.0,
            effects: vec![],
        });
        validate_content(&content);
    }

    #[test]
    #[should_panic(expected = "unknown node")]
    fn test_solar_system_edge_unknown_node_panics() {
        let mut content = minimal_content();
        content.solar_system.nodes.push(NodeDef {
            id: NodeId("node_a".to_string()),
            name: "A".to_string(),
        });
        content.solar_system.edges.push((
            NodeId("node_a".to_string()),
            NodeId("node_missing".to_string()),
        ));
        validate_content(&content);
    }

    #[test]
    #[should_panic(expected = "not a known element")]
    fn test_asteroid_template_unknown_element_panics() {
        let mut content = minimal_content();
        content.asteroid_templates.push(AsteroidTemplateDef {
            id: "tmpl_test".to_string(),
            anomaly_tags: vec![],
            composition_ranges: HashMap::from([("NoSuchElement".to_string(), (0.5_f32, 0.5_f32))]),
        });
        validate_content(&content);
    }

    #[test]
    #[should_panic(expected = "not a known element")]
    fn test_recipe_output_unknown_element_panics() {
        let mut content = minimal_content();
        content.module_defs.push(ModuleDef {
            id: "mod_test".to_string(),
            name: "Test Module".to_string(),
            mass_kg: 1000.0,
            volume_m3: 5.0,
            power_consumption_per_run: 10.0,
            behavior: ModuleBehaviorDef::Processor(ProcessorDef {
                processing_interval_ticks: 10,
                recipes: vec![RecipeDef {
                    id: "recipe_test".to_string(),
                    inputs: vec![RecipeInput {
                        filter: InputFilter::ItemKind(ItemKind::Ore),
                        amount: InputAmount::Kg(100.0),
                    }],
                    outputs: vec![OutputSpec::Material {
                        element: "Ghost".to_string(), // does not exist
                        yield_formula: YieldFormula::FixedFraction(1.0),
                        quality_formula: QualityFormula::Fixed(1.0),
                    }],
                    efficiency: 1.0,
                }],
            }),
        });
        validate_content(&content);
    }
}
