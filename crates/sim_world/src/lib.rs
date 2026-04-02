//! World generation and content loading shared between `sim_cli` and `sim_daemon`.

use anyhow::{Context, Result};
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::Deserialize;
use sim_core::{
    AHashMap, AlertRuleDef, AsteroidTemplateDef, ComponentId, Constants, Counters, ElementDef,
    GameContent, GameState, InputFilter, InventoryItem, MetaState, MetricsFileWriter,
    ModuleBehaviorDef, ModuleDef, ModuleItemId, OutputSpec, PowerState, PricingTable, PrincipalId,
    QualityFormula, ResearchState, ScanSite, ShipId, ShipState, SiteId, SolarSystemDef, StationId,
    StationState, TechDef, TechId, YieldFormula,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

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
    validate_constants(content);
    let element_ids: HashSet<&str> = content.elements.iter().map(|e| e.id.as_str()).collect();
    validate_elements(&element_ids);
    validate_techs(content, &element_ids);
    validate_solar_system(content);
    validate_orbital_bodies(content);
    validate_asteroid_templates(content, &element_ids);
    validate_module_recipes(content, &element_ids);
    validate_hull_defs(content);
    validate_autopilot(content, &element_ids);
    validate_crew_roles(content);
    validate_scoring(content);
    validate_milestones(&content.milestones);
}

fn validate_constants(content: &GameContent) {
    assert!(
        content.constants.minutes_per_tick > 0,
        "minutes_per_tick must be > 0"
    );
}

fn validate_elements(element_ids: &HashSet<&str>) {
    assert!(
        element_ids.contains("ore"),
        "required element 'ore' is missing from content.elements"
    );
    assert!(
        element_ids.contains("slag"),
        "required element 'slag' is missing from content.elements"
    );
}

fn validate_techs(content: &GameContent, _element_ids: &HashSet<&str>) {
    let tech_ids: HashSet<&TechId> = content.techs.iter().map(|t| &t.id).collect();
    for tech in &content.techs {
        for prereq in &tech.prereqs {
            assert!(
                tech_ids.contains(prereq),
                "tech '{}' prereq '{}' is not a known tech id",
                tech.id.0,
                prereq.0,
            );
        }
        for effect in &tech.effects {
            if let sim_core::TechEffect::StatModifier {
                stat: _,
                op: _,
                value,
            } = effect
            {
                assert!(
                    value.abs() < 100.0,
                    "tech '{}' has StatModifier with unreasonable value {} (expected -100..100)",
                    tech.id.0,
                    value,
                );
            }
        }
    }
}

fn validate_solar_system(content: &GameContent) {
    let node_ids: HashSet<&str> = content
        .solar_system
        .nodes
        .iter()
        .map(|n| n.id.0.as_str())
        .collect();
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
}

fn validate_orbital_bodies(content: &GameContent) {
    let body_ids: HashSet<&str> = content
        .solar_system
        .bodies
        .iter()
        .map(|b| b.id.0.as_str())
        .collect();
    assert!(
        body_ids.len() == content.solar_system.bodies.len(),
        "duplicate body id in orbital body tree"
    );
    for body in &content.solar_system.bodies {
        if let Some(ref parent) = body.parent {
            assert!(
                body_ids.contains(parent.0.as_str()),
                "orbital body '{}' references unknown parent '{}'",
                body.id.0,
                parent.0,
            );
        }
        if let Some(ref zone) = body.zone {
            assert!(
                zone.radius_max_au_um > zone.radius_min_au_um,
                "orbital body '{}' zone has radius_max <= radius_min",
                body.id.0,
            );
            assert!(
                zone.angle_span_mdeg > 0 && zone.angle_span_mdeg <= sim_core::FULL_CIRCLE,
                "orbital body '{}' zone has invalid angle_span (must be 1..=360000)",
                body.id.0,
            );
            assert!(
                zone.scan_site_weight > 0,
                "orbital body '{}' zone has scan_site_weight of 0",
                body.id.0,
            );
        }
    }
    // Verify no cycles: every body's ancestor chain must terminate at a root.
    for body in &content.solar_system.bodies {
        let mut visited = HashSet::new();
        let mut current_id = body.parent.as_ref();
        while let Some(pid) = current_id {
            assert!(
                visited.insert(pid.0.as_str()),
                "cycle detected in orbital body tree at '{}'",
                pid.0,
            );
            current_id = content
                .solar_system
                .bodies
                .iter()
                .find(|b| b.id == *pid)
                .and_then(|b| b.parent.as_ref());
        }
    }
}

fn validate_asteroid_templates(content: &GameContent, element_ids: &HashSet<&str>) {
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
}

fn validate_module_recipes(content: &GameContent, element_ids: &HashSet<&str>) {
    for module_def in content.module_defs.values() {
        if let ModuleBehaviorDef::Processor(processor) = &module_def.behavior {
            for recipe_id in &processor.recipes {
                let recipe = content.recipes.get(recipe_id).unwrap_or_else(|| {
                    panic!(
                        "module '{}' references unknown recipe '{}'",
                        module_def.id, recipe_id
                    );
                });
                validate_recipe_elements(content, element_ids, &module_def.id, recipe);
            }
        }
        if let ModuleBehaviorDef::Assembler(assembler) = &module_def.behavior {
            for recipe_id in &assembler.recipes {
                let recipe = content.recipes.get(recipe_id).unwrap_or_else(|| {
                    panic!(
                        "module '{}' references unknown recipe '{}'",
                        module_def.id, recipe_id
                    );
                });
                for input in &recipe.inputs {
                    if let InputFilter::Element(element_id) = &input.filter {
                        assert!(
                            element_ids.contains(element_id.as_str()),
                            "module '{}' assembler recipe '{}' input element '{}' is not a known element",
                            module_def.id, recipe_id, element_id,
                        );
                    }
                }
            }
        }
    }
}

fn validate_recipe_elements(
    content: &GameContent,
    element_ids: &HashSet<&str>,
    module_id: &str,
    recipe: &sim_core::RecipeDef,
) {
    for input in &recipe.inputs {
        if let InputFilter::Element(element_id) = &input.filter {
            assert!(
                element_ids.contains(element_id.as_str()),
                "module '{}' recipe '{}' input element '{}' is not a known element",
                module_id,
                recipe.id,
                element_id,
            );
        }
    }
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
                    module_id,
                    recipe.id,
                    element,
                );
                if let YieldFormula::ElementFraction { element: fe } = yield_formula {
                    assert!(
                        element_ids.contains(fe.as_str()),
                        "module '{}' recipe '{}' YieldFormula element '{}' is not a known element",
                        module_id,
                        recipe.id,
                        fe,
                    );
                }
                if let QualityFormula::ElementFractionTimesMultiplier { element: fe, .. } =
                    quality_formula
                {
                    assert!(
                        element_ids.contains(fe.as_str()),
                        "module '{}' recipe '{}' QualityFormula element '{}' is not a known element",
                        module_id,
                        recipe.id,
                        fe,
                    );
                }
            }
            OutputSpec::Slag { .. } | OutputSpec::Component { .. } => {}
            OutputSpec::Ship { hull_id } => {
                assert!(
                    content.hulls.contains_key(hull_id),
                    "module '{}' recipe '{}' OutputSpec::Ship references unknown hull_id '{}'",
                    module_id,
                    recipe.id,
                    hull_id.0,
                );
            }
        }
    }
}

fn validate_hull_defs(content: &GameContent) {
    // Collect all slot types defined across hulls
    let hull_slot_types: HashSet<&sim_core::SlotType> = content
        .hulls
        .values()
        .flat_map(|h| h.slots.iter().map(|s| &s.slot_type))
        .collect();

    // Warn about modules with compatible_slots referencing types not in any hull
    for module_def in content.module_defs.values() {
        for slot_type in &module_def.compatible_slots {
            if !hull_slot_types.contains(slot_type) {
                eprintln!(
                    "WARNING: module '{}' has compatible_slot '{}' not found in any hull",
                    module_def.id, slot_type
                );
            }
        }
    }

    // Warn about hull slot types with no compatible modules
    for hull in content.hulls.values() {
        for slot in &hull.slots {
            let has_compatible = content
                .module_defs
                .values()
                .any(|m| m.compatible_slots.contains(&slot.slot_type));
            if !has_compatible {
                eprintln!(
                    "WARNING: hull '{}' slot '{}' (type '{}') has no compatible modules",
                    hull.id, slot.label, slot.slot_type
                );
            }
        }
    }

    // Validate fitting templates reference valid hulls, modules, and compatible slots
    for (hull_id, fittings) in &content.fitting_templates {
        let hull = content.hulls.get(hull_id);
        assert!(
            hull.is_some(),
            "fitting_templates references unknown hull '{hull_id}'"
        );
        let hull = hull.unwrap();
        for fitting in fittings {
            assert!(
                fitting.slot_index < hull.slots.len(),
                "fitting_templates hull '{}' slot_index {} out of range (hull has {} slots)",
                hull_id,
                fitting.slot_index,
                hull.slots.len()
            );
            let module_def = content.module_defs.get(&fitting.module_def_id.0);
            assert!(
                module_def.is_some(),
                "fitting_templates hull '{}' references unknown module '{}'",
                hull_id,
                fitting.module_def_id
            );
            let module_def = module_def.unwrap();
            let slot_type = &hull.slots[fitting.slot_index].slot_type;
            assert!(
                module_def.compatible_slots.contains(slot_type),
                "fitting_templates hull '{}' slot {} (type '{}') incompatible with module '{}' (compatible: {:?})",
                hull_id,
                fitting.slot_index,
                slot_type,
                fitting.module_def_id,
                module_def.compatible_slots
            );
        }
    }
}

/// Validate autopilot config cross-references against content.
/// Only checks non-empty fields — empty means "not configured" (test fixtures).
fn validate_autopilot(content: &GameContent, element_ids: &HashSet<&str>) {
    let ap = &content.autopilot;
    let tech_ids: HashSet<&str> = content.techs.iter().map(|t| t.id.0.as_str()).collect();
    let comp_ids: HashSet<&str> = content
        .component_defs
        .iter()
        .map(|c| c.id.as_str())
        .collect();

    // Validate that role names referenced by the autopilot have at least one matching module
    for (role_name, field_name) in [
        (&ap.propellant_role, "propellant_role"),
        (&ap.propellant_support_role, "propellant_support_role"),
        (&ap.shipyard_role, "shipyard_role"),
    ] {
        if !role_name.is_empty() {
            let has_module = content
                .module_defs
                .values()
                .any(|def| def.roles.iter().any(|r| r == role_name));
            assert!(
                has_module,
                "autopilot.{field_name} '{role_name}' has no matching modules in module_defs"
            );
        }
    }
    if !ap.volatile_element.is_empty() {
        assert!(
            element_ids.contains(ap.volatile_element.as_str()),
            "autopilot.volatile_element '{}' not in elements",
            ap.volatile_element
        );
    }
    if !ap.propellant_element.is_empty() {
        assert!(
            element_ids.contains(ap.propellant_element.as_str()),
            "autopilot.propellant_element '{}' not in elements",
            ap.propellant_element
        );
    }
    if !ap.primary_mining_element.is_empty() {
        assert!(
            element_ids.contains(ap.primary_mining_element.as_str()),
            "autopilot.primary_mining_element '{}' not in elements",
            ap.primary_mining_element
        );
    }
    if !ap.deep_scan_tech.is_empty() {
        assert!(
            tech_ids.contains(ap.deep_scan_tech.as_str()),
            "autopilot.deep_scan_tech '{}' not in techs",
            ap.deep_scan_tech
        );
    }
    if !ap.ship_construction_tech.is_empty() {
        assert!(
            tech_ids.contains(ap.ship_construction_tech.as_str()),
            "autopilot.ship_construction_tech '{}' not in techs",
            ap.ship_construction_tech
        );
    }
    if !ap.shipyard_import_component.is_empty() {
        assert!(
            comp_ids.contains(ap.shipyard_import_component.as_str()),
            "autopilot.shipyard_import_component '{}' not in component_defs",
            ap.shipyard_import_component
        );
    }
    if !ap.export_component.component_id.is_empty() {
        assert!(
            comp_ids.contains(ap.export_component.component_id.as_str()),
            "autopilot.export_component.component_id '{}' not in component_defs",
            ap.export_component.component_id
        );
    }
    for entry in &ap.export_elements {
        assert!(
            element_ids.contains(entry.element.as_str()),
            "autopilot.export_elements element '{}' not in elements",
            entry.element
        );
    }
    let valid_tasks: HashSet<&str> = ["Deposit", "Mine", "DeepScan", "Survey"]
        .into_iter()
        .collect();
    for task in &ap.task_priority {
        assert!(
            valid_tasks.contains(task.as_str()),
            "autopilot.task_priority contains unknown task type '{task}'. \
             Valid values: Deposit, Mine, DeepScan, Survey"
        );
    }
}

fn validate_crew_roles(content: &GameContent) {
    for (role_id, def) in &content.crew_roles {
        if def.recruitment_cost <= 0.0 {
            eprintln!(
                "warning: crew role '{}' has non-positive recruitment_cost ({})",
                role_id, def.recruitment_cost
            );
        }
    }
    // Validate module crew_requirement references valid roles
    for (module_id, module_def) in &content.module_defs {
        for role in module_def.crew_requirement.keys() {
            assert!(
                content.crew_roles.contains_key(role),
                "module '{module_id}' crew_requirement references unknown crew role '{role}'"
            );
        }
    }
}

fn validate_scoring(content: &GameContent) {
    // Skip validation for test fixtures that use ScoringConfig::default() (empty dimensions).
    // Real content from scoring.json will have dimensions populated.
    if content.scoring.dimensions.is_empty() && content.scoring.thresholds.is_empty() {
        return;
    }
    if let Err(err) = sim_core::validate_scoring_config(&content.scoring) {
        panic!("invalid scoring config: {err}");
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

fn load_hull_defs(
    dir: &Path,
) -> Result<std::collections::BTreeMap<sim_core::HullId, sim_core::HullDef>> {
    match std::fs::read_to_string(dir.join("hull_defs.json")) {
        Ok(text) => {
            let defs: Vec<sim_core::HullDef> =
                serde_json::from_str(&text).context("parsing hull_defs.json")?;
            let mut map = std::collections::BTreeMap::new();
            for def in defs {
                let id = def.id.clone();
                assert!(
                    map.insert(id.clone(), def).is_none(),
                    "duplicate hull ID: {id}"
                );
            }
            Ok(map)
        }
        Err(_) => Ok(std::collections::BTreeMap::new()),
    }
}

fn load_fitting_templates(
    dir: &Path,
) -> Result<std::collections::BTreeMap<sim_core::HullId, Vec<sim_core::FittedModule>>> {
    match std::fs::read_to_string(dir.join("fitting_templates.json")) {
        Ok(text) => {
            let map: std::collections::BTreeMap<String, Vec<sim_core::FittedModule>> =
                serde_json::from_str(&text).context("parsing fitting_templates.json")?;
            Ok(map
                .into_iter()
                .map(|(key, value)| (sim_core::HullId(key), value))
                .collect())
        }
        Err(_) => Ok(std::collections::BTreeMap::new()),
    }
}

/// Validate milestone definitions: unique IDs, valid chained references.
fn validate_milestones(milestones: &[sim_core::MilestoneDef]) {
    let mut seen_ids = std::collections::HashSet::new();
    for m in milestones {
        assert!(
            seen_ids.insert(m.id.as_str()),
            "duplicate milestone id '{}'",
            m.id
        );
    }
    // Validate chained milestone references point to existing IDs.
    for m in milestones {
        for cond in &m.conditions {
            if let sim_core::MilestoneCondition::MilestoneCompleted { milestone_id } = cond {
                assert!(
                    seen_ids.contains(milestone_id.as_str()),
                    "milestone '{}' references unknown prerequisite milestone '{milestone_id}'",
                    m.id
                );
            }
        }
    }
}

/// Load an optional JSON file, returning `T::default()` if the file is missing.
fn load_optional_json<T: serde::de::DeserializeOwned + Default>(
    dir: &Path,
    filename: &str,
) -> Result<T> {
    match std::fs::read_to_string(dir.join(filename)) {
        Ok(text) => serde_json::from_str(&text).with_context(|| format!("parsing {filename}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(anyhow::anyhow!("reading {filename}: {e}")),
    }
}

/// Load crew role definitions from `crew_roles.json`.
/// Returns an empty map if the file is missing.
fn load_crew_roles(
    dir: &Path,
) -> Result<std::collections::BTreeMap<sim_core::CrewRole, sim_core::CrewRoleDef>> {
    let defs: Vec<sim_core::CrewRoleDef> = load_optional_json(dir, "crew_roles.json")?;
    let mut seen = std::collections::HashSet::new();
    for def in &defs {
        assert!(seen.insert(&def.id), "duplicate crew role id '{}'", def.id);
        if def.recruitment_cost <= 0.0 {
            eprintln!(
                "warning: crew role '{}' has non-positive recruitment_cost ({})",
                def.id, def.recruitment_cost
            );
        }
    }
    Ok(defs.into_iter().map(|d| (d.id.clone(), d)).collect())
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
    let module_defs: AHashMap<String, ModuleDef> = {
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
    let alert_rules: Vec<AlertRuleDef> = match std::fs::read_to_string(dir.join("alerts.json")) {
        Ok(text) => serde_json::from_str(&text).context("parsing alerts.json")?,
        Err(_) => Vec::new(),
    };
    let mut sim_events: Vec<sim_core::sim_events::SimEventDef> =
        match std::fs::read_to_string(dir.join("events.json")) {
            Ok(text) => serde_json::from_str(&text).context("parsing events.json")?,
            Err(_) => Vec::new(),
        };
    for event in &mut sim_events {
        event.resolve_weight();
    }
    let hulls = load_hull_defs(dir)?;
    let fitting_templates = load_fitting_templates(dir)?;
    let initial_station: sim_core::InitialStationDef =
        load_optional_json(dir, "initial_station.json")?;
    let autopilot: sim_core::AutopilotConfig = load_optional_json(dir, "autopilot.json")?;
    let scoring: sim_core::ScoringConfig = load_optional_json(dir, "scoring.json")?;
    let milestones: Vec<sim_core::MilestoneDef> = load_optional_json(dir, "milestones.json")?;
    let crew_roles = load_crew_roles(dir)?;
    let recipes: Vec<sim_core::RecipeDef> = serde_json::from_str(
        &std::fs::read_to_string(dir.join("recipes.json")).context("reading recipes.json")?,
    )
    .context("parsing recipes.json")?;
    // Check for duplicate recipe IDs before converting to map
    let mut recipe_ids_seen = std::collections::HashSet::new();
    for recipe in &recipes {
        assert!(
            recipe_ids_seen.insert(&recipe.id),
            "duplicate recipe id '{}'",
            recipe.id
        );
    }
    let recipe_map: std::collections::BTreeMap<sim_core::RecipeId, sim_core::RecipeDef> =
        recipes.into_iter().map(|r| (r.id.clone(), r)).collect();
    let mut content = GameContent {
        content_version: techs_file.content_version,
        techs: techs_file.techs,
        solar_system,
        asteroid_templates: templates_file.templates,
        elements: elements_file.elements,
        module_defs,
        component_defs,
        recipes: recipe_map,
        pricing,
        constants,
        alert_rules,
        events: sim_events,
        hulls,
        fitting_templates,
        initial_station,
        autopilot,
        crew_roles,
        scoring,
        milestones,
        density_map: AHashMap::default(),
    };
    content.constants.derive_tick_values();
    sim_core::derive_module_tick_values(&mut content.module_defs, &content.constants);
    content.init_caches();
    sim_core::sim_events::validate_event_defs(&content.events);
    validate_content(&content);
    Ok(content)
}

/// Build inventory items from an `InitialStationDef`.
fn build_initial_inventory(init: &sim_core::InitialStationDef) -> Vec<InventoryItem> {
    let mut inventory = Vec::new();
    for (index, module_def_id) in init.modules.iter().enumerate() {
        inventory.push(InventoryItem::Module {
            item_id: ModuleItemId(format!("module_item_{:04}", index + 1)),
            module_def_id: module_def_id.clone(),
        });
    }
    for mat in &init.materials {
        inventory.push(InventoryItem::Material {
            element: mat.element.clone(),
            kg: mat.kg,
            quality: mat.quality,
            thermal: None,
        });
    }
    for comp in &init.components {
        inventory.push(InventoryItem::Component {
            component_id: ComponentId(comp.id.clone()),
            count: comp.count,
            quality: comp.quality,
        });
    }
    inventory
}

fn build_initial_ship(
    content: &GameContent,
    c: &sim_core::Constants,
    position: &sim_core::Position,
) -> (ShipId, ShipState) {
    let ship_id = ShipId("ship_0001".to_string());
    let owner = PrincipalId("principal_autopilot".to_string());
    let hull_id = sim_core::HullId("hull_general_purpose".to_string());
    let fitted_modules = content
        .fitting_templates
        .get(&hull_id)
        .cloned()
        .unwrap_or_default();
    let mut ship = ShipState {
        id: ship_id.clone(),
        position: position.clone(),
        owner,
        inventory: vec![],
        cargo_capacity_m3: c.ship_cargo_capacity_m3,
        task: None,
        speed_ticks_per_au: None,
        modifiers: sim_core::modifiers::ModifierSet::default(),
        hull_id: hull_id.clone(),
        fitted_modules,
        propellant_kg: 0.0,
        propellant_capacity_kg: 0.0,
        crew: std::collections::BTreeMap::new(),
        leaders: Vec::new(),
    };
    if content.hulls.contains_key(&hull_id) {
        sim_core::recompute_ship_stats(&mut ship, content);
        ship.propellant_kg = ship.propellant_capacity_kg;
    }
    (ship_id, ship)
}

pub fn build_initial_state(content: &GameContent, seed: u64, rng: &mut impl Rng) -> GameState {
    // Station is in Earth orbit zone (~3000 µAU from Earth, i.e. ~450km altitude)
    let earth_orbit_pos = sim_core::Position {
        parent_body: sim_core::BodyId("earth_orbit_zone".to_string()),
        radius_au_um: sim_core::RadiusAuMicro(3_000),
        angle_mdeg: sim_core::AngleMilliDeg(0),
    };
    let c = &content.constants;
    let station_id = StationId("station_earth_orbit".to_string());

    let station = StationState {
        id: station_id.clone(),
        position: earth_orbit_pos.clone(),
        inventory: build_initial_inventory(&content.initial_station),
        cargo_capacity_m3: c.station_cargo_capacity_m3,
        power_available_per_tick: c.station_power_available_per_tick,
        modules: vec![],
        modifiers: sim_core::modifiers::ModifierSet::default(),
        crew: content.initial_station.crew.clone(),
        leaders: Vec::new(),
        thermal_links: Vec::new(),
        power: PowerState::default(),
        cached_inventory_volume_m3: None,
        module_type_index: sim_core::ModuleTypeIndex::default(),
        module_id_index: std::collections::HashMap::new(),
        power_budget_cache: sim_core::PowerBudgetCache::default(),
    };
    let (ship_id, ship) = build_initial_ship(content, c, &earth_orbit_pos);
    // Place scan sites in zone bodies using weighted picking + area-sampled positions.
    let zone_bodies: Vec<&sim_core::OrbitalBodyDef> = content
        .solar_system
        .bodies
        .iter()
        .filter(|b| b.zone.is_some())
        .collect();
    let templates = &content.asteroid_templates;
    let mut scan_sites = Vec::new();
    if !zone_bodies.is_empty() && !templates.is_empty() {
        let template_count = u32::try_from(content.asteroid_templates.len()).unwrap_or(u32::MAX);
        let total_sites = c.asteroid_count_per_template * template_count;
        for _ in 0..total_sites {
            let body = sim_core::pick_zone_weighted(&zone_bodies, rng);
            let zone_class = body.zone.as_ref().expect("zone body").resource_class;
            let template = sim_core::pick_template_biased(templates, zone_class, rng);
            let position = sim_core::random_position_in_zone(body, rng);
            let uuid = sim_core::generate_uuid(rng);
            scan_sites.push(ScanSite {
                id: SiteId(format!("site_{uuid}")),
                position,
                template_id: template.id.clone(),
            });
        }
    }
    GameState {
        meta: MetaState {
            tick: 0,
            seed,
            schema_version: sim_core::CURRENT_SCHEMA_VERSION,
            content_version: content.content_version.clone(),
        },
        scan_sites,
        asteroids: std::collections::BTreeMap::new(),
        ships: [(ship_id, ship)].into_iter().collect(),
        stations: [(station_id, station)].into_iter().collect(),
        research: ResearchState {
            unlocked: std::collections::HashSet::new(),
            data_pool: AHashMap::default(),
            evidence: AHashMap::default(),
            action_counts: AHashMap::default(),
        },
        balance: 1_000_000_000.0,
        export_revenue_total: 0.0,
        export_count: 0,
        counters: Counters {
            next_event_id: 0,
            next_command_id: 0,
            next_asteroid_id: 0,
            next_lot_id: 0,
            next_module_instance_id: 0,
        },
        modifiers: sim_core::modifiers::ModifierSet::default(),
        events: sim_core::sim_events::SimEventState::default(),
        propellant_consumed_total: 0.0,
        body_cache: sim_core::build_body_cache(&content.solar_system.bodies),
    }
}

/// Auto-assign available crew to modules that need them, highest priority first.
/// Call after building initial state so modules start with crew assigned.
pub fn auto_assign_initial_crew(state: &mut GameState, content: &GameContent) {
    let station_ids: Vec<StationId> = state.stations.keys().cloned().collect();
    for station_id in station_ids {
        let Some(station) = state.stations.get(&station_id) else {
            continue;
        };
        // Collect modules with crew requirements, sorted by priority desc then ID asc
        let mut module_order: Vec<usize> = (0..station.modules.len()).collect();
        module_order.sort_by(|&a, &b| {
            station.modules[b]
                .module_priority
                .cmp(&station.modules[a].module_priority)
                .then_with(|| station.modules[a].id.0.cmp(&station.modules[b].id.0))
        });
        // Track remaining crew
        let mut remaining: std::collections::BTreeMap<sim_core::CrewRole, u32> =
            station.crew.clone();
        let mut assignments: Vec<(usize, std::collections::BTreeMap<sim_core::CrewRole, u32>)> =
            Vec::new();
        for module_index in module_order {
            let def_id = &station.modules[module_index].def_id;
            let Some(def) = content.module_defs.get(def_id) else {
                continue;
            };
            if def.crew_requirement.is_empty() {
                continue;
            }
            let mut assigned = std::collections::BTreeMap::new();
            let mut can_satisfy = true;
            for (role, &needed) in &def.crew_requirement {
                let available = remaining.get(role).copied().unwrap_or(0);
                if available >= needed {
                    assigned.insert(role.clone(), needed);
                } else {
                    can_satisfy = false;
                    break;
                }
            }
            if can_satisfy {
                for (role, count) in &assigned {
                    *remaining.entry(role.clone()).or_insert(0) -= count;
                }
                assignments.push((module_index, assigned));
            }
        }
        let station = state.stations.get_mut(&station_id).expect("checked");
        for (module_index, assigned) in assignments {
            station.modules[module_index].assigned_crew = assigned;
            if let Some(def) = content
                .module_defs
                .get(&station.modules[module_index].def_id)
            {
                station.modules[module_index].efficiency = sim_core::compute_module_efficiency(
                    &station.modules[module_index],
                    def,
                    &content.constants,
                );
            }
        }
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
) -> Result<(GameState, ChaCha8Rng)> {
    if let Some(path) = state_file {
        let json =
            std::fs::read_to_string(path).with_context(|| format!("reading state file: {path}"))?;
        let mut loaded: GameState =
            serde_json::from_str(&json).with_context(|| format!("parsing state file: {path}"))?;

        // Validate schema version
        let expected = sim_core::CURRENT_SCHEMA_VERSION;
        let actual = loaded.meta.schema_version;
        anyhow::ensure!(
            actual == expected,
            "Save file '{path}' has schema version {actual}, but the engine expects version {expected}. \
             This save is incompatible with the current engine."
        );

        loaded.body_cache = sim_core::build_body_cache(&content.solar_system.bodies);
        for station in loaded.stations.values_mut() {
            station.rebuild_module_index(content);
            station.init_module_efficiency(content);
        }
        let rng = ChaCha8Rng::seed_from_u64(loaded.meta.seed);
        validate_state(&loaded, content);
        Ok((loaded, rng))
    } else {
        let resolved_seed = seed.unwrap_or_else(rand::random);
        let mut rng = ChaCha8Rng::seed_from_u64(resolved_seed);
        let mut state = build_initial_state(content, resolved_seed, &mut rng);
        for station in state.stations.values_mut() {
            station.rebuild_module_index(content);
        }
        Ok((state, rng))
    }
}

// ---------------------------------------------------------------------------
// RunSetup — eliminates duplicated init across sim_cli / sim_daemon / sim_bench
// ---------------------------------------------------------------------------

/// Fully-initialized simulation ready to run.
pub struct RunSetup {
    pub content: GameContent,
    pub game_state: GameState,
    pub rng: ChaCha8Rng,
    pub run_dir: Option<PathBuf>,
    pub metrics_writer: Option<MetricsFileWriter>,
}

/// Builder for [`RunSetup`]. Loads content, builds/loads state, optionally
/// creates a run directory and metrics writer.
pub struct RunSetupBuilder {
    content: GameContent,
    seed: Option<u64>,
    state_file: Option<String>,
    enable_metrics: bool,
    metrics_every: u64,
    runner_args: serde_json::Value,
}

impl RunSetupBuilder {
    /// Start a builder by loading content from `content_dir`.
    pub fn from_content_dir(content_dir: &str) -> Result<Self> {
        let content = load_content(content_dir)?;
        Ok(Self {
            content,
            seed: None,
            state_file: None,
            enable_metrics: false,
            metrics_every: 60,
            runner_args: serde_json::Value::Null,
        })
    }

    /// Use an already-loaded [`GameContent`] (useful for bench runner which
    /// loads content once and reuses it across seeds).
    pub fn from_content(content: GameContent) -> Self {
        Self {
            content,
            seed: None,
            state_file: None,
            enable_metrics: false,
            metrics_every: 60,
            runner_args: serde_json::Value::Null,
        }
    }

    /// Set the world-generation seed. Mutually exclusive with [`state_file`](Self::state_file).
    #[must_use]
    pub fn seed(mut self, seed: Option<u64>) -> Self {
        self.seed = seed;
        self
    }

    /// Load initial state from a JSON file. Mutually exclusive with [`seed`](Self::seed).
    #[must_use]
    pub fn state_file(mut self, path: Option<String>) -> Self {
        self.state_file = path;
        self
    }

    /// Enable run-directory creation, `run_info.json`, and metrics CSV writing.
    #[must_use]
    pub fn metrics(mut self, metrics_every: u64, runner_args: serde_json::Value) -> Self {
        self.enable_metrics = true;
        self.metrics_every = metrics_every;
        self.runner_args = runner_args;
        self
    }

    /// Consume the builder and produce a [`RunSetup`].
    pub fn build(self) -> Result<RunSetup> {
        let (game_state, rng) =
            load_or_build_state(&self.content, self.seed, self.state_file.as_deref())?;

        let (run_dir, metrics_writer) = if self.enable_metrics {
            let run_id = generate_run_id(game_state.meta.seed);
            let dir = create_run_dir(&run_id)?;
            write_run_info(
                &dir,
                &run_id,
                game_state.meta.seed,
                &self.content.content_version,
                self.metrics_every,
                self.runner_args,
            )?;
            let element_ids = sim_core::content_element_ids(&self.content);
            let behavior_types = sim_core::content_behavior_types(&self.content);
            let writer = MetricsFileWriter::new(dir.clone(), element_ids, behavior_types)
                .with_context(|| format!("opening metrics CSV in {}", dir.display()))?;
            (Some(dir), Some(writer))
        } else {
            (None, None)
        };

        Ok(RunSetup {
            content: self.content,
            game_state,
            rng,
            run_dir,
            metrics_writer,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::{
        test_fixtures::{base_content, minimal_content, test_position, ModuleDefBuilder},
        AHashMap, AssemblerDef, AsteroidTemplateDef, Counters, GameState, InputAmount, InputFilter,
        InventoryItem, ItemKind, MetaState, ModuleBehaviorDef, NodeDef, NodeId, OutputSpec,
        ProcessorDef, QualityFormula, RecipeDef, RecipeInput, ResearchState, StationId,
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
            preferred_class: None,
        });
        validate_content(&content);
    }

    #[test]
    #[should_panic(expected = "not a known element")]
    fn test_recipe_output_unknown_element_panics() {
        let mut content = minimal_content();
        let recipe = RecipeDef {
            id: sim_core::RecipeId("recipe_test".to_string()),
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
            thermal_req: None,
            required_tech: None,
            tags: vec![],
        };
        content.recipes.insert(recipe.id.clone(), recipe);
        content.module_defs.insert(
            "mod_test".to_string(),
            ModuleDefBuilder::new("mod_test")
                .name("Test Module")
                .mass(1000.0)
                .volume(5.0)
                .power(10.0)
                .behavior(ModuleBehaviorDef::Processor(ProcessorDef {
                    processing_interval_minutes: 10,
                    processing_interval_ticks: 10,
                    recipes: vec![sim_core::RecipeId("recipe_test".to_string())],
                }))
                .build(),
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
            ship.position, station.position,
            "ship and station should be at the same position"
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
        let recipe = RecipeDef {
            id: sim_core::RecipeId("recipe_asm_test".to_string()),
            inputs: vec![RecipeInput {
                filter: InputFilter::Element("Unobtanium".to_string()),
                amount: InputAmount::Kg(50.0),
            }],
            outputs: vec![],
            efficiency: 1.0,
            thermal_req: None,
            required_tech: None,
            tags: vec![],
        };
        content.recipes.insert(recipe.id.clone(), recipe);
        content.module_defs.insert(
            "mod_assembler_test".to_string(),
            ModuleDefBuilder::new("mod_assembler_test")
                .name("Test Assembler")
                .mass(1000.0)
                .volume(5.0)
                .power(10.0)
                .behavior(ModuleBehaviorDef::Assembler(AssemblerDef {
                    assembly_interval_minutes: 10,
                    assembly_interval_ticks: 10,
                    max_stock: std::collections::HashMap::new(),
                    recipes: vec![sim_core::RecipeId("recipe_asm_test".to_string())],
                }))
                .build(),
        );
        validate_content(&content);
    }

    #[test]
    #[should_panic(expected = "duplicate body id")]
    fn test_body_tree_duplicate_id_panics() {
        let mut content = minimal_content();
        let body = sim_core::OrbitalBodyDef {
            id: sim_core::BodyId("dup".to_string()),
            name: "Dup".to_string(),
            parent: None,
            body_type: sim_core::BodyType::Star,
            radius_au_um: 0,
            angle_mdeg: 0,
            solar_intensity: 1.0,
            zone: None,
        };
        content.solar_system.bodies.push(body.clone());
        content.solar_system.bodies.push(body);
        validate_content(&content);
    }

    #[test]
    #[should_panic(expected = "unknown parent")]
    fn test_body_tree_unknown_parent_panics() {
        let mut content = minimal_content();
        content.solar_system.bodies.push(sim_core::OrbitalBodyDef {
            id: sim_core::BodyId("orphan".to_string()),
            name: "Orphan".to_string(),
            parent: Some(sim_core::BodyId("nonexistent".to_string())),
            body_type: sim_core::BodyType::Planet,
            radius_au_um: 1_000_000,
            angle_mdeg: 0,
            solar_intensity: 1.0,
            zone: None,
        });
        validate_content(&content);
    }

    #[test]
    #[should_panic(expected = "radius_max <= radius_min")]
    fn test_body_tree_inverted_zone_radius_panics() {
        let mut content = minimal_content();
        content.solar_system.bodies.push(sim_core::OrbitalBodyDef {
            id: sim_core::BodyId("bad_zone".to_string()),
            name: "Bad Zone".to_string(),
            parent: None,
            body_type: sim_core::BodyType::Zone,
            radius_au_um: 0,
            angle_mdeg: 0,
            solar_intensity: 1.0,
            zone: Some(sim_core::ZoneDef {
                radius_min_au_um: 5000,
                radius_max_au_um: 1000,
                angle_start_mdeg: 0,
                angle_span_mdeg: 360_000,
                resource_class: sim_core::ResourceClass::MetalRich,
                scan_site_weight: 1,
            }),
        });
        validate_content(&content);
    }

    #[test]
    #[should_panic(expected = "invalid angle_span")]
    fn test_body_tree_oversized_angle_span_panics() {
        let mut content = minimal_content();
        content.solar_system.bodies.push(sim_core::OrbitalBodyDef {
            id: sim_core::BodyId("wide_zone".to_string()),
            name: "Wide Zone".to_string(),
            parent: None,
            body_type: sim_core::BodyType::Zone,
            radius_au_um: 0,
            angle_mdeg: 0,
            solar_intensity: 1.0,
            zone: Some(sim_core::ZoneDef {
                radius_min_au_um: 1000,
                radius_max_au_um: 5000,
                angle_start_mdeg: 0,
                angle_span_mdeg: 400_000,
                resource_class: sim_core::ResourceClass::Mixed,
                scan_site_weight: 1,
            }),
        });
        validate_content(&content);
    }

    #[test]
    fn test_body_tree_deserialization() {
        let json = r#"{
            "id": "earth",
            "name": "Earth",
            "parent": "sun",
            "body_type": "Planet",
            "radius_au_um": 1000000,
            "angle_mdeg": 0,
            "zone": null
        }"#;
        let body: sim_core::OrbitalBodyDef = serde_json::from_str(json).unwrap();
        assert_eq!(body.id, sim_core::BodyId("earth".to_string()));
        assert_eq!(body.parent, Some(sim_core::BodyId("sun".to_string())));
        assert_eq!(body.body_type, sim_core::BodyType::Planet);
        assert!((body.solar_intensity - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_load_content_parses_body_tree() {
        let content_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let content = load_content(content_dir.to_str().unwrap()).unwrap();
        assert!(
            !content.solar_system.bodies.is_empty(),
            "bodies should be populated from solar_system.json"
        );
        let sun = content.solar_system.bodies.iter().find(|b| b.id.0 == "sun");
        assert!(sun.is_some(), "should have a sun body");
        assert!(sun.unwrap().parent.is_none(), "sun should have no parent");
        let inner_belt = content
            .solar_system
            .bodies
            .iter()
            .find(|b| b.id.0 == "inner_belt");
        assert!(inner_belt.is_some(), "should have inner_belt body");
        assert!(
            inner_belt.unwrap().zone.is_some(),
            "inner_belt should have a zone"
        );
    }

    #[test]
    fn test_module_ports_load_from_content() {
        let content_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let content = load_content(content_dir.to_str().unwrap()).unwrap();

        // Smelter should have a molten_out port
        let smelter = content.module_defs.get("module_basic_smelter");
        assert!(smelter.is_some(), "smelter should exist");
        let smelter = smelter.unwrap();
        assert_eq!(smelter.ports.len(), 1, "smelter should have 1 port");
        assert_eq!(smelter.ports[0].id, "molten_out");
        assert_eq!(smelter.ports[0].direction, sim_core::PortDirection::Output);
        assert_eq!(smelter.ports[0].accepts, sim_core::PortFilter::AnyMolten);
    }

    #[test]
    fn test_module_without_ports_has_empty_vec() {
        let content_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let content = load_content(content_dir.to_str().unwrap()).unwrap();

        // Basic iron refinery has no ports
        let refinery = content.module_defs.get("module_basic_iron_refinery");
        assert!(refinery.is_some(), "refinery should exist");
        assert!(
            refinery.unwrap().ports.is_empty(),
            "refinery should have no ports (backward compat)"
        );
    }

    #[test]
    fn test_crucible_loads_from_content() {
        let content_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let content = load_content(content_dir.to_str().unwrap()).unwrap();

        let crucible = content.module_defs.get("module_crucible");
        assert!(crucible.is_some(), "crucible should exist in content");
        let crucible = crucible.unwrap();
        assert!(matches!(
            crucible.behavior,
            sim_core::ModuleBehaviorDef::ThermalContainer(_)
        ));
        assert_eq!(crucible.ports.len(), 2, "crucible should have 2 ports");
    }

    #[test]
    fn test_port_types_serialize_round_trip() {
        use sim_core::{ModulePort, PortDirection, PortFilter};

        let ports = vec![
            ModulePort {
                id: "molten_in".to_string(),
                direction: PortDirection::Input,
                accepts: PortFilter::AnyMolten,
            },
            ModulePort {
                id: "fe_out".to_string(),
                direction: PortDirection::Output,
                accepts: PortFilter::Element("Fe".to_string()),
            },
            ModulePort {
                id: "liquid_in".to_string(),
                direction: PortDirection::Input,
                accepts: PortFilter::Phase(sim_core::Phase::Liquid),
            },
        ];

        let json = serde_json::to_string(&ports).unwrap();
        let deserialized: Vec<ModulePort> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.len(), 3);
        assert_eq!(deserialized[0].direction, PortDirection::Input);
        assert_eq!(deserialized[0].accepts, PortFilter::AnyMolten);
        assert_eq!(
            deserialized[1].accepts,
            PortFilter::Element("Fe".to_string())
        );
        assert_eq!(
            deserialized[2].accepts,
            PortFilter::Phase(sim_core::Phase::Liquid)
        );
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
            asteroids: std::collections::BTreeMap::new(),
            ships: std::collections::BTreeMap::new(),
            stations: [(
                station_id.clone(),
                StationState {
                    id: station_id,
                    position: test_position(),
                    inventory: vec![InventoryItem::Material {
                        element: "Unobtanium".to_string(),
                        kg: 100.0,
                        quality: 1.0,
                        thermal: None,
                    }],
                    cargo_capacity_m3: 1000.0,
                    power_available_per_tick: 100.0,
                    modules: vec![],
                    modifiers: sim_core::modifiers::ModifierSet::default(),
                    crew: std::collections::BTreeMap::new(),
                    leaders: Vec::new(),
                    thermal_links: Vec::new(),
                    power: PowerState::default(),
                    cached_inventory_volume_m3: None,
                    module_type_index: sim_core::ModuleTypeIndex::default(),
                    module_id_index: std::collections::HashMap::new(),
                    power_budget_cache: sim_core::PowerBudgetCache::default(),
                },
            )]
            .into_iter()
            .collect(),
            research: ResearchState {
                unlocked: std::collections::HashSet::new(),
                data_pool: AHashMap::default(),
                evidence: AHashMap::default(),
                action_counts: AHashMap::default(),
            },
            balance: 0.0,
            export_revenue_total: 0.0,
            export_count: 0,
            counters: Counters {
                next_event_id: 0,
                next_command_id: 0,
                next_asteroid_id: 0,
                next_lot_id: 0,
                next_module_instance_id: 0,
            },
            modifiers: sim_core::modifiers::ModifierSet::default(),
            events: sim_core::sim_events::SimEventState::default(),
            propellant_consumed_total: 0.0,
            body_cache: AHashMap::default(),
        };
        validate_state(&state, &content);
    }

    #[test]
    fn schema_version_match_loads_successfully() {
        let content = base_content();
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let state = build_initial_state(&content, 1, &mut rng);
        assert_eq!(state.meta.schema_version, sim_core::CURRENT_SCHEMA_VERSION);

        // Write state to temp file and load it back
        let dir = std::env::temp_dir().join("sim_test_schema_match");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        let json = serde_json::to_string_pretty(&state).unwrap();
        std::fs::write(&path, &json).unwrap();

        let result = load_or_build_state(&content, None, Some(path.to_str().unwrap()));
        assert!(
            result.is_ok(),
            "current schema version should load: {result:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn schema_version_mismatch_returns_error() {
        let content = base_content();
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let mut state = build_initial_state(&content, 1, &mut rng);
        state.meta.schema_version = 999; // future version

        let dir = std::env::temp_dir().join("sim_test_schema_mismatch");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        let json = serde_json::to_string_pretty(&state).unwrap();
        std::fs::write(&path, &json).unwrap();

        let result = load_or_build_state(&content, None, Some(path.to_str().unwrap()));
        assert!(result.is_err(), "mismatched schema version should error");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("schema version 999"),
            "error should mention actual version: {err_msg}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn run_setup_builder_without_metrics() {
        let setup = RunSetupBuilder::from_content_dir("../../content")
            .unwrap()
            .seed(Some(42))
            .build()
            .unwrap();

        assert_eq!(setup.game_state.meta.seed, 42);
        assert!(setup.run_dir.is_none());
        assert!(setup.metrics_writer.is_none());
        assert!(!setup.content.techs.is_empty());
    }

    #[test]
    fn run_setup_builder_with_metrics() {
        let setup = RunSetupBuilder::from_content_dir("../../content")
            .unwrap()
            .seed(Some(99))
            .metrics(60, serde_json::json!({"runner": "test"}))
            .build()
            .unwrap();

        assert_eq!(setup.game_state.meta.seed, 99);
        assert!(setup.run_dir.is_some());
        assert!(setup.metrics_writer.is_some());
        let run_dir = setup.run_dir.as_ref().unwrap();
        assert!(run_dir.join("run_info.json").exists());

        // Clean up
        std::fs::remove_dir_all(run_dir).ok();
    }

    #[test]
    fn run_setup_builder_from_content() {
        let content = load_content("../../content").unwrap();
        let setup = RunSetupBuilder::from_content(content)
            .seed(Some(7))
            .build()
            .unwrap();

        assert_eq!(setup.game_state.meta.seed, 7);
        assert!(setup.run_dir.is_none());
    }

    /// Verify build_initial_state() produces the same module set as dev_advanced_state.json.
    /// Prevents drift between the two initial state sources.
    #[test]
    fn build_initial_state_matches_dev_advanced_state_modules() {
        let content = load_content("../../content").unwrap();
        let mut rng = rand::rngs::mock::StepRng::new(42, 1);
        let built = build_initial_state(&content, 42, &mut rng);

        let json = std::fs::read_to_string("../../content/dev_advanced_state.json").unwrap();
        let loaded: GameState = serde_json::from_str(&json).unwrap();

        let station_id = StationId("station_earth_orbit".to_string());
        let built_station = &built.stations[&station_id];
        let loaded_station = &loaded.stations[&station_id];

        // Extract module def_ids from both, sorted for comparison
        let mut built_modules: Vec<&str> = built_station
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Module { module_def_id, .. } => Some(module_def_id.as_str()),
                _ => None,
            })
            .collect();
        built_modules.sort();

        let mut loaded_modules: Vec<&str> = loaded_station
            .inventory
            .iter()
            .filter_map(|item| match item {
                InventoryItem::Module { module_def_id, .. } => Some(module_def_id.as_str()),
                _ => None,
            })
            .collect();
        loaded_modules.sort();

        assert_eq!(
            built_modules, loaded_modules,
            "build_initial_state() modules differ from dev_advanced_state.json.\n\
             Built: {built_modules:?}\n\
             Loaded: {loaded_modules:?}"
        );
    }

    #[test]
    fn events_json_loads_and_validates() {
        let content = load_content("../../content").unwrap();
        assert_eq!(content.events.len(), 6, "Expected 6 event definitions");

        // Verify all event IDs are present
        let ids: Vec<&str> = content.events.iter().map(|e| e.id.0.as_str()).collect();
        assert!(ids.contains(&"evt_equipment_failure"));
        assert!(ids.contains(&"evt_comet_flyby"));
        assert!(ids.contains(&"evt_supernova"));
        assert!(ids.contains(&"evt_solar_flare"));
        assert!(ids.contains(&"evt_micrometeorite"));
        assert!(ids.contains(&"evt_supply_cache"));

        // Verify weights were resolved
        for event in &content.events {
            assert!(
                event.resolved_weight > 0,
                "event '{}' should have resolved_weight > 0",
                event.id.0
            );
        }

        // Verify constants loaded
        assert!(content.constants.events_enabled);
        assert_eq!(content.constants.event_global_cooldown_ticks, 200);
        assert_eq!(content.constants.event_history_capacity, 100);
    }

    #[test]
    fn test_load_hull_defs_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let hull_json = r#"[
            {
                "id": "hull_test",
                "name": "Test Hull",
                "mass_kg": 5000.0,
                "cargo_capacity_m3": 50.0,
                "base_speed_ticks_per_au": 120,
                "base_propellant_capacity_kg": 10000.0,
                "slots": [
                    { "slot_type": "utility", "label": "Utility 1" }
                ]
            }
        ]"#;
        std::fs::write(dir.path().join("hull_defs.json"), hull_json).unwrap();
        let hulls = load_hull_defs(dir.path()).unwrap();
        assert_eq!(hulls.len(), 1);
        let hull = &hulls[&sim_core::HullId("hull_test".to_string())];
        assert_eq!(hull.name, "Test Hull");
        assert_eq!(hull.slots.len(), 1);
        assert_eq!(
            hull.slots[0].slot_type,
            sim_core::SlotType("utility".to_string())
        );
        assert_eq!(hull.base_propellant_capacity_kg, 10000.0);
    }

    #[test]
    #[should_panic(expected = "duplicate hull ID")]
    fn test_load_hull_defs_duplicate_panics() {
        let dir = tempfile::tempdir().unwrap();
        let hull_json = r#"[
            {
                "id": "hull_dup",
                "name": "Hull A",
                "mass_kg": 1000.0,
                "cargo_capacity_m3": 10.0,
                "base_speed_ticks_per_au": 100,
                "base_propellant_capacity_kg": 5000.0,
                "slots": []
            },
            {
                "id": "hull_dup",
                "name": "Hull B",
                "mass_kg": 2000.0,
                "cargo_capacity_m3": 20.0,
                "base_speed_ticks_per_au": 200,
                "base_propellant_capacity_kg": 8000.0,
                "slots": []
            }
        ]"#;
        std::fs::write(dir.path().join("hull_defs.json"), hull_json).unwrap();
        let _ = load_hull_defs(dir.path()).unwrap();
    }

    #[test]
    fn test_load_hull_defs_missing_file_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        // No hull_defs.json written
        let hulls = load_hull_defs(dir.path()).unwrap();
        assert!(hulls.is_empty());
    }

    #[test]
    fn test_validate_hull_defs_no_panic_on_empty() {
        let content = base_content();
        // Should not panic — empty hulls is valid
        validate_hull_defs(&content);
    }

    #[test]
    #[should_panic(expected = "references unknown module")]
    fn test_fitting_template_bad_module_panics() {
        let mut content = base_content();
        content.hulls.insert(
            sim_core::HullId("hull_test".to_string()),
            sim_core::HullDef {
                id: sim_core::HullId("hull_test".to_string()),
                name: "Test".to_string(),
                mass_kg: 1000.0,
                cargo_capacity_m3: 10.0,
                base_speed_ticks_per_au: 100,
                base_propellant_capacity_kg: 5000.0,
                slots: vec![sim_core::SlotDef {
                    slot_type: sim_core::SlotType("utility".to_string()),
                    label: "Utility 1".to_string(),
                }],
                bonuses: vec![],
                required_tech: None,
                tags: vec![],
            },
        );
        content.fitting_templates.insert(
            sim_core::HullId("hull_test".to_string()),
            vec![sim_core::FittedModule {
                slot_index: 0,
                module_def_id: sim_core::ModuleDefId("nonexistent_module".to_string()),
            }],
        );
        validate_content(&content);
    }

    #[test]
    #[should_panic(expected = "slot_index")]
    fn test_fitting_template_bad_slot_index_panics() {
        let mut content = base_content();
        content.hulls.insert(
            sim_core::HullId("hull_test".to_string()),
            sim_core::HullDef {
                id: sim_core::HullId("hull_test".to_string()),
                name: "Test".to_string(),
                mass_kg: 1000.0,
                cargo_capacity_m3: 10.0,
                base_speed_ticks_per_au: 100,
                base_propellant_capacity_kg: 5000.0,
                slots: vec![sim_core::SlotDef {
                    slot_type: sim_core::SlotType("utility".to_string()),
                    label: "Utility 1".to_string(),
                }],
                bonuses: vec![],
                required_tech: None,
                tags: vec![],
            },
        );
        // Add a valid module def
        content.module_defs.insert(
            "mod_valid".to_string(),
            ModuleDefBuilder::new("mod_valid")
                .name("Valid")
                .mass(100.0)
                .volume(1.0)
                .behavior(sim_core::ModuleBehaviorDef::Equipment)
                .compatible_slots(vec![sim_core::SlotType("utility".to_string())])
                .build(),
        );
        content.fitting_templates.insert(
            sim_core::HullId("hull_test".to_string()),
            vec![sim_core::FittedModule {
                slot_index: 99, // out of bounds
                module_def_id: sim_core::ModuleDefId("mod_valid".to_string()),
            }],
        );
        validate_content(&content);
    }

    #[test]
    fn scoring_config_loads_from_content() {
        let content = load_content("../../content").expect("load_content failed");
        assert_eq!(content.scoring.dimensions.len(), 6);
        assert_eq!(content.scoring.thresholds.len(), 5);
        assert_eq!(content.scoring.computation_interval_ticks, 24);
        assert!(
            sim_core::validate_scoring_config(&content.scoring).is_ok(),
            "scoring config should be valid"
        );
    }

    #[test]
    #[should_panic(expected = "invalid scoring config")]
    fn scoring_config_bad_weights_panics() {
        let mut content = minimal_content();
        content.scoring = sim_core::ScoringConfig {
            dimensions: vec![
                sim_core::DimensionDef {
                    id: "a".into(),
                    name: "A".into(),
                    weight: 0.6,
                    ceiling: 1.0,
                },
                sim_core::DimensionDef {
                    id: "b".into(),
                    name: "B".into(),
                    weight: 0.6, // sums to 1.2
                    ceiling: 1.0,
                },
            ],
            thresholds: vec![sim_core::ThresholdDef {
                name: "Start".into(),
                min_score: 0.0,
            }],
            computation_interval_ticks: 24,
            scale_factor: 2500.0,
        };
        validate_content(&content);
    }

    #[test]
    fn milestones_load_from_content() {
        let content = load_content("../../content").unwrap();
        assert!(
            content.milestones.len() >= 8,
            "expected at least 8 milestones, got {}",
            content.milestones.len()
        );
        let first = &content.milestones[0];
        assert_eq!(first.id, "first_survey");
        assert!(!first.conditions.is_empty());
    }

    #[test]
    fn milestones_serde_round_trip() {
        let milestone = sim_core::MilestoneDef {
            id: "test_milestone".to_string(),
            name: "Test".to_string(),
            description: "A test milestone".to_string(),
            conditions: vec![
                sim_core::MilestoneCondition::MetricAbove {
                    field: "total_ore_kg".to_string(),
                    threshold: 100.0,
                },
                sim_core::MilestoneCondition::CounterAbove {
                    counter: "asteroids_discovered".to_string(),
                    threshold: 1.0,
                },
                sim_core::MilestoneCondition::MilestoneCompleted {
                    milestone_id: "test_milestone".to_string(),
                },
            ],
            rewards: sim_core::MilestoneReward {
                grant_amount: 5_000_000.0,
                reputation: 10.0,
                unlock_trade_tier: Some(sim_core::TradeTier::BasicImport),
                unlock_zone_ids: vec!["inner_belt".to_string()],
                unlock_module_ids: vec![],
            },
            phase_advance: Some(sim_core::GamePhase::Orbital),
        };
        let json = serde_json::to_string(&milestone).unwrap();
        let roundtripped: sim_core::MilestoneDef = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtripped.id, "test_milestone");
        assert_eq!(roundtripped.conditions.len(), 3);
        assert_eq!(
            roundtripped.phase_advance,
            Some(sim_core::GamePhase::Orbital)
        );
    }

    #[test]
    fn validate_milestones_accepts_valid() {
        let milestones = vec![
            sim_core::MilestoneDef {
                id: "m1".to_string(),
                name: "M1".to_string(),
                description: String::new(),
                conditions: vec![],
                rewards: sim_core::MilestoneReward {
                    grant_amount: 0.0,
                    reputation: 0.0,
                    unlock_trade_tier: None,
                    unlock_zone_ids: vec![],
                    unlock_module_ids: vec![],
                },
                phase_advance: None,
            },
            sim_core::MilestoneDef {
                id: "m2".to_string(),
                name: "M2".to_string(),
                description: String::new(),
                conditions: vec![sim_core::MilestoneCondition::MilestoneCompleted {
                    milestone_id: "m1".to_string(),
                }],
                rewards: sim_core::MilestoneReward {
                    grant_amount: 0.0,
                    reputation: 0.0,
                    unlock_trade_tier: None,
                    unlock_zone_ids: vec![],
                    unlock_module_ids: vec![],
                },
                phase_advance: None,
            },
        ];
        validate_milestones(&milestones); // should not panic
    }

    #[test]
    #[should_panic(expected = "duplicate milestone id")]
    fn validate_milestones_rejects_duplicate_ids() {
        let milestones = vec![
            sim_core::MilestoneDef {
                id: "dup".to_string(),
                name: "D1".to_string(),
                description: String::new(),
                conditions: vec![],
                rewards: sim_core::MilestoneReward {
                    grant_amount: 0.0,
                    reputation: 0.0,
                    unlock_trade_tier: None,
                    unlock_zone_ids: vec![],
                    unlock_module_ids: vec![],
                },
                phase_advance: None,
            },
            sim_core::MilestoneDef {
                id: "dup".to_string(),
                name: "D2".to_string(),
                description: String::new(),
                conditions: vec![],
                rewards: sim_core::MilestoneReward {
                    grant_amount: 0.0,
                    reputation: 0.0,
                    unlock_trade_tier: None,
                    unlock_zone_ids: vec![],
                    unlock_module_ids: vec![],
                },
                phase_advance: None,
            },
        ];
        validate_milestones(&milestones);
    }

    #[test]
    #[should_panic(expected = "unknown prerequisite milestone")]
    fn validate_milestones_rejects_unknown_prereq() {
        let milestones = vec![sim_core::MilestoneDef {
            id: "m1".to_string(),
            name: "M1".to_string(),
            description: String::new(),
            conditions: vec![sim_core::MilestoneCondition::MilestoneCompleted {
                milestone_id: "nonexistent".to_string(),
            }],
            rewards: sim_core::MilestoneReward {
                grant_amount: 0.0,
                reputation: 0.0,
                unlock_trade_tier: None,
                unlock_zone_ids: vec![],
                unlock_module_ids: vec![],
            },
            phase_advance: None,
        }];
        validate_milestones(&milestones);
    }

    #[test]
    fn trade_tier_ordering() {
        assert!(sim_core::TradeTier::None < sim_core::TradeTier::BasicImport);
        assert!(sim_core::TradeTier::BasicImport < sim_core::TradeTier::Export);
        assert!(sim_core::TradeTier::Export < sim_core::TradeTier::Full);
    }
}
