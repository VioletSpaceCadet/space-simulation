import type { AsteroidState, ComponentItem, InventoryItem, MaterialItem, ModuleKindState, PowerState, ResearchState, ScanSite, ShipState, SimEvent, SlagItem, StationState, TaskState, TradeItemSpec } from '../types';

/** Mutable state bundle threaded through event handlers. */
interface SimState {
  asteroids: Record<string, AsteroidState>;
  ships: Record<string, ShipState>;
  stations: Record<string, StationState>;
  research: ResearchState;
  scanSites: ScanSite[];
  balance: number;
}

type EventHandler = (state: SimState, event: Record<string, unknown>, tick: number) => SimState;

const MODULE_KIND_STATE_MAP: Record<string, ModuleKindState> = {
  Processor: { Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false } },
  Storage: 'Storage',
  Maintenance: { Maintenance: { ticks_since_last_run: 0 } },
  Assembler: { Assembler: { ticks_since_last_run: 0, stalled: false, capped: false, cap_override: {} } },
  Lab: { Lab: { ticks_since_last_run: 0, assigned_tech: null, starved: false } },
  SensorArray: { SensorArray: { ticks_since_last_run: 0 } },
  SolarArray: { SolarArray: { ticks_since_last_run: 0 } },
  Battery: { Battery: { charge_kwh: 0 } },
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
function tradeItemToInventory(itemSpec: TradeItemSpec): InventoryItem {
  if ('Material' in itemSpec) {
    const { element, kg } = itemSpec.Material;
    return { kind: 'Material', element, kg, quality: 1.0 };
  }
  if ('Component' in itemSpec) {
    const { component_id, count } = itemSpec.Component;
    return { kind: 'Component', component_id, count, quality: 1.0 };
  }
  const { module_def_id } = itemSpec.Module;
  return { kind: 'Module', item_id: `imported_${module_def_id}_${Date.now()}`, module_def_id };
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

function handleAsteroidDiscovered(state: SimState, event: Record<string, unknown>): SimState {
  const { asteroid_id, location_node } = event as { asteroid_id: string; location_node: string };
  if (state.asteroids[asteroid_id]) {return state;}
  return {
    ...state,
    asteroids: {
      ...state.asteroids,
      [asteroid_id]: {
        id: asteroid_id,
        location_node,
        anomaly_tags: [],
        knowledge: { tag_beliefs: [], composition: null },
      },
    },
  };
}

function handleOreMined(state: SimState, event: Record<string, unknown>): SimState {
  const { ship_id, asteroid_id, ore_lot, asteroid_remaining_kg } = event as {
    ship_id: string
    asteroid_id: string
    ore_lot: ShipState['inventory'][number]
    asteroid_remaining_kg: number
  };
  let { asteroids, ships } = state;
  if (asteroid_remaining_kg <= 0) {
    asteroids = Object.fromEntries(
      Object.entries(asteroids).filter(([id]) => id !== asteroid_id)
    );
  } else if (asteroids[asteroid_id]) {
    asteroids = {
      ...asteroids,
      [asteroid_id]: { ...asteroids[asteroid_id], mass_kg: asteroid_remaining_kg },
    };
  }
  if (ships[ship_id]) {
    ships = {
      ...ships,
      [ship_id]: {
        ...ships[ship_id],
        inventory: [...ships[ship_id].inventory, ore_lot],
      },
    };
  }
  return { ...state, asteroids, ships };
}

function handleOreDeposited(state: SimState, event: Record<string, unknown>): SimState {
  const { ship_id, station_id, items } = event as {
    ship_id: string
    station_id: string
    items: StationState['inventory']
  };
  let { ships, stations } = state;
  if (ships[ship_id]) {
    ships = { ...ships, [ship_id]: { ...ships[ship_id], inventory: [] } };
  }
  if (stations[station_id]) {
    stations = {
      ...stations,
      [station_id]: {
        ...stations[station_id],
        inventory: [...stations[station_id].inventory, ...items],
      },
    };
  }
  return { ...state, ships, stations };
}

function handleModuleInstalled(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id, module_id, module_item_id, module_def_id, behavior_type } = event as {
    station_id: string
    module_id: string
    module_item_id: string
    module_def_id: string
    behavior_type: string
  };
  if (!state.stations[station_id]) {return state;}
  const station = state.stations[station_id];
  let kindState: ModuleKindState = MODULE_KIND_STATE_MAP[behavior_type];
  if (!kindState) {
    console.warn(`[applyEvents] Unknown behavior_type "${behavior_type}" for module ${module_id}, defaulting to Processor`);
    kindState = { Processor: { threshold_kg: 0, ticks_since_last_run: 0, stalled: false } };
  }
  return {
    ...state,
    stations: {
      ...state.stations,
      [station_id]: {
        ...station,
        inventory: station.inventory.filter(
          (i) => !(i.kind === 'Module' && i.item_id === module_item_id)
        ),
        modules: [
          ...station.modules,
          {
            id: module_id,
            def_id: module_def_id,
            enabled: false,
            kind_state: kindState,
            wear: { wear: 0 },
          },
        ],
      },
    },
  };
}

function handleModuleUninstalled(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id, module_id, module_item_id } = event as {
    station_id: string
    module_id: string
    module_item_id: string
  };
  if (!state.stations[station_id]) {return state;}
  const station = state.stations[station_id];
  const removed = station.modules.find((m) => m.id === module_id);
  const updatedModules = station.modules.filter((m) => m.id !== module_id);
  const updatedInventory = removed
    ? [...station.inventory, { kind: 'Module' as const, item_id: module_item_id, module_def_id: removed.def_id }]
    : station.inventory;
  return {
    ...state,
    stations: {
      ...state.stations,
      [station_id]: { ...station, modules: updatedModules, inventory: updatedInventory },
    },
  };
}

function handleModuleToggled(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id, module_id, enabled } = event as {
    station_id: string; module_id: string; enabled: boolean
  };
  return mapStationModule(state, station_id, module_id, (m) => ({ ...m, enabled }));
}

function handleModuleThresholdSet(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id, module_id, threshold_kg } = event as {
    station_id: string; module_id: string; threshold_kg: number
  };
  return mapStationModule(state, station_id, module_id, (m) => {
    const ks = m.kind_state;
    if (typeof ks === 'object' && 'Processor' in ks) {
      return { ...m, kind_state: { Processor: { ...ks.Processor, threshold_kg } } };
    }
    return m;
  });
}

function handleRefineryRan(state: SimState, event: Record<string, unknown>): SimState {
  const {
    station_id, ore_consumed_kg, material_produced_kg,
    material_quality, slag_produced_kg, material_element,
  } = event as {
    station_id: string
    ore_consumed_kg: number
    material_produced_kg: number
    material_quality: number
    slag_produced_kg: number
    material_element: string
  };
  if (!state.stations[station_id]) {return state;}
  let stationInv = [...state.stations[station_id].inventory];

  // Consume ore FIFO
  let remaining = ore_consumed_kg;
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
  if (material_produced_kg > 0.001) {
    const matIndex = stationInv.findIndex((i) => i.kind === 'Material' && i.element === material_element);
    if (matIndex >= 0) {
      const existing = stationInv[matIndex] as MaterialItem;
      const total = existing.kg + material_produced_kg;
      stationInv[matIndex] = {
        ...existing,
        kg: total,
        quality: (existing.kg * existing.quality + material_produced_kg * material_quality) / total,
      };
    } else {
      stationInv.push({ kind: 'Material', element: material_element, kg: material_produced_kg, quality: material_quality });
    }
  }

  // Blend or add slag
  if (slag_produced_kg > 0.001) {
    const existingIndex = stationInv.findIndex((i) => i.kind === 'Slag');
    if (existingIndex >= 0) {
      const existing = stationInv[existingIndex] as SlagItem;
      stationInv[existingIndex] = { ...existing, kg: existing.kg + slag_produced_kg };
    } else {
      stationInv.push({ kind: 'Slag', kg: slag_produced_kg, composition: {} });
    }
  }

  return {
    ...state,
    stations: {
      ...state.stations,
      [station_id]: { ...state.stations[station_id], inventory: stationInv },
    },
  };
}

function handleAssemblerRan(state: SimState, event: Record<string, unknown>): SimState {
  const {
    station_id, material_consumed_kg, material_element,
    component_produced_id, component_produced_count, component_quality,
  } = event as {
    station_id: string
    material_consumed_kg: number
    material_element: string
    component_produced_id: string
    component_produced_count: number
    component_quality: number
  };
  if (!state.stations[station_id]) {return state;}
  let stationInv = [...state.stations[station_id].inventory];

  // Consume material
  let remaining = material_consumed_kg;
  stationInv = stationInv.reduce<typeof stationInv>((acc, item) => {
    if (remaining > 0 && item.kind === 'Material' && item.element === material_element) {
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
    (i) => i.kind === 'Component' && (i as ComponentItem).component_id === component_produced_id
  );
  if (compIndex >= 0) {
    const existing = stationInv[compIndex] as ComponentItem;
    stationInv[compIndex] = { ...existing, count: existing.count + component_produced_count };
  } else {
    stationInv.push({
      kind: 'Component',
      component_id: component_produced_id,
      count: component_produced_count,
      quality: component_quality,
    });
  }

  return {
    ...state,
    stations: {
      ...state.stations,
      [station_id]: { ...state.stations[station_id], inventory: stationInv },
    },
  };
}

function handleWearAccumulated(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id, module_id, wear_after } = event as {
    station_id: string; module_id: string; wear_after: number
  };
  return mapStationModule(state, station_id, module_id, (m) => ({ ...m, wear: { wear: wear_after } }));
}

function handleModuleAutoDisabled(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id, module_id } = event as { station_id: string; module_id: string };
  return mapStationModule(state, station_id, module_id, (m) => ({ ...m, enabled: false }));
}

function handleModuleStalled(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id, module_id } = event as { station_id: string; module_id: string };
  return mapStationModule(state, station_id, module_id, (m) => {
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

function handleModuleResumed(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id, module_id } = event as { station_id: string; module_id: string };
  return mapStationModule(state, station_id, module_id, (m) => {
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

function handleAssemblerCapped(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id, module_id } = event as { station_id: string; module_id: string };
  return mapStationModule(state, station_id, module_id, (m) => {
    const ks = m.kind_state;
    if (typeof ks === 'object' && 'Assembler' in ks) {
      return { ...m, kind_state: { Assembler: { ...ks.Assembler, capped: true } } };
    }
    return m;
  });
}

function handleAssemblerUncapped(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id, module_id } = event as { station_id: string; module_id: string };
  return mapStationModule(state, station_id, module_id, (m) => {
    const ks = m.kind_state;
    if (typeof ks === 'object' && 'Assembler' in ks) {
      return { ...m, kind_state: { Assembler: { ...ks.Assembler, capped: false } } };
    }
    return m;
  });
}

function handleDepositBlocked(state: SimState, event: Record<string, unknown>): SimState {
  const { ship_id } = event as { ship_id: string };
  if (!state.ships[ship_id]?.task) {return state;}
  const task = state.ships[ship_id].task!;
  const kind = task.kind;
  if (typeof kind === 'object' && 'Deposit' in kind) {
    return {
      ...state,
      ships: {
        ...state.ships,
        [ship_id]: {
          ...state.ships[ship_id],
          task: { ...task, kind: { Deposit: { ...kind.Deposit, blocked: true } } },
        },
      },
    };
  }
  return state;
}

function handleDepositUnblocked(state: SimState, event: Record<string, unknown>): SimState {
  const { ship_id } = event as { ship_id: string };
  if (!state.ships[ship_id]?.task) {return state;}
  const task = state.ships[ship_id].task!;
  const kind = task.kind;
  if (typeof kind === 'object' && 'Deposit' in kind) {
    return {
      ...state,
      ships: {
        ...state.ships,
        [ship_id]: {
          ...state.ships[ship_id],
          task: { ...task, kind: { Deposit: { ...kind.Deposit, blocked: false } } },
        },
      },
    };
  }
  return state;
}

function handleMaintenanceRan(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id, target_module_id, wear_after, repair_kits_remaining } = event as {
    station_id: string
    target_module_id: string
    wear_after: number
    repair_kits_remaining: number
  };
  if (!state.stations[station_id]) {return state;}
  const station = state.stations[station_id];
  const updatedModules = station.modules.map((m) =>
    m.id === target_module_id ? { ...m, wear: { wear: wear_after } } : m
  );
  const updatedInventory = station.inventory.map((item) => {
    if (item.kind === 'Component' && (item as ComponentItem).component_id === 'repair_kit') {
      return { ...item, count: repair_kits_remaining } as ComponentItem;
    }
    return item;
  });
  return {
    ...state,
    stations: {
      ...state.stations,
      [station_id]: { ...station, modules: updatedModules, inventory: updatedInventory },
    },
  };
}

function handleLabRan(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id, module_id, tech_id } = event as {
    station_id: string; module_id: string; tech_id: string
  };
  return mapStationModule(state, station_id, module_id, (m) => {
    const ks = m.kind_state;
    if (typeof ks === 'object' && 'Lab' in ks) {
      return {
        ...m,
        kind_state: { Lab: { ...ks.Lab, ticks_since_last_run: 0, assigned_tech: tech_id, starved: false } },
      };
    }
    return m;
  });
}

function handleLabStarved(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id, module_id } = event as { station_id: string; module_id: string };
  return mapStationModule(state, station_id, module_id, (m) => {
    const ks = m.kind_state;
    if (typeof ks === 'object' && 'Lab' in ks) {
      return { ...m, kind_state: { Lab: { ...ks.Lab, starved: true } } };
    }
    return m;
  });
}

function handleLabResumed(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id, module_id } = event as { station_id: string; module_id: string };
  return mapStationModule(state, station_id, module_id, (m) => {
    const ks = m.kind_state;
    if (typeof ks === 'object' && 'Lab' in ks) {
      return { ...m, kind_state: { Lab: { ...ks.Lab, starved: false } } };
    }
    return m;
  });
}

function handleScanResult(state: SimState, event: Record<string, unknown>): SimState {
  const { asteroid_id, tags } = event as { asteroid_id: string; tags: [string, number][] };
  if (!state.asteroids[asteroid_id]) {return state;}
  return {
    ...state,
    asteroids: {
      ...state.asteroids,
      [asteroid_id]: {
        ...state.asteroids[asteroid_id],
        knowledge: { ...state.asteroids[asteroid_id].knowledge, tag_beliefs: tags },
      },
    },
  };
}

function handleCompositionMapped(state: SimState, event: Record<string, unknown>): SimState {
  const { asteroid_id, composition } = event as { asteroid_id: string; composition: Record<string, number> };
  if (!state.asteroids[asteroid_id]) {return state;}
  return {
    ...state,
    asteroids: {
      ...state.asteroids,
      [asteroid_id]: {
        ...state.asteroids[asteroid_id],
        knowledge: { ...state.asteroids[asteroid_id].knowledge, composition },
      },
    },
  };
}

function handleTechUnlocked(state: SimState, event: Record<string, unknown>): SimState {
  const { tech_id } = event as { tech_id: string };
  return {
    ...state,
    research: { ...state.research, unlocked: [...state.research.unlocked, tech_id] },
  };
}

function handleScanSiteSpawned(state: SimState, event: Record<string, unknown>): SimState {
  const { site_id, node, template_id } = event as { site_id: string; node: string; template_id: string };
  return { ...state, scanSites: [...state.scanSites, { id: site_id, node, template_id }] };
}

function handleShipConstructed(state: SimState, event: Record<string, unknown>): SimState {
  const { ship_id, location_node, cargo_capacity_m3 } = event as {
    ship_id: string; location_node: string; cargo_capacity_m3: number
  };
  return {
    ...state,
    ships: {
      ...state.ships,
      [ship_id]: {
        id: ship_id,
        location_node,
        owner: 'principal_autopilot',
        inventory: [],
        cargo_capacity_m3,
        task: null,
      },
    },
  };
}

function handleItemImported(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id, item_spec, balance_after } = event as {
    station_id: string; item_spec: TradeItemSpec; balance_after: number
  };
  let updatedState = { ...state, balance: balance_after };
  if (!state.stations[station_id]) {return updatedState;}
  const station = state.stations[station_id];
  const newItem = tradeItemToInventory(item_spec);
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
      [station_id]: { ...station, inventory: stationInv },
    },
  };
  return updatedState;
}

function handleItemExported(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id, item_spec, balance_after } = event as {
    station_id: string; item_spec: TradeItemSpec; balance_after: number
  };
  let updatedState = { ...state, balance: balance_after };
  if (!state.stations[station_id]) {return updatedState;}
  const station = state.stations[station_id];
  let stationInv = [...station.inventory];
  if ('Material' in item_spec) {
    const { element, kg } = item_spec.Material;
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
  } else if ('Component' in item_spec) {
    const { component_id, count } = item_spec.Component;
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
  } else if ('Module' in item_spec) {
    const { module_def_id } = item_spec.Module;
    const moduleIndex = stationInv.findIndex(
      (i) => i.kind === 'Module' && i.module_def_id === module_def_id
    );
    if (moduleIndex >= 0) {stationInv.splice(moduleIndex, 1);}
  }
  updatedState = {
    ...updatedState,
    stations: {
      ...state.stations,
      [station_id]: { ...station, inventory: stationInv },
    },
  };
  return updatedState;
}

function handleSlagJettisoned(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id } = event as { station_id: string };
  if (!state.stations[station_id]) {return state;}
  const station = state.stations[station_id];
  return {
    ...state,
    stations: {
      ...state.stations,
      [station_id]: { ...station, inventory: station.inventory.filter((i) => i.kind !== 'Slag') },
    },
  };
}

function handlePowerStateUpdated(state: SimState, event: Record<string, unknown>): SimState {
  const { station_id, power } = event as { station_id: string; power: PowerState };
  if (!state.stations[station_id]) {return state;}
  return {
    ...state,
    stations: {
      ...state.stations,
      [station_id]: { ...state.stations[station_id], power },
    },
  };
}

function handleTaskStarted(state: SimState, event: Record<string, unknown>, tick: number): SimState {
  const { ship_id, task_kind, target } = event as {
    ship_id: string; task_kind: string; target: string | null
  };
  if (!state.ships[ship_id]) {return state;}
  return {
    ...state,
    ships: {
      ...state.ships,
      [ship_id]: { ...state.ships[ship_id], task: buildTaskStub(task_kind, target, tick) },
    },
  };
}

function handleTaskCompleted(state: SimState, event: Record<string, unknown>): SimState {
  const { ship_id } = event as { ship_id: string };
  if (!state.ships[ship_id]) {return state;}
  return {
    ...state,
    ships: { ...state.ships, [ship_id]: { ...state.ships[ship_id], task: null } },
  };
}

function handleShipArrived(state: SimState, event: Record<string, unknown>): SimState {
  const { ship_id, node } = event as { ship_id: string; node: string };
  if (!state.ships[ship_id]) {return state;}
  return {
    ...state,
    ships: { ...state.ships, [ship_id]: { ...state.ships[ship_id], location_node: node } },
  };
}

function handleDataGenerated(state: SimState, event: Record<string, unknown>): SimState {
  const { kind, amount } = event as { kind: string; amount: number };
  return {
    ...state,
    research: {
      ...state.research,
      data_pool: {
        ...state.research.data_pool,
        [kind]: (state.research.data_pool[kind] ?? 0) + amount,
      },
    },
  };
}

// No-op handler for informational events that don't mutate state
function noOp(state: SimState): SimState {
  return state;
}

/** Handler lookup table — maps event type names to their handler functions. */
const EVENT_HANDLERS: Record<string, EventHandler> = {
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
    const event = e[eventKey] as Record<string, unknown>;

    const handler = EVENT_HANDLERS[eventKey];
    if (handler) {
      state = handler(state, event, evt.tick);
    } else if (import.meta.env.DEV) {
      console.warn(`[applyEvents] Unhandled event type: ${eventKey}`, event);
    }
  }

  return state;
}
