export interface MetaInfo {
  tick: number
  seed: number
  schema_version?: number
  content_version: string
  ticks_per_sec: number
  paused: boolean
  minutes_per_tick: number
  trade_unlock_tick: number
}

export interface TaskState {
  kind:
    | { Idle: Record<string, never> }
    | { Survey: { site: string } }
    | { DeepScan: { asteroid: string } }
    | { Mine: { asteroid: string; duration_ticks: number } }
    | { Deposit: { station: string; blocked: boolean } }
    | { Transit: { destination: string; total_ticks: number; then: Record<string, unknown> } }
  started_tick: number
  eta_tick: number
}

// Inventory item discriminated union (matches Rust InventoryItem #[serde(tag = "kind")])
export type CompositionVec = Record<string, number>

export interface OreItem {
  kind: 'Ore'
  lot_id: string
  asteroid_id: string
  kg: number
  composition: CompositionVec
}

export interface SlagItem {
  kind: 'Slag'
  kg: number
  composition: CompositionVec
}

export interface MaterialItem {
  kind: 'Material'
  element: string
  kg: number
  quality: number
}

export interface ComponentItem {
  kind: 'Component'
  component_id: string
  count: number
  quality: number
}

export interface ModuleItem {
  kind: 'Module'
  item_id: string
  module_def_id: string
}

export type InventoryItem = OreItem | SlagItem | MaterialItem | ComponentItem | ModuleItem

// Module state
export interface ProcessorState {
  threshold_kg: number
  ticks_since_last_run: number
  stalled: boolean
}

export interface MaintenanceState {
  ticks_since_last_run: number
}

export interface AssemblerState {
  ticks_since_last_run: number
  stalled: boolean
  capped: boolean
  cap_override: Record<string, number>
}

export interface LabState {
  ticks_since_last_run: number
  assigned_tech: string | null
  starved: boolean
}

export interface BatteryState {
  charge_kwh: number
}

export interface SensorArrayState {
  ticks_since_last_run: number
}

export type ModuleKindState =
  | { Processor: ProcessorState }
  | { Maintenance: MaintenanceState }
  | { Assembler: AssemblerState }
  | { Lab: LabState }
  | { SensorArray: SensorArrayState }
  | { SolarArray: SensorArrayState }
  | { Battery: BatteryState }
  | 'Storage'

export interface WearState {
  wear: number
}

export interface ModuleState {
  id: string
  def_id: string
  enabled: boolean
  kind_state: ModuleKindState
  wear: WearState
}

export interface ShipState {
  id: string
  location_node: string
  owner: string
  inventory: InventoryItem[]
  cargo_capacity_m3: number
  task: TaskState | null
}

export interface PowerState {
  generated_kw: number
  consumed_kw: number
  deficit_kw: number
  battery_discharge_kw: number
  battery_charge_kw: number
  battery_stored_kwh: number
}

export interface StationState {
  id: string
  location_node: string
  power_available_per_tick: number
  inventory: InventoryItem[]
  cargo_capacity_m3: number
  modules: ModuleState[]
  power: PowerState
}

export interface AsteroidKnowledge {
  // Each entry: ["IronRich", 0.85]
  tag_beliefs: [string, number][]
  composition: Record<string, number> | null
}

export interface AsteroidState {
  id: string
  location_node: string
  anomaly_tags: string[]
  mass_kg?: number   // undefined = not yet known (discovered via event before snapshot)
  knowledge: AsteroidKnowledge
}

export interface ScanSite {
  id: string
  node: string
  template_id: string
}

export interface DomainProgress {
  points: Record<string, number>
}

export interface ResearchState {
  unlocked: string[]
  data_pool: Record<string, number>
  evidence: Record<string, DomainProgress>
  action_counts: Record<string, number>
}

export interface SimSnapshot {
  meta: MetaInfo
  balance: number
  scan_sites: ScanSite[]
  asteroids: Record<string, AsteroidState>
  ships: Record<string, ShipState>
  stations: Record<string, StationState>
  research: ResearchState
}

export interface SimEvent {
  id: string
  tick: number
  event: Record<string, unknown>
}

export type StreamMessage = SimEvent[] | { heartbeat: true; tick: number }

export interface PricingEntry {
  base_price_per_unit: number
  importable: boolean
  exportable: boolean
}

export interface PricingTable {
  import_surcharge_per_kg: number
  export_surcharge_per_kg: number
  items: Record<string, PricingEntry>
}

export type TradeItemSpec =
  | { Material: { element: string; kg: number } }
  | { Component: { component_id: string; count: number } }
  | { Module: { module_def_id: string } }

export type AlertSeverity = 'Warning' | 'Critical'

export interface ActiveAlert {
  alert_id: string
  severity: AlertSeverity
  message: string
  suggested_action: string
  tick: number
}
