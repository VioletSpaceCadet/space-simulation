import { describe, expect, it } from 'vitest';

import type { AsteroidState, ComponentItem, MaterialItem, ModuleItem, OreItem, ResearchState, ScanSite, ShipState, SlagItem, StationState, TradeItemSpec } from '../types';

import { applyEvents } from './applyEvents';

const emptyResearch: ResearchState = { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} };
const defaultBalance = 1_000_000_000;

function makeShip(overrides: Partial<ShipState> = {}): ShipState {
  return {
    id: 'ship_0001',
    location_node: 'node_a',
    owner: 'player',
    inventory: [],
    cargo_capacity_m3: 20,
    task: null,
    ...overrides,
  };
}

function makeAsteroid(overrides: Partial<AsteroidState> = {}): AsteroidState {
  return {
    id: 'ast_001',
    location_node: 'node_a',
    anomaly_tags: [],
    mass_kg: 500,
    knowledge: { tag_beliefs: [], composition: null },
    ...overrides,
  };
}

function makeStation(overrides: Partial<StationState> = {}): StationState {
  return {
    id: 'station_001',
    location_node: 'node_a',
    power_available_per_tick: 10,
    inventory: [],
    cargo_capacity_m3: 100,
    modules: [],
    power: {
      generated_kw: 0, consumed_kw: 0, deficit_kw: 0,
      battery_discharge_kw: 0, battery_charge_kw: 0, battery_stored_kwh: 0,
    },
    ...overrides,
  };
}

function makeOreLot(overrides: Partial<OreItem> = {}): OreItem {
  return {
    kind: 'Ore',
    lot_id: 'lot_1',
    asteroid_id: 'ast_001',
    kg: 50,
    composition: { Fe: 0.7, Si: 0.3 },
    ...overrides,
  };
}

describe('applyEvents', () => {
  describe('OreMined', () => {
    it('updates ship inventory and asteroid mass', () => {
      const ship = makeShip();
      const asteroid = makeAsteroid({ mass_kg: 500 });
      const oreLot = makeOreLot();

      const events = [{
        id: 'e1',
        tick: 10,
        event: {
          OreMined: {
            ship_id: 'ship_0001',
            asteroid_id: 'ast_001',
            ore_lot: oreLot,
            asteroid_remaining_kg: 450,
          },
        },
      }];

      const result = applyEvents(
        { ast_001: asteroid },
        { ship_0001: ship },
        {},
        emptyResearch,
        [],
        defaultBalance,
        events,
      );

      expect(result.ships['ship_0001'].inventory).toHaveLength(1);
      expect(result.ships['ship_0001'].inventory[0]).toEqual(oreLot);
      expect(result.asteroids['ast_001'].mass_kg).toBe(450);
    });

    it('removes depleted asteroid when asteroid_remaining_kg is zero', () => {
      const ship = makeShip();
      const asteroid = makeAsteroid({ mass_kg: 10 });
      const oreLot = makeOreLot({ kg: 10 });

      const events = [{
        id: 'e1',
        tick: 10,
        event: {
          OreMined: {
            ship_id: 'ship_0001',
            asteroid_id: 'ast_001',
            ore_lot: oreLot,
            asteroid_remaining_kg: 0,
          },
        },
      }];

      const result = applyEvents(
        { ast_001: asteroid },
        { ship_0001: ship },
        {},
        emptyResearch,
        [],
        defaultBalance,
        events,
      );

      expect(result.asteroids['ast_001']).toBeUndefined();
      // Ship still gets the ore even though asteroid is depleted
      expect(result.ships['ship_0001'].inventory).toHaveLength(1);
      expect(result.ships['ship_0001'].inventory[0]).toEqual(oreLot);
    });
  });

  describe('OreDeposited', () => {
    it('clears ship inventory and adds items to station', () => {
      const oreLot = makeOreLot();
      const ship = makeShip({ inventory: [oreLot] });
      const station = makeStation();

      const depositedItems = [makeOreLot({ lot_id: 'lot_1', kg: 50 })];

      const events = [{
        id: 'e1',
        tick: 20,
        event: {
          OreDeposited: {
            ship_id: 'ship_0001',
            station_id: 'station_001',
            items: depositedItems,
          },
        },
      }];

      const result = applyEvents(
        {},
        { ship_0001: ship },
        { station_001: station },
        emptyResearch,
        [],
        defaultBalance,
        events,
      );

      expect(result.ships['ship_0001'].inventory).toEqual([]);
      expect(result.stations['station_001'].inventory).toHaveLength(1);
      expect(result.stations['station_001'].inventory[0]).toEqual(depositedItems[0]);
    });

    it('appends to existing station inventory', () => {
      const existingOre = makeOreLot({ lot_id: 'lot_existing', kg: 100 });
      const station = makeStation({ inventory: [existingOre] });
      const ship = makeShip({ inventory: [makeOreLot()] });

      const newOre = makeOreLot({ lot_id: 'lot_new', kg: 75 });

      const events = [{
        id: 'e1',
        tick: 20,
        event: {
          OreDeposited: {
            ship_id: 'ship_0001',
            station_id: 'station_001',
            items: [newOre],
          },
        },
      }];

      const result = applyEvents(
        {},
        { ship_0001: ship },
        { station_001: station },
        emptyResearch,
        [],
        defaultBalance,
        events,
      );

      expect(result.stations['station_001'].inventory).toHaveLength(2);
      expect(result.stations['station_001'].inventory[0]).toEqual(existingOre);
      expect(result.stations['station_001'].inventory[1]).toEqual(newOre);
      expect(result.ships['ship_0001'].inventory).toEqual([]);
    });
  });

  describe('ItemImported', () => {
    it('updates balance and adds material to station inventory', () => {
      const station = makeStation();
      const itemSpec: TradeItemSpec = { Material: { element: 'Fe', kg: 100 } };
      const events = [{
        id: 'e1',
        tick: 10,
        event: {
          ItemImported: {
            station_id: 'station_001',
            item_spec: itemSpec,
            cost: 5000,
            balance_after: 999_995_000,
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);

      expect(result.balance).toBe(999_995_000);
      expect(result.stations['station_001'].inventory).toHaveLength(1);
      const item = result.stations['station_001'].inventory[0] as MaterialItem;
      expect(item.kind).toBe('Material');
      expect(item.element).toBe('Fe');
      expect(item.kg).toBe(100);
    });

    it('merges imported material with existing stock', () => {
      const station = makeStation({
        inventory: [{ kind: 'Material', element: 'Fe', kg: 50, quality: 1.0 }],
      });
      const itemSpec: TradeItemSpec = { Material: { element: 'Fe', kg: 100 } };
      const events = [{
        id: 'e1',
        tick: 10,
        event: {
          ItemImported: {
            station_id: 'station_001',
            item_spec: itemSpec,
            cost: 5000,
            balance_after: 999_995_000,
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);

      expect(result.stations['station_001'].inventory).toHaveLength(1);
      const item = result.stations['station_001'].inventory[0] as MaterialItem;
      expect(item.kg).toBe(150);
    });

    it('merges imported component with existing stock', () => {
      const station = makeStation({
        inventory: [{ kind: 'Component', component_id: 'thruster', count: 2, quality: 1.0 }],
      });
      const itemSpec: TradeItemSpec = { Component: { component_id: 'thruster', count: 3 } };
      const events = [{
        id: 'e1',
        tick: 10,
        event: {
          ItemImported: {
            station_id: 'station_001',
            item_spec: itemSpec,
            cost: 10000,
            balance_after: 999_990_000,
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);

      expect(result.stations['station_001'].inventory).toHaveLength(1);
      const item = result.stations['station_001'].inventory[0] as ComponentItem;
      expect(item.count).toBe(5);
    });

    it('adds module as new inventory item', () => {
      const station = makeStation();
      const itemSpec: TradeItemSpec = { Module: { module_def_id: 'module_refinery' } };
      const events = [{
        id: 'e1',
        tick: 10,
        event: {
          ItemImported: {
            station_id: 'station_001',
            item_spec: itemSpec,
            cost: 50000,
            balance_after: 999_950_000,
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);

      expect(result.balance).toBe(999_950_000);
      expect(result.stations['station_001'].inventory).toHaveLength(1);
      expect(result.stations['station_001'].inventory[0].kind).toBe('Module');
    });
  });

  describe('ItemExported', () => {
    it('updates balance and removes material from station inventory', () => {
      const station = makeStation({
        inventory: [{ kind: 'Material', element: 'Fe', kg: 100, quality: 1.0 }],
      });
      const itemSpec: TradeItemSpec = { Material: { element: 'Fe', kg: 50 } };
      const events = [{
        id: 'e1',
        tick: 10,
        event: {
          ItemExported: {
            station_id: 'station_001',
            item_spec: itemSpec,
            revenue: 3000,
            balance_after: 1_000_003_000,
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);

      expect(result.balance).toBe(1_000_003_000);
      expect(result.stations['station_001'].inventory).toHaveLength(1);
      const item = result.stations['station_001'].inventory[0] as MaterialItem;
      expect(item.kg).toBe(50);
    });

    it('removes material entry when fully exported', () => {
      const station = makeStation({
        inventory: [{ kind: 'Material', element: 'Fe', kg: 100, quality: 1.0 }],
      });
      const itemSpec: TradeItemSpec = { Material: { element: 'Fe', kg: 100 } };
      const events = [{
        id: 'e1',
        tick: 10,
        event: {
          ItemExported: {
            station_id: 'station_001',
            item_spec: itemSpec,
            revenue: 6000,
            balance_after: 1_000_006_000,
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);

      expect(result.stations['station_001'].inventory).toHaveLength(0);
    });

    it('removes component from inventory', () => {
      const station = makeStation({
        inventory: [{ kind: 'Component', component_id: 'repair_kit', count: 5, quality: 1.0 }],
      });
      const itemSpec: TradeItemSpec = { Component: { component_id: 'repair_kit', count: 2 } };
      const events = [{
        id: 'e1',
        tick: 10,
        event: {
          ItemExported: {
            station_id: 'station_001',
            item_spec: itemSpec,
            revenue: 1000,
            balance_after: 1_000_001_000,
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);

      expect(result.stations['station_001'].inventory).toHaveLength(1);
      const item = result.stations['station_001'].inventory[0] as ComponentItem;
      expect(item.count).toBe(3);
    });
  });

  describe('SlagJettisoned', () => {
    it('removes all slag from station inventory', () => {
      const station = makeStation({
        inventory: [
          { kind: 'Slag', kg: 50, composition: {} } as SlagItem,
          { kind: 'Material', element: 'Fe', kg: 100, quality: 1.0 },
          { kind: 'Slag', kg: 30, composition: {} } as SlagItem,
        ],
      });
      const events = [{
        id: 'e1',
        tick: 10,
        event: { SlagJettisoned: { station_id: 'station_001', kg: 80 } },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);

      expect(result.stations['station_001'].inventory).toHaveLength(1);
      expect(result.stations['station_001'].inventory[0].kind).toBe('Material');
    });
  });

  describe('LabRan', () => {
    it('resets ticks_since_last_run, sets assigned_tech, and clears starved', () => {
      const station = makeStation({
        modules: [{
          id: 'mod_lab', def_id: 'module_lab', enabled: true,
          kind_state: { Lab: { ticks_since_last_run: 5, assigned_tech: 'tech_a', starved: true } },
          wear: { wear: 0 },
        }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: {
          LabRan: {
            station_id: 'station_001', module_id: 'mod_lab',
            tech_id: 'tech_b', data_consumed: 4.0, points_produced: 2.0,
            domain: 'Materials',
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      const lab = result.stations['station_001'].modules[0];
      expect(lab.kind_state).toEqual({
        Lab: { ticks_since_last_run: 0, assigned_tech: 'tech_b', starved: false },
      });
    });
  });

  describe('LabStarved', () => {
    it('sets starved to true on the lab module', () => {
      const station = makeStation({
        modules: [{
          id: 'mod_lab', def_id: 'module_lab', enabled: true,
          kind_state: { Lab: { ticks_since_last_run: 3, assigned_tech: 'tech_a', starved: false } },
          wear: { wear: 0 },
        }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: { LabStarved: { station_id: 'station_001', module_id: 'mod_lab' } },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      const lab = result.stations['station_001'].modules[0];
      expect(lab.kind_state).toEqual({
        Lab: { ticks_since_last_run: 3, assigned_tech: 'tech_a', starved: true },
      });
    });
  });

  describe('LabResumed', () => {
    it('sets starved to false on the lab module', () => {
      const station = makeStation({
        modules: [{
          id: 'mod_lab', def_id: 'module_lab', enabled: true,
          kind_state: { Lab: { ticks_since_last_run: 3, assigned_tech: 'tech_a', starved: true } },
          wear: { wear: 0 },
        }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: { LabResumed: { station_id: 'station_001', module_id: 'mod_lab' } },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      const lab = result.stations['station_001'].modules[0];
      expect(lab.kind_state).toEqual({
        Lab: { ticks_since_last_run: 3, assigned_tech: 'tech_a', starved: false },
      });
    });
  });

  describe('ModuleUninstalled', () => {
    it('removes module from station and adds item to inventory', () => {
      const station = makeStation({
        modules: [{
          id: 'mod_proc', def_id: 'module_refinery', enabled: true,
          kind_state: { Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false } },
          wear: { wear: 0.2 },
        }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: {
          ModuleUninstalled: {
            station_id: 'station_001', module_id: 'mod_proc', module_item_id: 'item_001',
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      expect(result.stations['station_001'].modules).toHaveLength(0);
      expect(result.stations['station_001'].inventory).toHaveLength(1);
      const item = result.stations['station_001'].inventory[0] as ModuleItem;
      expect(item.kind).toBe('Module');
      expect(item.item_id).toBe('item_001');
      expect(item.module_def_id).toBe('module_refinery');
    });
  });

  describe('ModuleStalled', () => {
    it('sets stalled to true on Processor module', () => {
      const station = makeStation({
        modules: [{
          id: 'mod_proc', def_id: 'module_refinery', enabled: true,
          kind_state: { Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false } },
          wear: { wear: 0 },
        }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: { ModuleStalled: { station_id: 'station_001', module_id: 'mod_proc', shortfall_m3: 5.0 } },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      const mod = result.stations['station_001'].modules[0];
      expect(mod.kind_state).toEqual({ Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: true } });
    });

    it('sets stalled to true on Assembler module', () => {
      const station = makeStation({
        modules: [{
          id: 'mod_asm', def_id: 'module_assembler', enabled: true,
          kind_state: { Assembler: { ticks_since_last_run: 0, stalled: false, capped: false, cap_override: {} } },
          wear: { wear: 0 },
        }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: { ModuleStalled: { station_id: 'station_001', module_id: 'mod_asm', shortfall_m3: 3.0 } },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      const mod = result.stations['station_001'].modules[0];
      expect(mod.kind_state).toEqual({
        Assembler: { ticks_since_last_run: 0, stalled: true, capped: false, cap_override: {} },
      });
    });
  });

  describe('ModuleResumed', () => {
    it('sets stalled to false on Processor module', () => {
      const station = makeStation({
        modules: [{
          id: 'mod_proc', def_id: 'module_refinery', enabled: true,
          kind_state: { Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: true } },
          wear: { wear: 0 },
        }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: { ModuleResumed: { station_id: 'station_001', module_id: 'mod_proc' } },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      const mod = result.stations['station_001'].modules[0];
      expect(mod.kind_state).toEqual({ Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false } });
    });

    it('sets stalled to false on Assembler module', () => {
      const station = makeStation({
        modules: [{
          id: 'mod_asm', def_id: 'module_assembler', enabled: true,
          kind_state: { Assembler: { ticks_since_last_run: 0, stalled: true, capped: false, cap_override: {} } },
          wear: { wear: 0 },
        }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: { ModuleResumed: { station_id: 'station_001', module_id: 'mod_asm' } },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      const mod = result.stations['station_001'].modules[0];
      expect(mod.kind_state).toEqual({
        Assembler: { ticks_since_last_run: 0, stalled: false, capped: false, cap_override: {} },
      });
    });
  });

  describe('AssemblerCapped', () => {
    it('sets capped to true on assembler module', () => {
      const station = makeStation({
        modules: [{
          id: 'mod_asm', def_id: 'module_assembler', enabled: true,
          kind_state: { Assembler: { ticks_since_last_run: 0, stalled: false, capped: false, cap_override: {} } },
          wear: { wear: 0 },
        }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: { AssemblerCapped: { station_id: 'station_001', module_id: 'mod_asm' } },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      const mod = result.stations['station_001'].modules[0];
      expect(mod.kind_state).toEqual({
        Assembler: { ticks_since_last_run: 0, stalled: false, capped: true, cap_override: {} },
      });
    });
  });

  describe('AssemblerUncapped', () => {
    it('sets capped to false on assembler module', () => {
      const station = makeStation({
        modules: [{
          id: 'mod_asm', def_id: 'module_assembler', enabled: true,
          kind_state: { Assembler: { ticks_since_last_run: 0, stalled: false, capped: true, cap_override: {} } },
          wear: { wear: 0 },
        }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: { AssemblerUncapped: { station_id: 'station_001', module_id: 'mod_asm' } },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      const mod = result.stations['station_001'].modules[0];
      expect(mod.kind_state).toEqual({
        Assembler: { ticks_since_last_run: 0, stalled: false, capped: false, cap_override: {} },
      });
    });
  });

  describe('DepositBlocked', () => {
    it('sets blocked to true on ship deposit task', () => {
      const ship = makeShip({
        task: {
          kind: { Deposit: { station: 'station_001', blocked: false } },
          started_tick: 5, eta_tick: 10,
        },
      });

      const events = [{
        id: 'e1', tick: 10,
        event: { DepositBlocked: { ship_id: 'ship_0001', station_id: 'station_001', shortfall_m3: 5.0 } },
      }];

      const result = applyEvents({}, { ship_0001: ship }, {}, emptyResearch, [], defaultBalance, events);
      const task = result.ships['ship_0001'].task!;
      expect(task.kind).toEqual({ Deposit: { station: 'station_001', blocked: true } });
    });
  });

  describe('DepositUnblocked', () => {
    it('sets blocked to false on ship deposit task', () => {
      const ship = makeShip({
        task: {
          kind: { Deposit: { station: 'station_001', blocked: true } },
          started_tick: 5, eta_tick: 10,
        },
      });

      const events = [{
        id: 'e1', tick: 10,
        event: { DepositUnblocked: { ship_id: 'ship_0001', station_id: 'station_001' } },
      }];

      const result = applyEvents({}, { ship_0001: ship }, {}, emptyResearch, [], defaultBalance, events);
      const task = result.ships['ship_0001'].task!;
      expect(task.kind).toEqual({ Deposit: { station: 'station_001', blocked: false } });
    });
  });

  describe('PowerStateUpdated', () => {
    it('updates station power state', () => {
      const station = makeStation();
      const newPower = {
        generated_kw: 50, consumed_kw: 30, deficit_kw: 0,
        battery_discharge_kw: 0, battery_charge_kw: 20, battery_stored_kwh: 100,
      };

      const events = [{
        id: 'e1', tick: 10,
        event: { PowerStateUpdated: { station_id: 'station_001', power: newPower } },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      expect(result.stations['station_001'].power).toEqual(newPower);
    });
  });

  describe('InsufficientFunds', () => {
    it('does not change state', () => {
      const station = makeStation();
      const events = [{
        id: 'e1',
        tick: 10,
        event: {
          InsufficientFunds: {
            station_id: 'station_001',
            action: 'Import Fe 100kg',
            required: 5000,
            available: 100,
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);

      expect(result.balance).toBe(defaultBalance);
      expect(result.stations['station_001'].inventory).toHaveLength(0);
    });
  });

  describe('AsteroidDiscovered', () => {
    it('adds a new asteroid to state', () => {
      const events = [{
        id: 'e1', tick: 5,
        event: { AsteroidDiscovered: { asteroid_id: 'ast_new', location_node: 'node_b' } },
      }];

      const result = applyEvents({}, {}, {}, emptyResearch, [], defaultBalance, events);

      expect(result.asteroids['ast_new']).toBeDefined();
      expect(result.asteroids['ast_new'].id).toBe('ast_new');
      expect(result.asteroids['ast_new'].location_node).toBe('node_b');
      expect(result.asteroids['ast_new'].knowledge.tag_beliefs).toEqual([]);
      expect(result.asteroids['ast_new'].knowledge.composition).toBeNull();
    });

    it('does not overwrite an existing asteroid', () => {
      const existing = makeAsteroid({ id: 'ast_001', mass_kg: 999 });
      const events = [{
        id: 'e1', tick: 5,
        event: { AsteroidDiscovered: { asteroid_id: 'ast_001', location_node: 'node_b' } },
      }];

      const result = applyEvents(
        { ast_001: existing }, {}, {}, emptyResearch, [], defaultBalance, events,
      );

      expect(result.asteroids['ast_001'].mass_kg).toBe(999);
    });
  });

  describe('ModuleInstalled', () => {
    it('adds Processor module and removes item from inventory', () => {
      const station = makeStation({
        inventory: [
          { kind: 'Module', item_id: 'item_ref', module_def_id: 'module_refinery' } as ModuleItem,
        ],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: {
          ModuleInstalled: {
            station_id: 'station_001', module_id: 'mod_1',
            module_item_id: 'item_ref', module_def_id: 'module_refinery',
            behavior_type: 'Processor',
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);

      expect(result.stations['station_001'].modules).toHaveLength(1);
      expect(result.stations['station_001'].modules[0].id).toBe('mod_1');
      expect(result.stations['station_001'].modules[0].def_id).toBe('module_refinery');
      expect(result.stations['station_001'].modules[0].enabled).toBe(false);
      expect(result.stations['station_001'].modules[0].kind_state).toEqual({
        Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false },
      });
      expect(result.stations['station_001'].inventory).toHaveLength(0);
    });

    it('assigns correct kind_state for each behavior_type', () => {
      const behaviorTypes: Record<string, unknown> = {
        Processor: { Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false } },
        Storage: 'Storage',
        Maintenance: { Maintenance: { ticks_since_last_run: 0 } },
        Assembler: {
          Assembler: { ticks_since_last_run: 0, stalled: false, capped: false, cap_override: {} },
        },
        Lab: { Lab: { ticks_since_last_run: 0, assigned_tech: null, starved: false } },
        SensorArray: { SensorArray: { ticks_since_last_run: 0 } },
        SolarArray: { SolarArray: { ticks_since_last_run: 0 } },
        Battery: { Battery: { charge_kwh: 0 } },
      };

      for (const [behaviorType, expectedKindState] of Object.entries(behaviorTypes)) {
        const station = makeStation();
        const events = [{
          id: 'e1', tick: 10,
          event: {
            ModuleInstalled: {
              station_id: 'station_001', module_id: `mod_${behaviorType}`,
              module_item_id: 'item_1', module_def_id: `module_${behaviorType}`,
              behavior_type: behaviorType,
            },
          },
        }];

        const result = applyEvents(
          {}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events,
        );
        expect(result.stations['station_001'].modules[0].kind_state).toEqual(expectedKindState);
      }
    });

    it('falls back to Processor for unknown behavior_type', () => {
      const station = makeStation();
      const events = [{
        id: 'e1', tick: 10,
        event: {
          ModuleInstalled: {
            station_id: 'station_001', module_id: 'mod_unknown',
            module_item_id: 'item_1', module_def_id: 'module_custom',
            behavior_type: 'UnknownType',
          },
        },
      }];

      const result = applyEvents(
        {}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events,
      );
      expect(result.stations['station_001'].modules[0].kind_state).toEqual({
        Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false },
      });
    });
  });

  describe('ModuleToggled', () => {
    it('toggles module enabled to true', () => {
      const station = makeStation({
        modules: [{
          id: 'mod_1', def_id: 'module_refinery', enabled: false,
          kind_state: { Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false } },
          wear: { wear: 0 },
        }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: { ModuleToggled: { station_id: 'station_001', module_id: 'mod_1', enabled: true } },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      expect(result.stations['station_001'].modules[0].enabled).toBe(true);
    });

    it('toggles module enabled to false', () => {
      const station = makeStation({
        modules: [{
          id: 'mod_1', def_id: 'module_refinery', enabled: true,
          kind_state: { Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false } },
          wear: { wear: 0 },
        }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: { ModuleToggled: { station_id: 'station_001', module_id: 'mod_1', enabled: false } },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      expect(result.stations['station_001'].modules[0].enabled).toBe(false);
    });
  });

  describe('ModuleThresholdSet', () => {
    it('updates threshold_kg on Processor module', () => {
      const station = makeStation({
        modules: [{
          id: 'mod_1', def_id: 'module_refinery', enabled: true,
          kind_state: { Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false } },
          wear: { wear: 0 },
        }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: {
          ModuleThresholdSet: {
            station_id: 'station_001', module_id: 'mod_1', threshold_kg: 500,
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      const mod = result.stations['station_001'].modules[0];
      expect(mod.kind_state).toEqual({
        Processor: { threshold_kg: 500, ticks_since_last_run: 0, stalled: false },
      });
    });

    it('ignores non-Processor modules', () => {
      const station = makeStation({
        modules: [{
          id: 'mod_1', def_id: 'module_storage', enabled: true,
          kind_state: 'Storage',
          wear: { wear: 0 },
        }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: {
          ModuleThresholdSet: {
            station_id: 'station_001', module_id: 'mod_1', threshold_kg: 500,
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      expect(result.stations['station_001'].modules[0].kind_state).toBe('Storage');
    });
  });

  describe('RefineryRan', () => {
    it('consumes ore, produces material and slag', () => {
      const station = makeStation({
        inventory: [makeOreLot({ lot_id: 'lot_1', kg: 100 })],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: {
          RefineryRan: {
            station_id: 'station_001', module_id: 'mod_1',
            ore_consumed_kg: 30, material_produced_kg: 20,
            material_quality: 0.8, slag_produced_kg: 10,
            material_element: 'Fe',
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      const inv = result.stations['station_001'].inventory;

      // Ore reduced
      const ore = inv.find((i) => i.kind === 'Ore');
      expect(ore).toBeDefined();
      expect(ore!.kg).toBe(70);

      // Material produced
      const mat = inv.find((i) => i.kind === 'Material') as MaterialItem;
      expect(mat).toBeDefined();
      expect(mat.element).toBe('Fe');
      expect(mat.kg).toBe(20);
      expect(mat.quality).toBe(0.8);

      // Slag produced
      const slag = inv.find((i) => i.kind === 'Slag');
      expect(slag).toBeDefined();
      expect(slag!.kg).toBe(10);
    });

    it('merges material with existing stock', () => {
      const station = makeStation({
        inventory: [
          makeOreLot({ kg: 100 }),
          { kind: 'Material', element: 'Fe', kg: 50, quality: 1.0 },
        ],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: {
          RefineryRan: {
            station_id: 'station_001', module_id: 'mod_1',
            ore_consumed_kg: 20, material_produced_kg: 10,
            material_quality: 0.5, slag_produced_kg: 0,
            material_element: 'Fe',
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      const mat = result.stations['station_001'].inventory.find(
        (i) => i.kind === 'Material',
      ) as MaterialItem;
      expect(mat.kg).toBe(60);
      // Weighted average quality: (50*1.0 + 10*0.5) / 60
      expect(mat.quality).toBeCloseTo((50 + 5) / 60);
    });

    it('removes ore lot when fully consumed', () => {
      const station = makeStation({
        inventory: [makeOreLot({ kg: 30 })],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: {
          RefineryRan: {
            station_id: 'station_001', module_id: 'mod_1',
            ore_consumed_kg: 30, material_produced_kg: 20,
            material_quality: 1.0, slag_produced_kg: 5,
            material_element: 'Fe',
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      const ores = result.stations['station_001'].inventory.filter((i) => i.kind === 'Ore');
      expect(ores).toHaveLength(0);
    });
  });

  describe('AssemblerRan', () => {
    it('consumes material and produces component', () => {
      const station = makeStation({
        inventory: [{ kind: 'Material', element: 'Fe', kg: 100, quality: 1.0 }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: {
          AssemblerRan: {
            station_id: 'station_001', module_id: 'mod_1',
            material_consumed_kg: 20, material_element: 'Fe',
            component_produced_id: 'thruster', component_produced_count: 1,
            component_quality: 0.9,
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      const inv = result.stations['station_001'].inventory;

      const mat = inv.find((i) => i.kind === 'Material') as MaterialItem;
      expect(mat.kg).toBe(80);

      const comp = inv.find((i) => i.kind === 'Component') as ComponentItem;
      expect(comp.component_id).toBe('thruster');
      expect(comp.count).toBe(1);
      expect(comp.quality).toBe(0.9);
    });

    it('merges component with existing stock', () => {
      const station = makeStation({
        inventory: [
          { kind: 'Material', element: 'Fe', kg: 100, quality: 1.0 },
          { kind: 'Component', component_id: 'thruster', count: 3, quality: 1.0 },
        ],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: {
          AssemblerRan: {
            station_id: 'station_001', module_id: 'mod_1',
            material_consumed_kg: 20, material_element: 'Fe',
            component_produced_id: 'thruster', component_produced_count: 2,
            component_quality: 0.9,
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      const comp = result.stations['station_001'].inventory.find(
        (i) => i.kind === 'Component',
      ) as ComponentItem;
      expect(comp.count).toBe(5);
    });
  });

  describe('WearAccumulated', () => {
    it('updates module wear value', () => {
      const station = makeStation({
        modules: [{
          id: 'mod_1', def_id: 'module_refinery', enabled: true,
          kind_state: { Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false } },
          wear: { wear: 0.1 },
        }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: {
          WearAccumulated: {
            station_id: 'station_001', module_id: 'mod_1', wear_after: 0.25,
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      expect(result.stations['station_001'].modules[0].wear).toEqual({ wear: 0.25 });
    });
  });

  describe('ModuleAutoDisabled', () => {
    it('sets module enabled to false', () => {
      const station = makeStation({
        modules: [{
          id: 'mod_1', def_id: 'module_refinery', enabled: true,
          kind_state: { Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false } },
          wear: { wear: 1.0 },
        }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: {
          ModuleAutoDisabled: {
            station_id: 'station_001', module_id: 'mod_1', reason: 'WearLimit',
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      expect(result.stations['station_001'].modules[0].enabled).toBe(false);
    });
  });

  describe('MaintenanceRan', () => {
    it('reduces target module wear and updates repair kit count', () => {
      const station = makeStation({
        modules: [
          {
            id: 'mod_maint', def_id: 'module_maintenance', enabled: true,
            kind_state: { Maintenance: { ticks_since_last_run: 0 } },
            wear: { wear: 0 },
          },
          {
            id: 'mod_ref', def_id: 'module_refinery', enabled: true,
            kind_state: { Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false } },
            wear: { wear: 0.5 },
          },
        ],
        inventory: [
          { kind: 'Component', component_id: 'repair_kit', count: 10, quality: 1.0 },
        ],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: {
          MaintenanceRan: {
            station_id: 'station_001', module_id: 'mod_maint',
            target_module_id: 'mod_ref', wear_after: 0.3,
            repair_kits_remaining: 9,
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);

      // Target module wear reduced
      const refinery = result.stations['station_001'].modules.find((m) => m.id === 'mod_ref')!;
      expect(refinery.wear).toEqual({ wear: 0.3 });

      // Repair kit count updated
      const kits = result.stations['station_001'].inventory.find(
        (i) => i.kind === 'Component' && (i as ComponentItem).component_id === 'repair_kit',
      ) as ComponentItem;
      expect(kits.count).toBe(9);
    });
  });

  describe('ShipConstructed', () => {
    it('adds a new ship to state', () => {
      const events = [{
        id: 'e1', tick: 10,
        event: {
          ShipConstructed: {
            ship_id: 'ship_new', station_id: 'station_001',
            location_node: 'node_a', cargo_capacity_m3: 30,
          },
        },
      }];

      const result = applyEvents({}, {}, {}, emptyResearch, [], defaultBalance, events);

      expect(result.ships['ship_new']).toBeDefined();
      expect(result.ships['ship_new'].id).toBe('ship_new');
      expect(result.ships['ship_new'].location_node).toBe('node_a');
      expect(result.ships['ship_new'].cargo_capacity_m3).toBe(30);
      expect(result.ships['ship_new'].inventory).toEqual([]);
      expect(result.ships['ship_new'].task).toBeNull();
    });
  });

  describe('TechUnlocked', () => {
    it('adds tech_id to research unlocked list', () => {
      const events = [{
        id: 'e1', tick: 10,
        event: { TechUnlocked: { tech_id: 'tech_advanced_mining' } },
      }];

      const result = applyEvents({}, {}, {}, emptyResearch, [], defaultBalance, events);
      expect(result.research.unlocked).toContain('tech_advanced_mining');
    });

    it('appends to existing unlocked list', () => {
      const research: ResearchState = {
        ...emptyResearch,
        unlocked: ['tech_basic'],
      };
      const events = [{
        id: 'e1', tick: 10,
        event: { TechUnlocked: { tech_id: 'tech_advanced' } },
      }];

      const result = applyEvents({}, {}, {}, research, [], defaultBalance, events);
      expect(result.research.unlocked).toEqual(['tech_basic', 'tech_advanced']);
    });
  });

  describe('ScanSiteSpawned', () => {
    it('adds a new scan site', () => {
      const events = [{
        id: 'e1', tick: 10,
        event: {
          ScanSiteSpawned: {
            site_id: 'site_1', node: 'node_a', template_id: 'template_iron',
          },
        },
      }];

      const result = applyEvents({}, {}, {}, emptyResearch, [], defaultBalance, events);
      expect(result.scanSites).toHaveLength(1);
      expect(result.scanSites[0]).toEqual({
        id: 'site_1', node: 'node_a', template_id: 'template_iron',
      });
    });

    it('appends to existing scan sites', () => {
      const existingSites: ScanSite[] = [
        { id: 'site_0', node: 'node_b', template_id: 'template_gold' },
      ];
      const events = [{
        id: 'e1', tick: 10,
        event: {
          ScanSiteSpawned: {
            site_id: 'site_1', node: 'node_a', template_id: 'template_iron',
          },
        },
      }];

      const result = applyEvents({}, {}, {}, emptyResearch, existingSites, defaultBalance, events);
      expect(result.scanSites).toHaveLength(2);
    });
  });

  describe('ScanResult', () => {
    it('updates asteroid tag_beliefs', () => {
      const asteroid = makeAsteroid();
      const tags: [string, number][] = [['metallic', 0.8], ['icy', 0.2]];

      const events = [{
        id: 'e1', tick: 10,
        event: { ScanResult: { asteroid_id: 'ast_001', tags } },
      }];

      const result = applyEvents(
        { ast_001: asteroid }, {}, {}, emptyResearch, [], defaultBalance, events,
      );
      expect(result.asteroids['ast_001'].knowledge.tag_beliefs).toEqual(tags);
    });
  });

  describe('CompositionMapped', () => {
    it('updates asteroid composition', () => {
      const asteroid = makeAsteroid();
      const composition = { Fe: 0.6, Si: 0.3, Ni: 0.1 };

      const events = [{
        id: 'e1', tick: 10,
        event: { CompositionMapped: { asteroid_id: 'ast_001', composition } },
      }];

      const result = applyEvents(
        { ast_001: asteroid }, {}, {}, emptyResearch, [], defaultBalance, events,
      );
      expect(result.asteroids['ast_001'].knowledge.composition).toEqual(composition);
    });
  });

  describe('TaskStarted', () => {
    it('assigns a Mine task to the ship', () => {
      const ship = makeShip();
      const events = [{
        id: 'e1', tick: 10,
        event: {
          TaskStarted: {
            ship_id: 'ship_0001', task_kind: 'Mine', target: 'ast_001',
          },
        },
      }];

      const result = applyEvents({}, { ship_0001: ship }, {}, emptyResearch, [], defaultBalance, events);
      const task = result.ships['ship_0001'].task!;
      expect(task.started_tick).toBe(10);
      expect(task.kind).toEqual({ Mine: { asteroid: 'ast_001', duration_ticks: 0 } });
    });

    it('assigns a Survey task to the ship', () => {
      const ship = makeShip();
      const events = [{
        id: 'e1', tick: 10,
        event: {
          TaskStarted: {
            ship_id: 'ship_0001', task_kind: 'Survey', target: 'site_1',
          },
        },
      }];

      const result = applyEvents({}, { ship_0001: ship }, {}, emptyResearch, [], defaultBalance, events);
      expect(result.ships['ship_0001'].task!.kind).toEqual({ Survey: { site: 'site_1' } });
    });

    it('assigns a DeepScan task to the ship', () => {
      const ship = makeShip();
      const events = [{
        id: 'e1', tick: 10,
        event: {
          TaskStarted: {
            ship_id: 'ship_0001', task_kind: 'DeepScan', target: 'ast_001',
          },
        },
      }];

      const result = applyEvents({}, { ship_0001: ship }, {}, emptyResearch, [], defaultBalance, events);
      expect(result.ships['ship_0001'].task!.kind).toEqual({ DeepScan: { asteroid: 'ast_001' } });
    });

    it('assigns a Deposit task to the ship', () => {
      const ship = makeShip();
      const events = [{
        id: 'e1', tick: 10,
        event: {
          TaskStarted: {
            ship_id: 'ship_0001', task_kind: 'Deposit', target: 'station_001',
          },
        },
      }];

      const result = applyEvents({}, { ship_0001: ship }, {}, emptyResearch, [], defaultBalance, events);
      expect(result.ships['ship_0001'].task!.kind).toEqual({
        Deposit: { station: 'station_001', blocked: false },
      });
    });

    it('assigns a Transit task to the ship', () => {
      const ship = makeShip();
      const events = [{
        id: 'e1', tick: 10,
        event: {
          TaskStarted: {
            ship_id: 'ship_0001', task_kind: 'Transit', target: 'node_b',
          },
        },
      }];

      const result = applyEvents({}, { ship_0001: ship }, {}, emptyResearch, [], defaultBalance, events);
      expect(result.ships['ship_0001'].task!.kind).toEqual({
        Transit: { destination: 'node_b', total_ticks: 0, then: { Idle: {} } },
      });
    });

    it('falls back to Idle for unknown task kind', () => {
      const ship = makeShip();
      const events = [{
        id: 'e1', tick: 10,
        event: {
          TaskStarted: {
            ship_id: 'ship_0001', task_kind: 'UnknownTask', target: null,
          },
        },
      }];

      const result = applyEvents({}, { ship_0001: ship }, {}, emptyResearch, [], defaultBalance, events);
      expect(result.ships['ship_0001'].task!.kind).toEqual({ Idle: {} });
    });
  });

  describe('TaskCompleted', () => {
    it('clears the ship task', () => {
      const ship = makeShip({
        task: {
          kind: { Mine: { asteroid: 'ast_001', duration_ticks: 10 } },
          started_tick: 5, eta_tick: 15,
        },
      });

      const events = [{
        id: 'e1', tick: 15,
        event: { TaskCompleted: { ship_id: 'ship_0001', task_kind: 'Mine' } },
      }];

      const result = applyEvents({}, { ship_0001: ship }, {}, emptyResearch, [], defaultBalance, events);
      expect(result.ships['ship_0001'].task).toBeNull();
    });
  });

  describe('ShipArrived', () => {
    it('updates ship location_node', () => {
      const ship = makeShip({ location_node: 'node_a' });

      const events = [{
        id: 'e1', tick: 10,
        event: { ShipArrived: { ship_id: 'ship_0001', node: 'node_b' } },
      }];

      const result = applyEvents({}, { ship_0001: ship }, {}, emptyResearch, [], defaultBalance, events);
      expect(result.ships['ship_0001'].location_node).toBe('node_b');
    });
  });

  describe('DataGenerated', () => {
    it('adds to research data pool', () => {
      const events = [{
        id: 'e1', tick: 10,
        event: { DataGenerated: { kind: 'Materials', amount: 5.0 } },
      }];

      const result = applyEvents({}, {}, {}, emptyResearch, [], defaultBalance, events);
      expect(result.research.data_pool['Materials']).toBe(5.0);
    });

    it('accumulates with existing data', () => {
      const research: ResearchState = {
        ...emptyResearch,
        data_pool: { Materials: 3.0 },
      };
      const events = [{
        id: 'e1', tick: 10,
        event: { DataGenerated: { kind: 'Materials', amount: 7.0 } },
      }];

      const result = applyEvents({}, {}, {}, research, [], defaultBalance, events);
      expect(result.research.data_pool['Materials']).toBe(10.0);
    });
  });

  describe('ModuleAwaitingTech', () => {
    it('is a no-op (informational event)', () => {
      const station = makeStation({
        modules: [{
          id: 'mod_1', def_id: 'module_refinery', enabled: true,
          kind_state: { Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false } },
          wear: { wear: 0 },
        }],
      });

      const events = [{
        id: 'e1', tick: 10,
        event: {
          ModuleAwaitingTech: {
            station_id: 'station_001', module_id: 'mod_1', tech_id: 'tech_advanced',
          },
        },
      }];

      const result = applyEvents({}, {}, { station_001: station }, emptyResearch, [], defaultBalance, events);
      // State should be unchanged
      expect(result.stations['station_001'].modules[0].enabled).toBe(true);
      expect(result.stations['station_001'].modules[0].kind_state).toEqual({
        Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false },
      });
    });
  });
});
