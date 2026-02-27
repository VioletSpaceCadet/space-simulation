//! World generation and content loading shared between `sim_cli` and `sim_daemon`.

use anyhow::{Context, Result};
use rand::Rng;
use serde::Deserialize;
use sim_core::{
    AsteroidTemplateDef, ComponentId, Constants, Counters, ElementDef, GameContent, GameState,
    InputFilter, InventoryItem, MetaState, ModuleBehaviorDef, ModuleDef, ModuleItemId, NodeId,
    OutputSpec, PowerState, PricingTable, PrincipalId, QualityFormula, ResearchState, ScanSite,
    ShipId, ShipState, SiteId, SolarSystemDef, StationId, StationState, TechDef, TechId,
    YieldFormula,
};
use std::collections::{HashMap, HashSet};
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
#[allow(clippy::too_many_lines)]
pub fn validate_content(content: &GameContent) {
    let element_ids: HashSet<&str> = content.elements.iter().map(|e| e.id.as_str()).collect();
    assert!(
        element_ids.contains("ore"),
        "required element 'ore' is missing from content.elements"
    );
    assert!(
        element_ids.contains("slag"),
        "required element 'slag' is missing from content.elements"
    );
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
    for module_def in content.module_defs.values() {
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
                        OutputSpec::Slag { .. }
                        | OutputSpec::Component { .. }
                        | OutputSpec::Ship { .. } => {}
                    }
                }
            }
        }

        if let ModuleBehaviorDef::Assembler(assembler) = &module_def.behavior {
            for recipe in &assembler.recipes {
                for input in &recipe.inputs {
                    if let InputFilter::Element(element_id) = &input.filter {
                        assert!(
                            element_ids.contains(element_id.as_str()),
                            "module '{}' assembler recipe '{}' input element '{}' is not a known element",
                            module_def.id, recipe.id, element_id,
                        );
                    }
                }
            }
        }
    }
}

pub fn validate_state(state: &GameState, content: &GameContent) {
    let element_ids: HashSet<&str> = content.elements.iter().map(|e| e.id.as_str()).collect();
    for station in state.stations.values() {
        for item in &station.inventory {
            if let InventoryItem::Material { element, .. } = item {
                assert!(
                    element_ids.contains(element.as_str()),
                    "station '{}' inventory material element '{}' is not a known element",
                    station.id.0,
                    element
                );
            }
        }
    }
    for ship in state.ships.values() {
        for item in &ship.inventory {
            if let InventoryItem::Material { element, .. } = item {
                assert!(
                    element_ids.contains(element.as_str()),
                    "ship '{}' inventory material element '{}' is not a known element",
                    ship.id.0,
                    element
                );
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
    let module_defs: HashMap<String, ModuleDef> = {
        let defs: Vec<ModuleDef> = serde_json::from_str(
            &std::fs::read_to_string(dir.join("module_defs.json"))
                .context("reading module_defs.json")?,
        )
        .context("parsing module_defs.json")?;
        defs.into_iter().map(|d| (d.id.clone(), d)).collect()
    };
    let component_defs: Vec<sim_core::ComponentDef> = serde_json::from_str(
        &std::fs::read_to_string(dir.join("component_defs.json"))
            .context("reading component_defs.json")?,
    )
    .context("parsing component_defs.json")?;
    let pricing: PricingTable = serde_json::from_str(
        &std::fs::read_to_string(dir.join("pricing.json")).context("reading pricing.json")?,
    )
    .context("parsing pricing.json")?;
    let mut content = GameContent {
        content_version: techs_file.content_version,
        techs: techs_file.techs,
        solar_system,
        asteroid_templates: templates_file.templates,
        elements: elements_file.elements,
        module_defs,
        component_defs,
        pricing,
        constants,
        density_map: std::collections::HashMap::new(),
    };
    content.init_caches();
    validate_content(&content);
    Ok(content)
}

#[allow(clippy::too_many_lines)]
pub fn build_initial_state(content: &GameContent, seed: u64, rng: &mut impl Rng) -> GameState {
    let earth_orbit = NodeId("node_earth_orbit".to_string());
    let c = &content.constants;
    let station_id = StationId("station_earth_orbit".to_string());
    let station = StationState {
        id: station_id.clone(),
        location_node: earth_orbit.clone(),
        inventory: vec![
            InventoryItem::Module {
                item_id: ModuleItemId("module_item_0001".to_string()),
                module_def_id: "module_basic_iron_refinery".to_string(),
            },
            InventoryItem::Component {
                component_id: ComponentId("repair_kit".to_string()),
                count: 10,
                quality: 1.0,
            },
            InventoryItem::Module {
                item_id: ModuleItemId("module_item_0002".to_string()),
                module_def_id: "module_maintenance_bay".to_string(),
            },
            InventoryItem::Module {
                item_id: ModuleItemId("module_item_0003".to_string()),
                module_def_id: "module_basic_assembler".to_string(),
            },
            InventoryItem::Module {
                item_id: ModuleItemId("module_item_0004".to_string()),
                module_def_id: "module_exploration_lab".to_string(),
            },
            InventoryItem::Module {
                item_id: ModuleItemId("module_item_0005".to_string()),
                module_def_id: "module_materials_lab".to_string(),
            },
            InventoryItem::Module {
                item_id: ModuleItemId("module_item_0006".to_string()),
                module_def_id: "module_engineering_lab".to_string(),
            },
            InventoryItem::Module {
                item_id: ModuleItemId("module_item_0007".to_string()),
                module_def_id: "module_sensor_array".to_string(),
            },
            InventoryItem::Module {
                item_id: ModuleItemId("module_item_0008".to_string()),
                module_def_id: "module_shipyard".to_string(),
            },
            InventoryItem::Module {
                item_id: ModuleItemId("module_item_0009".to_string()),
                module_def_id: "module_basic_solar_array".to_string(),
            },
            InventoryItem::Module {
                item_id: ModuleItemId("module_item_0010".to_string()),
                module_def_id: "module_basic_solar_array".to_string(),
            },
            InventoryItem::Module {
                item_id: ModuleItemId("module_item_0011".to_string()),
                module_def_id: "module_basic_battery".to_string(),
            },
            InventoryItem::Material {
                element: "Fe".to_string(),
                kg: 500.0,
                quality: 0.7,
            },
        ],
        cargo_capacity_m3: c.station_cargo_capacity_m3,
        power_available_per_tick: c.station_power_available_per_tick,
        modules: vec![],
        power: PowerState::default(),
        cached_inventory_volume_m3: None,
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
            action_counts: std::collections::HashMap::new(),
        },
        balance: 1_000_000_000.0,
        counters: Counters {
            next_event_id: 0,
            next_command_id: 0,
            next_asteroid_id: 0,
            next_lot_id: 0,
            next_module_instance_id: 0,
        },
    }
}

// ---------------------------------------------------------------------------
// Run directory utilities
// ---------------------------------------------------------------------------

/// Generates a timestamped run ID like `20260218_143022_seed42`.
pub fn generate_run_id(seed: u64) -> String {
    let now = chrono::Utc::now();
    now.format(&format!("%Y%m%d_%H%M%S_seed{seed}")).to_string()
}

/// Creates the `runs/<run_id>/` directory tree, returning the path.
pub fn create_run_dir(run_id: &str) -> Result<std::path::PathBuf> {
    let dir = std::path::PathBuf::from("runs").join(run_id);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating run directory: {}", dir.display()))?;
    Ok(dir)
}

/// Writes `run_info.json` into the run directory.
///
/// `runner_args` is an arbitrary JSON value containing runner-specific CLI arguments.
#[allow(clippy::needless_pass_by_value)]
pub fn write_run_info(
    dir: &std::path::Path,
    run_id: &str,
    seed: u64,
    content_version: &str,
    metrics_every: u64,
    runner_args: serde_json::Value,
) -> Result<()> {
    let info = serde_json::json!({
        "run_id": run_id,
        "seed": seed,
        "content_version": content_version,
        "metrics_every": metrics_every,
        "args": runner_args,
    });
    let path = dir.join("run_info.json");
    let file =
        std::fs::File::create(&path).with_context(|| format!("creating {}", path.display()))?;
    serde_json::to_writer_pretty(file, &info)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Loads state from a JSON file or builds initial state from content.
///
/// Returns the game state and a seeded RNG.
pub fn load_or_build_state(
    content: &GameContent,
    seed: Option<u64>,
    state_file: Option<&str>,
) -> Result<(GameState, rand_chacha::ChaCha8Rng)> {
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    if let Some(path) = state_file {
        let json =
            std::fs::read_to_string(path).with_context(|| format!("reading state file: {path}"))?;
        let loaded: GameState =
            serde_json::from_str(&json).with_context(|| format!("parsing state file: {path}"))?;
        let rng = ChaCha8Rng::seed_from_u64(loaded.meta.seed);
        validate_state(&loaded, content);
        Ok((loaded, rng))
    } else {
        let resolved_seed = seed.unwrap_or_else(rand::random);
        let mut rng = ChaCha8Rng::seed_from_u64(resolved_seed);
        let state = build_initial_state(content, resolved_seed, &mut rng);
        Ok((state, rng))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use sim_core::{
        test_fixtures::{base_content, minimal_content},
        AssemblerDef, AsteroidTemplateDef, Counters, GameState, InputAmount, InputFilter,
        InventoryItem, ItemKind, MetaState, ModuleBehaviorDef, ModuleDef, NodeDef, NodeId,
        OutputSpec, ProcessorDef, QualityFormula, RecipeDef, RecipeInput, ResearchState, StationId,
        StationState, TechDef, TechId, YieldFormula,
    };
    use std::collections::HashMap;

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
            domain_requirements: HashMap::new(),
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
            solar_intensity: 1.0,
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
        content.module_defs.insert(
            "mod_test".to_string(),
            ModuleDef {
                id: "mod_test".to_string(),
                name: "Test Module".to_string(),
                mass_kg: 1000.0,
                volume_m3: 5.0,
                power_consumption_per_run: 10.0,
                wear_per_run: 0.0,
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
            },
        );
        validate_content(&content);
    }

    #[test]
    fn test_build_initial_state_determinism() {
        let content = base_content();
        let mut rng1 = ChaCha8Rng::seed_from_u64(42);
        let mut rng2 = ChaCha8Rng::seed_from_u64(42);
        let state1 = build_initial_state(&content, 42, &mut rng1);
        let state2 = build_initial_state(&content, 42, &mut rng2);
        assert_eq!(
            serde_json::to_string(&state1).unwrap(),
            serde_json::to_string(&state2).unwrap()
        );
    }

    #[test]
    fn test_load_content_missing_file() {
        let result = load_content("/tmp/nonexistent_dir_12345");
        assert!(result.is_err());
    }

    #[test]
    fn test_build_initial_state_has_ship_and_station() {
        let content = base_content();
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let state = build_initial_state(&content, 42, &mut rng);
        assert_eq!(state.ships.len(), 1, "expected exactly 1 ship");
        assert_eq!(state.stations.len(), 1, "expected exactly 1 station");
        let ship = state.ships.values().next().unwrap();
        let station = state.stations.values().next().unwrap();
        assert_eq!(
            ship.location_node, station.location_node,
            "ship and station should be at the same node"
        );
    }

    #[test]
    #[should_panic(expected = "required element 'ore' is missing")]
    fn test_missing_ore_element_panics() {
        let mut content = minimal_content();
        content.elements.retain(|e| e.id != "ore");
        validate_content(&content);
    }

    #[test]
    #[should_panic(expected = "required element 'slag' is missing")]
    fn test_missing_slag_element_panics() {
        let mut content = minimal_content();
        content.elements.retain(|e| e.id != "slag");
        validate_content(&content);
    }

    #[test]
    #[should_panic(expected = "not a known element")]
    fn test_assembler_recipe_unknown_element_panics() {
        let mut content = minimal_content();
        content.module_defs.insert(
            "mod_assembler_test".to_string(),
            ModuleDef {
                id: "mod_assembler_test".to_string(),
                name: "Test Assembler".to_string(),
                mass_kg: 1000.0,
                volume_m3: 5.0,
                power_consumption_per_run: 10.0,
                wear_per_run: 0.0,
                behavior: ModuleBehaviorDef::Assembler(AssemblerDef {
                    assembly_interval_ticks: 10,
                    max_stock: std::collections::HashMap::new(),
                    recipes: vec![RecipeDef {
                        id: "recipe_asm_test".to_string(),
                        inputs: vec![RecipeInput {
                            filter: InputFilter::Element("Unobtanium".to_string()),
                            amount: InputAmount::Kg(50.0),
                        }],
                        outputs: vec![],
                        efficiency: 1.0,
                    }],
                }),
            },
        );
        validate_content(&content);
    }

    #[test]
    #[should_panic(expected = "not a known element")]
    fn test_state_with_unknown_material_element_panics() {
        let content = minimal_content();
        let station_id = StationId("station_test".to_string());
        let state = GameState {
            meta: MetaState {
                tick: 0,
                seed: 42,
                schema_version: 1,
                content_version: "test".to_string(),
            },
            scan_sites: vec![],
            asteroids: HashMap::new(),
            ships: HashMap::new(),
            stations: HashMap::from([(
                station_id.clone(),
                StationState {
                    id: station_id,
                    location_node: NodeId("node_test".to_string()),
                    inventory: vec![InventoryItem::Material {
                        element: "Unobtanium".to_string(),
                        kg: 100.0,
                        quality: 1.0,
                    }],
                    cargo_capacity_m3: 1000.0,
                    power_available_per_tick: 100.0,
                    modules: vec![],
                    power: PowerState::default(),
                    cached_inventory_volume_m3: None,
                },
            )]),
            research: ResearchState {
                unlocked: std::collections::HashSet::new(),
                data_pool: HashMap::new(),
                evidence: HashMap::new(),
                action_counts: HashMap::new(),
            },
            balance: 0.0,
            counters: Counters {
                next_event_id: 0,
                next_command_id: 0,
                next_asteroid_id: 0,
                next_lot_id: 0,
                next_module_instance_id: 0,
            },
        };
        validate_state(&state, &content);
    }
}
