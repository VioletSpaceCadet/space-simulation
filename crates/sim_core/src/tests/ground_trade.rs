use super::*;
use crate::test_fixtures::{base_content, base_state, make_rng};
use std::collections::HashMap;

fn trade_content() -> GameContent {
    let mut content = base_content();
    content.pricing.items.insert(
        "Fe".to_string(),
        PricingEntry {
            base_price_per_unit: 50.0,
            importable: true,
            exportable: true,
            category: String::new(),
        },
    );
    content
}

fn add_ground_facility(state: &mut GameState) -> GroundFacilityId {
    let gf_id = GroundFacilityId("gf_trade_test".to_string());
    let gf = GroundFacilityState {
        id: gf_id.clone(),
        name: "Trade Test Facility".to_string(),
        position: crate::test_fixtures::test_position(),
        core: FacilityCore {
            modules: vec![],
            inventory: vec![],
            cargo_capacity_m3: 1000.0,
            power_available_per_tick: 100.0,
            modifiers: crate::modifiers::ModifierSet::default(),
            crew: std::collections::BTreeMap::new(),
            thermal_links: Vec::new(),
            power: PowerState::default(),
            cached_inventory_volume_m3: None,
            module_type_index: ModuleTypeIndex::default(),
            module_id_index: HashMap::new(),
            power_budget_cache: PowerBudgetCache::default(),
        },
        launch_transits: Vec::new(),
    };
    state.ground_facilities.insert(gf_id.clone(), gf);
    gf_id
}

fn import_command(gf_id: &GroundFacilityId) -> Command {
    Command::Import {
        facility_id: FacilityId::Ground(gf_id.clone()),
        item_spec: TradeItemSpec::Material {
            element: "Fe".to_string(),
            kg: 100.0,
        },
    }
}

fn make_envelope(command: Command) -> CommandEnvelope {
    CommandEnvelope {
        id: CommandId(0),
        issued_by: crate::PrincipalId("test".to_string()),
        issued_tick: 0,
        execute_at_tick: 0,
        command,
    }
}

#[test]
fn ground_import_bypasses_trade_tier() {
    let content = trade_content();
    let mut state = base_state(&content);
    state.balance = 1_000_000.0;
    // Ensure trade tier is NOT unlocked for stations
    assert!(!state
        .progression
        .trade_tier_unlocked(TradeTier::BasicImport));

    let gf_id = add_ground_facility(&mut state);
    let cmd = make_envelope(import_command(&gf_id));
    let mut rng = make_rng();

    let events = tick(&mut state, &[cmd], &content, &mut rng, None);

    // Ground import should succeed despite no trade tier
    let imported = events
        .iter()
        .any(|e| matches!(&e.event, Event::ItemImported { .. }));
    assert!(imported, "ground facility import should bypass trade tier");
}

#[test]
fn ground_import_insufficient_funds_emits_event() {
    let content = trade_content();
    let mut state = base_state(&content);
    state.balance = 0.01; // Not enough for any import
    let gf_id = add_ground_facility(&mut state);
    let cmd = make_envelope(import_command(&gf_id));
    let mut rng = make_rng();

    let events = tick(&mut state, &[cmd], &content, &mut rng, None);

    let insufficient = events
        .iter()
        .any(|e| matches!(&e.event, Event::InsufficientFunds { .. }));
    assert!(
        insufficient,
        "should emit InsufficientFunds when balance too low"
    );
}

#[test]
fn ground_export_bypasses_trade_tier() {
    let content = trade_content();
    let mut state = base_state(&content);
    state.balance = 1_000_000.0;
    assert!(!state.progression.trade_tier_unlocked(TradeTier::Export));

    let gf_id = add_ground_facility(&mut state);
    // Add exportable material to ground facility
    state
        .ground_facilities
        .get_mut(&gf_id)
        .unwrap()
        .core
        .inventory
        .push(InventoryItem::Material {
            element: "Fe".to_string(),
            kg: 500.0,
            quality: 0.8,
            thermal: None,
        });

    let cmd = make_envelope(Command::Export {
        facility_id: FacilityId::Ground(gf_id.clone()),
        item_spec: TradeItemSpec::Material {
            element: "Fe".to_string(),
            kg: 100.0,
        },
    });
    let mut rng = make_rng();

    let events = tick(&mut state, &[cmd], &content, &mut rng, None);

    let exported = events
        .iter()
        .any(|e| matches!(&e.event, Event::ItemExported { .. }));
    assert!(exported, "ground facility export should bypass trade tier");
}

#[test]
fn station_import_still_requires_trade_tier() {
    let content = trade_content();
    let mut state = base_state(&content);
    state.balance = 1_000_000.0;
    assert!(!state
        .progression
        .trade_tier_unlocked(TradeTier::BasicImport));

    let station_id = crate::test_fixtures::test_station_id();
    let cmd = make_envelope(Command::Import {
        facility_id: FacilityId::Station(station_id),
        item_spec: TradeItemSpec::Material {
            element: "Fe".to_string(),
            kg: 100.0,
        },
    });
    let mut rng = make_rng();

    let events = tick(&mut state, &[cmd], &content, &mut rng, None);

    let imported = events
        .iter()
        .any(|e| matches!(&e.event, Event::ItemImported { .. }));
    assert!(
        !imported,
        "station import should still require trade tier unlock"
    );
}
