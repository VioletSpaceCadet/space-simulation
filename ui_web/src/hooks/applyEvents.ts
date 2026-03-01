import type { z } from 'zod';

import type { AsteroidState, ComponentItem, MaterialItem, ModuleKindState, ResearchState, ScanSite, ShipState, SimEvent, SlagItem, StationState, TaskState, TradeItemSpec } from '../types';

import { eventSchemas } from './eventSchemas';
import type { EventType } from './eventSchemas';

/** Mutable state bundle threaded through event handlers. */
interface SimState {
  asteroids: Record<string, AsteroidState>;
  ships: Record<string, ShipState>;
  stations: Record<string, StationState>;
  research: ResearchState;
  scanSites: ScanSite[];
  balance: number;
}

/** Infer the parsed payload type from a Zod schema in eventSchemas. */
type EventPayload<K extends EventType> = z.infer<(typeof eventSchemas)[K]>;

const MODULE_KIND_STATE_MAP: Record<string, ModuleKindState> = {
  Processor: { Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false } },
  Storage: 'Storage',
  Maintenance: { Maintenance: { ticks_since_last_run: 0 } },
  Assembler: { Assembler: { ticks_since_last_run: 0, stalled: false, capped: false, cap_override: {} } },
  Lab: { Lab: { ticks_since_last_run: 0, assigned_tech: null, starved: false } },
  SensorArray: { SensorArray: { ticks_since_last_run: 0 } },
  SolarArray: { SolarArray: { ticks_since_last_run: 0 } },
  Battery: { Battery: { charge_kwh: 0 } },
  Radiator: { Radiator: {} },
};

function buildTaskStub(taskKind: string, target: string | null, tick: number): TaskState {
  const kindMap: Record<string, Record<string, unknown>> = {
    Survey: target ? { Survey: { site: target } } : { Idle: {} },
    DeepScan: target ? { DeepScan: { asteroid: target } } : { Idle: {} },
    Mine: target ? { Mine: { asteroid: target, duration_ticks: 0 } } : { Idle: {} },
    Deposit: target ? { Deposit: { station: target, blocked: false } } : { Idle: {} },
    Transit: target ? { Transit: { destination: target, total_ticks: 0, then: { Idle: {} } } } : { Idle: {} },
  };
  return {
    kind: (kindMap[taskKind] ?? { Idle: {} }) as TaskState['kind'],
    started_tick: tick,
    eta_tick: 0,
  };
}

/** Convert a TradeItemSpec (serde-tagged union) into an InventoryItem for the UI. */
function tradeItemToInventory(itemSpec: TradeItemSpec) {
  if ('Material' in itemSpec) {
    const { element, kg } = itemSpec.Material;
    return { kind: 'Material' as const, element, kg, quality: 1.0 };
  }
  if ('Component' in itemSpec) {
    const { component_id, count } = itemSpec.Component;
    return { kind: 'Component' as const, component_id, count, quality: 1.0 };
  }
  const { module_def_id } = itemSpec.Module;
  return { kind: 'Module' as const, item_id: `imported_${module_def_id}_${Date.now()}`, module_def_id };
}

// --- Helper: update a single module's kind_state within a station ---
function mapStationModule(
  state: SimState,
  stationId: string,
  moduleId: string,
  updater: (m: StationState['modules'][number]) => StationState['modules'][number],
): SimState {
  if (!state.stations[stationId]) {return state;}
  return {
    ...state,
    stations: {
      ...state.stations,
      [stationId]: {
        ...state.stations[stationId],
        modules: state.stations[stationId].modules.map((m) =>
          m.id === moduleId ? updater(m) : m
        ),
      },
    },
  };
}

// --- Individual event handlers ---

function handleAsteroidDiscovered(state: SimState, event: EventPayload<'AsteroidDiscovered'>): SimState {
  if (state.asteroids[event.asteroid_id]) {return state;}
  return {
    ...state,
    asteroids: {
      ...state.asteroids,
      [event.asteroid_id]: {
        id: event.asteroid_id,
        location_node: event.location_node,
        anomaly_tags: [],
        knowledge: { tag_beliefs: [], composition: null },
      },
    },
  };
}

function handleOreMined(state: SimState, event: EventPayload<'OreMined'>): SimState {
  let { asteroids, ships } = state;
  if (event.asteroid_remaining_kg <= 0) {
    asteroids = Object.fromEntries(
      Object.entries(asteroids).filter(([id]) => id !== event.asteroid_id)
    );
  } else if (asteroids[event.asteroid_id]) {
    asteroids = {
      ...asteroids,
      [event.asteroid_id]: { ...asteroids[event.asteroid_id], mass_kg: event.asteroid_remaining_kg },
    };
  }
  if (ships[event.ship_id]) {
    ships = {
      ...ships,
      [event.ship_id]: {
        ...ships[event.ship_id],
        inventory: [...ships[event.ship_id].inventory, event.ore_lot],
      },
    };
  }
  return { ...state, asteroids, ships };
}

function handleOreDeposited(state: SimState, event: EventPayload<'OreDeposited'>): SimState {
  let { ships, stations } = state;
  if (ships[event.ship_id]) {
    ships = { ...ships, [event.ship_id]: { ...ships[event.ship_id], inventory: [] } };
  }
  if (stations[event.station_id]) {
    stations = {
      ...stations,
      [event.station_id]: {
        ...stations[event.station_id],
        inventory: [...stations[event.station_id].inventory, ...event.items],
      },
    };
  }
  return { ...state, ships, stations };
}

function handleModuleInstalled(state: SimState, event: EventPayload<'ModuleInstalled'>): SimState {
  if (!state.stations[event.station_id]) {return state;}
  const station = state.stations[event.station_id];
  let kindState: ModuleKindState = MODULE_KIND_STATE_MAP[event.behavior_type];
  if (!kindState) {
    console.warn(
      `[applyEvents] Unknown behavior_type "${event.behavior_type}" for module ${event.module_id}, defaulting to Processor`
    );
    kindState = { Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false } };
  }
  return {
    ...state,
    stations: {
      ...state.stations,
      [event.station_id]: {
        ...station,
        inventory: station.inventory.filter(
          (i) => !(i.kind === 'Module' && i.item_id === event.module_item_id)
        ),
        modules: [
          ...station.modules,
          {
            id: event.module_id,
            def_id: event.module_def_id,
            enabled: false,
            kind_state: kindState,
            wear: { wear: 0 },
          },
        ],
      },
    },
  };
}

function handleModuleUninstalled(state: SimState, event: EventPayload<'ModuleUninstalled'>): SimState {
  if (!state.stations[event.station_id]) {return state;}
  const station = state.stations[event.station_id];
  const removed = station.modules.find((m) => m.id === event.module_id);
  const updatedModules = station.modules.filter((m) => m.id !== event.module_id);
  const updatedInventory = removed
    ? [...station.inventory, { kind: 'Module' as const, item_id: event.module_item_id, module_def_id: removed.def_id }]
    : station.inventory;
  return {
    ...state,
    stations: {
      ...state.stations,
      [event.station_id]: { ...station, modules: updatedModules, inventory: updatedInventory },
    },
  };
}

function handleModuleToggled(state: SimState, event: EventPayload<'ModuleToggled'>): SimState {
  return mapStationModule(state, event.station_id, event.module_id, (m) => ({ ...m, enabled: event.enabled }));
}

function handleModuleThresholdSet(state: SimState, event: EventPayload<'ModuleThresholdSet'>): SimState {
  return mapStationModule(state, event.station_id, event.module_id, (m) => {
    const ks = m.kind_state;
    if (typeof ks === 'object' && 'Processor' in ks) {
      return { ...m, kind_state: { Processor: { ...ks.Processor, threshold_kg: event.threshold_kg } } };
    }
    return m;
  });
}

function handleRefineryRan(state: SimState, event: EventPayload<'RefineryRan'>): SimState {
  if (!state.stations[event.station_id]) {return state;}
  let stationInv = [...state.stations[event.station_id].inventory];

  // Consume ore FIFO
  let remaining = event.ore_consumed_kg;
  stationInv = stationInv.reduce<typeof stationInv>((acc, item) => {
    if (remaining > 0 && item.kind === 'Ore') {
      const take = Math.min(item.kg, remaining);
      remaining -= take;
      if (item.kg - take > 0.001) {
        acc.push({ ...item, kg: item.kg - take });
      }
      return acc;
    }
    acc.push(item);
    return acc;
  }, []);

  // Merge material into existing lot or push new
  if (event.material_produced_kg > 0.001) {
    const matIndex = stationInv.findIndex((i) => i.kind === 'Material' && i.element === event.material_element);
    if (matIndex >= 0) {
      const existing = stationInv[matIndex] as MaterialItem;
      const total = existing.kg + event.material_produced_kg;
      stationInv[matIndex] = {
        ...existing,
        kg: total,
        quality: (existing.kg * existing.quality + event.material_produced_kg * event.material_quality) / total,
      };
    } else {
      stationInv.push({
        kind: 'Material',
        element: event.material_element,
        kg: event.material_produced_kg,
        quality: event.material_quality,
      });
    }
  }

  // Blend or add slag
  if (event.slag_produced_kg > 0.001) {
    const existingIndex = stationInv.findIndex((i) => i.kind === 'Slag');
    if (existingIndex >= 0) {
      const existing = stationInv[existingIndex] as SlagItem;
      stationInv[existingIndex] = { ...existing, kg: existing.kg + event.slag_produced_kg };
    } else {
      stationInv.push({ kind: 'Slag', kg: event.slag_produced_kg, composition: {} });
    }
  }

  return {
    ...state,
    stations: {
      ...state.stations,
      [event.station_id]: { ...state.stations[event.station_id], inventory: stationInv },
    },
  };
}

function handleAssemblerRan(state: SimState, event: EventPayload<'AssemblerRan'>): SimState {
  if (!state.stations[event.station_id]) {return state;}
  let stationInv = [...state.stations[event.station_id].inventory];

  // Consume material
  let remaining = event.material_consumed_kg;
  stationInv = stationInv.reduce<typeof stationInv>((acc, item) => {
    if (remaining > 0 && item.kind === 'Material' && item.element === event.material_element) {
      const take = Math.min(item.kg, remaining);
      remaining -= take;
      if (item.kg - take > 0.001) {
        acc.push({ ...item, kg: item.kg - take });
      }
      return acc;
    }
    acc.push(item);
    return acc;
  }, []);

  // Merge or create component
  const compIndex = stationInv.findIndex(
    (i) => i.kind === 'Component' && (i as ComponentItem).component_id === event.component_produced_id
  );
  if (compIndex >= 0) {
    const existing = stationInv[compIndex] as ComponentItem;
    stationInv[compIndex] = { ...existing, count: existing.count + event.component_produced_count };
  } else {
    stationInv.push({
      kind: 'Component',
      component_id: event.component_produced_id,
      count: event.component_produced_count,
      quality: event.component_quality,
    });
  }

  return {
    ...state,
    stations: {
      ...state.stations,
      [event.station_id]: { ...state.stations[event.station_id], inventory: stationInv },
    },
  };
}

function handleWearAccumulated(state: SimState, event: EventPayload<'WearAccumulated'>): SimState {
  return mapStationModule(state, event.station_id, event.module_id, (m) => (
    { ...m, wear: { wear: event.wear_after } }
  ));
}

function handleModuleAutoDisabled(state: SimState, event: EventPayload<'ModuleAutoDisabled'>): SimState {
  return mapStationModule(state, event.station_id, event.module_id, (m) => ({ ...m, enabled: false }));
}

function handleModuleStalled(state: SimState, event: EventPayload<'ModuleStalled'>): SimState {
  return mapStationModule(state, event.station_id, event.module_id, (m) => {
    const ks = m.kind_state;
    if (typeof ks === 'object' && 'Processor' in ks) {
      return { ...m, kind_state: { Processor: { ...ks.Processor, stalled: true } } };
    }
    if (typeof ks === 'object' && 'Assembler' in ks) {
      return { ...m, kind_state: { Assembler: { ...ks.Assembler, stalled: true } } };
    }
    return m;
  });
}

function handleModuleResumed(state: SimState, event: EventPayload<'ModuleResumed'>): SimState {
  return mapStationModule(state, event.station_id, event.module_id, (m) => {
    const ks = m.kind_state;
    if (typeof ks === 'object' && 'Processor' in ks) {
      return { ...m, kind_state: { Processor: { ...ks.Processor, stalled: false } } };
    }
    if (typeof ks === 'object' && 'Assembler' in ks) {
      return { ...m, kind_state: { Assembler: { ...ks.Assembler, stalled: false } } };
    }
    return m;
  });
}

function handleAssemblerCapped(state: SimState, event: EventPayload<'AssemblerCapped'>): SimState {
  return mapStationModule(state, event.station_id, event.module_id, (m) => {
    const ks = m.kind_state;
    if (typeof ks === 'object' && 'Assembler' in ks) {
      return { ...m, kind_state: { Assembler: { ...ks.Assembler, capped: true } } };
    }
    return m;
  });
}

function handleAssemblerUncapped(state: SimState, event: EventPayload<'AssemblerUncapped'>): SimState {
  return mapStationModule(state, event.station_id, event.module_id, (m) => {
    const ks = m.kind_state;
    if (typeof ks === 'object' && 'Assembler' in ks) {
      return { ...m, kind_state: { Assembler: { ...ks.Assembler, capped: false } } };
    }
    return m;
  });
}

function handleDepositBlocked(state: SimState, event: EventPayload<'DepositBlocked'>): SimState {
  if (!state.ships[event.ship_id]?.task) {return state;}
  const task = state.ships[event.ship_id].task!;
  const kind = task.kind;
  if (typeof kind === 'object' && 'Deposit' in kind) {
    return {
      ...state,
      ships: {
        ...state.ships,
        [event.ship_id]: {
          ...state.ships[event.ship_id],
          task: { ...task, kind: { Deposit: { ...kind.Deposit, blocked: true } } },
        },
      },
    };
  }
  return state;
}

function handleDepositUnblocked(state: SimState, event: EventPayload<'DepositUnblocked'>): SimState {
  if (!state.ships[event.ship_id]?.task) {return state;}
  const task = state.ships[event.ship_id].task!;
  const kind = task.kind;
  if (typeof kind === 'object' && 'Deposit' in kind) {
    return {
      ...state,
      ships: {
        ...state.ships,
        [event.ship_id]: {
          ...state.ships[event.ship_id],
          task: { ...task, kind: { Deposit: { ...kind.Deposit, blocked: false } } },
        },
      },
    };
  }
  return state;
}

function handleMaintenanceRan(state: SimState, event: EventPayload<'MaintenanceRan'>): SimState {
  if (!state.stations[event.station_id]) {return state;}
  const station = state.stations[event.station_id];
  const updatedModules = station.modules.map((m) =>
    m.id === event.target_module_id ? { ...m, wear: { wear: event.wear_after } } : m
  );
  const updatedInventory = station.inventory.map((item) => {
    if (item.kind === 'Component' && (item as ComponentItem).component_id === 'repair_kit') {
      return { ...item, count: event.repair_kits_remaining } as ComponentItem;
    }
    return item;
  });
  return {
    ...state,
    stations: {
      ...state.stations,
      [event.station_id]: { ...station, modules: updatedModules, inventory: updatedInventory },
    },
  };
}

function handleLabRan(state: SimState, event: EventPayload<'LabRan'>): SimState {
  return mapStationModule(state, event.station_id, event.module_id, (m) => {
    const ks = m.kind_state;
    if (typeof ks === 'object' && 'Lab' in ks) {
      return {
        ...m,
        kind_state: { Lab: { ...ks.Lab, ticks_since_last_run: 0, assigned_tech: event.tech_id, starved: false } },
      };
    }
    return m;
  });
}

function handleLabStarved(state: SimState, event: EventPayload<'LabStarved'>): SimState {
  return mapStationModule(state, event.station_id, event.module_id, (m) => {
    const ks = m.kind_state;
    if (typeof ks === 'object' && 'Lab' in ks) {
      return { ...m, kind_state: { Lab: { ...ks.Lab, starved: true } } };
    }
    return m;
  });
}

function handleLabResumed(state: SimState, event: EventPayload<'LabResumed'>): SimState {
  return mapStationModule(state, event.station_id, event.module_id, (m) => {
    const ks = m.kind_state;
    if (typeof ks === 'object' && 'Lab' in ks) {
      return { ...m, kind_state: { Lab: { ...ks.Lab, starved: false } } };
    }
    return m;
  });
}

function handleScanResult(state: SimState, event: EventPayload<'ScanResult'>): SimState {
  if (!state.asteroids[event.asteroid_id]) {return state;}
  return {
    ...state,
    asteroids: {
      ...state.asteroids,
      [event.asteroid_id]: {
        ...state.asteroids[event.asteroid_id],
        knowledge: { ...state.asteroids[event.asteroid_id].knowledge, tag_beliefs: event.tags },
      },
    },
  };
}

function handleCompositionMapped(state: SimState, event: EventPayload<'CompositionMapped'>): SimState {
  if (!state.asteroids[event.asteroid_id]) {return state;}
  return {
    ...state,
    asteroids: {
      ...state.asteroids,
      [event.asteroid_id]: {
        ...state.asteroids[event.asteroid_id],
        knowledge: { ...state.asteroids[event.asteroid_id].knowledge, composition: event.composition },
      },
    },
  };
}

function handleTechUnlocked(state: SimState, event: EventPayload<'TechUnlocked'>): SimState {
  return {
    ...state,
    research: { ...state.research, unlocked: [...state.research.unlocked, event.tech_id] },
  };
}

function handleScanSiteSpawned(state: SimState, event: EventPayload<'ScanSiteSpawned'>): SimState {
  return {
    ...state,
    scanSites: [...state.scanSites, { id: event.site_id, node: event.node, template_id: event.template_id }],
  };
}

function handleShipConstructed(state: SimState, event: EventPayload<'ShipConstructed'>): SimState {
  return {
    ...state,
    ships: {
      ...state.ships,
      [event.ship_id]: {
        id: event.ship_id,
        location_node: event.location_node,
        owner: 'principal_autopilot',
        inventory: [],
        cargo_capacity_m3: event.cargo_capacity_m3,
        task: null,
      },
    },
  };
}

function handleItemImported(state: SimState, event: EventPayload<'ItemImported'>): SimState {
  let updatedState = { ...state, balance: event.balance_after };
  if (!state.stations[event.station_id]) {return updatedState;}
  const station = state.stations[event.station_id];
  const newItem = tradeItemToInventory(event.item_spec);
  const stationInv = [...station.inventory];
  let merged = false;
  if (newItem.kind === 'Material') {
    const existingIndex = stationInv.findIndex(
      (i) => i.kind === 'Material' && i.element === newItem.element
    );
    if (existingIndex >= 0) {
      const existing = stationInv[existingIndex] as MaterialItem;
      stationInv[existingIndex] = { ...existing, kg: existing.kg + newItem.kg };
      merged = true;
    }
  } else if (newItem.kind === 'Component') {
    const existingIndex = stationInv.findIndex(
      (i) => i.kind === 'Component' && (i as ComponentItem).component_id === newItem.component_id
    );
    if (existingIndex >= 0) {
      const existing = stationInv[existingIndex] as ComponentItem;
      stationInv[existingIndex] = { ...existing, count: existing.count + newItem.count };
      merged = true;
    }
  }
  if (!merged) {stationInv.push(newItem);}
  updatedState = {
    ...updatedState,
    stations: {
      ...state.stations,
      [event.station_id]: { ...station, inventory: stationInv },
    },
  };
  return updatedState;
}

function handleItemExported(state: SimState, event: EventPayload<'ItemExported'>): SimState {
  const itemSpec = event.item_spec;
  let updatedState = { ...state, balance: event.balance_after };
  if (!state.stations[event.station_id]) {return updatedState;}
  const station = state.stations[event.station_id];
  let stationInv = [...station.inventory];
  if ('Material' in itemSpec) {
    const { element, kg } = itemSpec.Material;
    let remaining = kg;
    stationInv = stationInv.reduce<typeof stationInv>((acc, item) => {
      if (remaining > 0 && item.kind === 'Material' && item.element === element) {
        const take = Math.min(item.kg, remaining);
        remaining -= take;
        if (item.kg - take > 0.001) {
          acc.push({ ...item, kg: item.kg - take });
        }
        return acc;
      }
      acc.push(item);
      return acc;
    }, []);
  } else if ('Component' in itemSpec) {
    const { component_id, count } = itemSpec.Component;
    let remaining = count;
    stationInv = stationInv.reduce<typeof stationInv>((acc, item) => {
      if (remaining > 0 && item.kind === 'Component' && (item as ComponentItem).component_id === component_id) {
        const take = Math.min((item as ComponentItem).count, remaining);
        remaining -= take;
        if ((item as ComponentItem).count - take > 0) {
          acc.push({ ...item, count: (item as ComponentItem).count - take } as ComponentItem);
        }
        return acc;
      }
      acc.push(item);
      return acc;
    }, []);
  } else if ('Module' in itemSpec) {
    const { module_def_id } = itemSpec.Module;
    const moduleIndex = stationInv.findIndex(
      (i) => i.kind === 'Module' && i.module_def_id === module_def_id
    );
    if (moduleIndex >= 0) {stationInv.splice(moduleIndex, 1);}
  }
  updatedState = {
    ...updatedState,
    stations: {
      ...state.stations,
      [event.station_id]: { ...station, inventory: stationInv },
    },
  };
  return updatedState;
}

function handleSlagJettisoned(state: SimState, event: EventPayload<'SlagJettisoned'>): SimState {
  if (!state.stations[event.station_id]) {return state;}
  const station = state.stations[event.station_id];
  return {
    ...state,
    stations: {
      ...state.stations,
      [event.station_id]: { ...station, inventory: station.inventory.filter((i) => i.kind !== 'Slag') },
    },
  };
}

function handlePowerStateUpdated(state: SimState, event: EventPayload<'PowerStateUpdated'>): SimState {
  if (!state.stations[event.station_id]) {return state;}
  return {
    ...state,
    stations: {
      ...state.stations,
      [event.station_id]: { ...state.stations[event.station_id], power: event.power },
    },
  };
}

function handleTaskStarted(state: SimState, event: EventPayload<'TaskStarted'>, tick: number): SimState {
  if (!state.ships[event.ship_id]) {return state;}
  return {
    ...state,
    ships: {
      ...state.ships,
      [event.ship_id]: { ...state.ships[event.ship_id], task: buildTaskStub(event.task_kind, event.target, tick) },
    },
  };
}

function handleTaskCompleted(state: SimState, event: EventPayload<'TaskCompleted'>): SimState {
  if (!state.ships[event.ship_id]) {return state;}
  return {
    ...state,
    ships: { ...state.ships, [event.ship_id]: { ...state.ships[event.ship_id], task: null } },
  };
}

function handleShipArrived(state: SimState, event: EventPayload<'ShipArrived'>): SimState {
  if (!state.ships[event.ship_id]) {return state;}
  return {
    ...state,
    ships: { ...state.ships, [event.ship_id]: { ...state.ships[event.ship_id], location_node: event.node } },
  };
}

function handleDataGenerated(state: SimState, event: EventPayload<'DataGenerated'>): SimState {
  return {
    ...state,
    research: {
      ...state.research,
      data_pool: {
        ...state.research.data_pool,
        [event.kind]: (state.research.data_pool[event.kind] ?? 0) + event.amount,
      },
    },
  };
}

// No-op handler for informational events that don't mutate state
function noOp(state: SimState): SimState {
  return state;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyEventHandler = (state: SimState, event: any, tick: number) => SimState;

/** Handler lookup table — maps event type names to their handler functions. */
const EVENT_HANDLERS: Record<string, AnyEventHandler> = {
  AsteroidDiscovered: handleAsteroidDiscovered,
  OreMined: handleOreMined,
  OreDeposited: handleOreDeposited,
  ModuleInstalled: handleModuleInstalled,
  ModuleUninstalled: handleModuleUninstalled,
  ModuleToggled: handleModuleToggled,
  ModuleThresholdSet: handleModuleThresholdSet,
  RefineryRan: handleRefineryRan,
  AssemblerRan: handleAssemblerRan,
  WearAccumulated: handleWearAccumulated,
  ModuleAutoDisabled: handleModuleAutoDisabled,
  ModuleStalled: handleModuleStalled,
  ModuleResumed: handleModuleResumed,
  ModuleAwaitingTech: noOp,
  AssemblerCapped: handleAssemblerCapped,
  AssemblerUncapped: handleAssemblerUncapped,
  DepositBlocked: handleDepositBlocked,
  DepositUnblocked: handleDepositUnblocked,
  MaintenanceRan: handleMaintenanceRan,
  LabRan: handleLabRan,
  LabStarved: handleLabStarved,
  LabResumed: handleLabResumed,
  ScanResult: handleScanResult,
  CompositionMapped: handleCompositionMapped,
  TechUnlocked: handleTechUnlocked,
  ScanSiteSpawned: handleScanSiteSpawned,
  ShipConstructed: handleShipConstructed,
  ItemImported: handleItemImported,
  ItemExported: handleItemExported,
  SlagJettisoned: handleSlagJettisoned,
  PowerStateUpdated: handlePowerStateUpdated,
  InsufficientFunds: noOp,
  AlertRaised: noOp,
  AlertCleared: noOp,
  ResearchRoll: noOp,
  PowerConsumed: noOp,
  TaskStarted: handleTaskStarted,
  TaskCompleted: handleTaskCompleted,
  ShipArrived: handleShipArrived,
  DataGenerated: handleDataGenerated,
  ProcessorTooCold: noOp,
  OverheatWarning: noOp,
  OverheatCritical: noOp,
  OverheatCleared: noOp,
};

export function applyEvents(
  asteroids: Record<string, AsteroidState>,
  ships: Record<string, ShipState>,
  stations: Record<string, StationState>,
  research: ResearchState,
  scanSites: ScanSite[],
  balance: number,
  events: SimEvent[],
): {
  asteroids: Record<string, AsteroidState>
  ships: Record<string, ShipState>
  stations: Record<string, StationState>
  research: ResearchState
  scanSites: ScanSite[]
  balance: number
} {
  let state: SimState = {
    asteroids: { ...asteroids },
    ships: { ...ships },
    stations: { ...stations },
    research,
    scanSites: [...scanSites],
    balance,
  };

  for (const evt of events) {
    const e = evt.event;
    const eventKey = Object.keys(e)[0];

    const handler = EVENT_HANDLERS[eventKey];
    if (!handler) {
      if (import.meta.env.DEV) {
        console.warn(`[applyEvents] Unhandled event type: ${eventKey}`, e[eventKey]);
      }
      continue;
    }

    // Validate event payload against Zod schema
    const schema = eventSchemas[eventKey as EventType];
    if (schema) {
      const result = schema.safeParse(e[eventKey]);
      if (!result.success) {
        console.error(
          `[applyEvents] Invalid ${eventKey} event payload:`,
          result.error.issues,
          e[eventKey],
        );
        continue;
      }
      state = handler(state, result.data, evt.tick);
    } else {
      // No schema defined — pass raw data (shouldn't happen if schemas are exhaustive)
      state = handler(state, e[eventKey], evt.tick);
    }
  }

  return state;
}
