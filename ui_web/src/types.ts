export interface MetaInfo {
  tick: number
  seed: number
  content_version: string
  ticks_per_sec: number
}

export interface FacilitiesState {
  compute_units_total: number
  power_per_compute_unit_per_tick: number
  efficiency: number
}

export interface TaskState {
  kind:
    | { Idle: Record<string, never> }
    | { Survey: { site: string } }
    | { DeepScan: { asteroid: string } }
    | { Mine: { asteroid: string; duration_ticks: number } }
    | { Deposit: { station: string; blocked: boolean } }
    | { Transit: { destination: string; total_ticks: number } }
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

export type ModuleKindState =
  | { Processor: ProcessorState }
  | 'Storage'

export interface ModuleState {
  id: string
  def_id: string
  enabled: boolean
  kind_state: ModuleKindState
}

export interface ShipState {
  id: string
  location_node: string
  owner: string
  inventory: InventoryItem[]
  cargo_capacity_m3: number
  task: TaskState | null
}

export interface StationState {
  id: string
  location_node: string
  power_available_per_tick: number
  inventory: InventoryItem[]
  cargo_capacity_m3: number
  facilities: FacilitiesState
  modules: ModuleState[]
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

export interface ResearchState {
  unlocked: string[]
  data_pool: Record<string, number>
  evidence: Record<string, number>
}

export interface SimSnapshot {
  meta: MetaInfo
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
