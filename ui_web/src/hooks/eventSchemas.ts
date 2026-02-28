import { z } from 'zod';

// --- Shared sub-schemas ---

const tradeItemSpecSchema = z.union([
  z.object({ Material: z.object({ element: z.string(), kg: z.number() }) }),
  z.object({ Component: z.object({ component_id: z.string(), count: z.number() }) }),
  z.object({ Module: z.object({ module_def_id: z.string() }) }),
]);

const powerStateSchema = z.object({
  generated_kw: z.number(),
  consumed_kw: z.number(),
  deficit_kw: z.number(),
  battery_discharge_kw: z.number(),
  battery_charge_kw: z.number(),
  battery_stored_kwh: z.number(),
});

const oreItemSchema = z.object({
  kind: z.literal('Ore'),
  lot_id: z.string(),
  asteroid_id: z.string(),
  kg: z.number(),
  composition: z.record(z.string(), z.number()),
});

const slagItemSchema = z.object({
  kind: z.literal('Slag'),
  kg: z.number(),
  composition: z.record(z.string(), z.number()),
});

const materialItemSchema = z.object({
  kind: z.literal('Material'),
  element: z.string(),
  kg: z.number(),
  quality: z.number(),
});

const componentItemSchema = z.object({
  kind: z.literal('Component'),
  component_id: z.string(),
  count: z.number(),
  quality: z.number(),
});

const moduleItemSchema = z.object({
  kind: z.literal('Module'),
  item_id: z.string(),
  module_def_id: z.string(),
});

const inventoryItemSchema = z.discriminatedUnion('kind', [
  oreItemSchema,
  slagItemSchema,
  materialItemSchema,
  componentItemSchema,
  moduleItemSchema,
]);

// --- Per-event payload schemas ---

export const eventSchemas = {
  AsteroidDiscovered: z.object({
    asteroid_id: z.string(),
    location_node: z.string(),
  }),

  OreMined: z.object({
    ship_id: z.string(),
    asteroid_id: z.string(),
    ore_lot: inventoryItemSchema,
    asteroid_remaining_kg: z.number(),
  }),

  OreDeposited: z.object({
    ship_id: z.string(),
    station_id: z.string(),
    items: z.array(inventoryItemSchema),
  }),

  ModuleInstalled: z.object({
    station_id: z.string(),
    module_id: z.string(),
    module_item_id: z.string(),
    module_def_id: z.string(),
    behavior_type: z.string(),
  }),

  ModuleUninstalled: z.object({
    station_id: z.string(),
    module_id: z.string(),
    module_item_id: z.string(),
  }),

  ModuleToggled: z.object({
    station_id: z.string(),
    module_id: z.string(),
    enabled: z.boolean(),
  }),

  ModuleThresholdSet: z.object({
    station_id: z.string(),
    module_id: z.string(),
    threshold_kg: z.number(),
  }),

  RefineryRan: z.object({
    station_id: z.string(),
    module_id: z.string(),
    ore_consumed_kg: z.number(),
    material_produced_kg: z.number(),
    material_quality: z.number(),
    slag_produced_kg: z.number(),
    material_element: z.string(),
  }),

  AssemblerRan: z.object({
    station_id: z.string(),
    module_id: z.string(),
    recipe_id: z.string(),
    material_consumed_kg: z.number(),
    material_element: z.string(),
    component_produced_id: z.string(),
    component_produced_count: z.number(),
    component_quality: z.number(),
  }),

  WearAccumulated: z.object({
    station_id: z.string(),
    module_id: z.string(),
    wear_before: z.number(),
    wear_after: z.number(),
  }),

  ModuleAutoDisabled: z.object({
    station_id: z.string(),
    module_id: z.string(),
  }),

  ModuleStalled: z.object({
    station_id: z.string(),
    module_id: z.string(),
    shortfall_m3: z.number(),
  }),

  ModuleResumed: z.object({
    station_id: z.string(),
    module_id: z.string(),
  }),

  AssemblerCapped: z.object({
    station_id: z.string(),
    module_id: z.string(),
  }),

  AssemblerUncapped: z.object({
    station_id: z.string(),
    module_id: z.string(),
  }),

  DepositBlocked: z.object({
    ship_id: z.string(),
    station_id: z.string(),
    shortfall_m3: z.number(),
  }),

  DepositUnblocked: z.object({
    ship_id: z.string(),
    station_id: z.string(),
  }),

  MaintenanceRan: z.object({
    station_id: z.string(),
    target_module_id: z.string(),
    wear_before: z.number(),
    wear_after: z.number(),
    repair_kits_remaining: z.number(),
  }),

  LabRan: z.object({
    station_id: z.string(),
    module_id: z.string(),
    tech_id: z.string(),
    data_consumed: z.number(),
    points_produced: z.number(),
    domain: z.string(),
  }),

  LabStarved: z.object({
    station_id: z.string(),
    module_id: z.string(),
  }),

  LabResumed: z.object({
    station_id: z.string(),
    module_id: z.string(),
  }),

  ScanResult: z.object({
    asteroid_id: z.string(),
    tags: z.array(z.tuple([z.string(), z.number()])),
  }),

  CompositionMapped: z.object({
    asteroid_id: z.string(),
    composition: z.record(z.string(), z.number()),
  }),

  TechUnlocked: z.object({
    tech_id: z.string(),
  }),

  ScanSiteSpawned: z.object({
    site_id: z.string(),
    node: z.string(),
    template_id: z.string(),
  }),

  ShipConstructed: z.object({
    ship_id: z.string(),
    station_id: z.string(),
    location_node: z.string(),
    cargo_capacity_m3: z.number(),
  }),

  ItemImported: z.object({
    station_id: z.string(),
    item_spec: tradeItemSpecSchema,
    cost: z.number(),
    balance_after: z.number(),
  }),

  ItemExported: z.object({
    station_id: z.string(),
    item_spec: tradeItemSpecSchema,
    revenue: z.number(),
    balance_after: z.number(),
  }),

  SlagJettisoned: z.object({
    station_id: z.string(),
    kg: z.number(),
  }),

  PowerStateUpdated: z.object({
    station_id: z.string(),
    power: powerStateSchema,
  }),

  TaskStarted: z.object({
    ship_id: z.string(),
    task_kind: z.string(),
    target: z.string().nullable(),
  }),

  TaskCompleted: z.object({
    ship_id: z.string(),
    task_kind: z.string(),
    target: z.string().nullable(),
  }),

  ShipArrived: z.object({
    ship_id: z.string(),
    node: z.string(),
  }),

  DataGenerated: z.object({
    kind: z.string(),
    amount: z.number(),
  }),

  // --- noOp events: no fields read, but schema validates structure ---
  ModuleAwaitingTech: z.object({
    station_id: z.string(),
    module_id: z.string(),
    tech_id: z.string(),
  }),

  InsufficientFunds: z.object({
    station_id: z.string(),
    action: z.string(),
    required: z.number(),
    available: z.number(),
  }),

  AlertRaised: z.object({
    alert_id: z.string(),
    severity: z.string(),
    message: z.string(),
    suggested_action: z.string(),
  }),

  AlertCleared: z.object({
    alert_id: z.string(),
  }),

  ResearchRoll: z.object({
    tech_id: z.string(),
    evidence: z.number(),
    p: z.number(),
    rolled: z.number(),
  }),

  PowerConsumed: z.object({
    station_id: z.string(),
    amount: z.number(),
  }),
} as const;

export type EventSchemas = typeof eventSchemas;
export type EventType = keyof EventSchemas;
