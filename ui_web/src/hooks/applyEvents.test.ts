import { describe, expect, it } from 'vitest';

import type { AsteroidState, ComponentItem, MaterialItem, ModuleItem, OreItem, ResearchState, ShipState, SlagItem, StationState, TradeItemSpec } from '../types';

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
});
