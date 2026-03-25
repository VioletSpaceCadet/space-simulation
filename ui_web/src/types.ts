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
    | { Transit: { destination: Position; total_ticks: number; then: Record<string, unknown> } }
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

export type Phase = 'Solid' | 'Liquid'

export interface MaterialThermalProps {
  temp_mk: number
  phase: Phase
  latent_heat_buffer_j: number
}

export interface MaterialItem {
  kind: 'Material'
  element: string
  kg: number
  quality: number
  thermal?: MaterialThermalProps
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
  selected_recipe?: string | null
}

export interface MaintenanceState {
  ticks_since_last_run: number
}

export interface AssemblerState {
  ticks_since_last_run: number
  stalled: boolean
  capped: boolean
  cap_override: Record<string, number>
  selected_recipe?: string | null
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
  | { Radiator: Record<string, never> }
  | 'Storage'

export interface WearState {
  wear: number
}

export type OverheatZone = 'Nominal' | 'Warning' | 'Critical' | 'Damage'

export interface ThermalState {
  temp_mk: number
  thermal_group: string | null
  overheat_zone: OverheatZone
  overheat_disabled: boolean
}

export interface ModuleState {
  id: string
  def_id: string
  enabled: boolean
  kind_state: ModuleKindState
  wear: WearState
  thermal?: ThermalState
  manufacturing_priority?: number
}

export interface Position {
  parent_body: string
  radius_au_um: number
  angle_mdeg: number
}

export interface FittedModule {
  slot_index: number
  module_def_id: string
}

export interface ShipState {
  id: string
  position: Position
  owner: string
  inventory: InventoryItem[]
  cargo_capacity_m3: number
  task: TaskState | null
  hull_id?: string
  fitted_modules?: FittedModule[]
  propellant_kg?: number
  propellant_capacity_kg?: number
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
  position: Position
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
  position: Position
  anomaly_tags: string[]
  mass_kg?: number   // undefined = not yet known (discovered via event before snapshot)
  knowledge: AsteroidKnowledge
}

export interface ScanSite {
  id: string
  position: Position
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

export interface AbsolutePos {
  x_au_um: number
  y_au_um: number
}

export type BodyType = 'Star' | 'Planet' | 'Moon' | 'Belt' | 'Zone'
export type ResourceClass = 'MetalRich' | 'Mixed' | 'VolatileRich'

export interface ZoneDef {
  radius_min_au_um: number
  radius_max_au_um: number
  angle_start_mdeg: number
  angle_span_mdeg: number
  resource_class: ResourceClass
  scan_site_weight: number
}

export interface OrbitalBodyDef {
  id: string
  name: string
  parent: string | null
  body_type: BodyType
  radius_au_um: number
  angle_mdeg: number
  solar_intensity: number
  zone: ZoneDef | null
}

export interface SolarSystemConfig {
  bodies: OrbitalBodyDef[]
  body_absolutes: Record<string, AbsolutePos>
  ticks_per_au: number
  min_transit_ticks: number
  docking_range_au_um: number
}

export interface SimSnapshot {
  meta: MetaInfo
  balance: number
  scan_sites: ScanSite[]
  asteroids: Record<string, AsteroidState>
  ships: Record<string, ShipState>
  stations: Record<string, StationState>
  research: ResearchState
  body_absolutes: Record<string, AbsolutePos>
}

export interface SimEvent {
  id: number
  tick: number
  event: Record<string, unknown>
}

export type StreamMessage = SimEvent[] | { heartbeat: true; tick: number }

export interface PricingEntry {
  base_price_per_unit: number
  importable: boolean
  exportable: boolean
  category: string
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

export interface TechEffect {
  type: string
  sigma?: number
}

export interface TechDef {
  id: string
  name: string
  prereqs: string[]
  domain_requirements: Record<string, number>
  accepted_data: string[]
  difficulty: number
  effects: TechEffect[]
}

export interface LabRateInfo {
  station_id: string
  module_id: string
  module_name: string
  assigned_tech: string | null
  domain: string
  points_per_hour: number
  starved: boolean
  enabled: boolean
}

export interface RecipeInput {
  filter: Record<string, unknown>
  amount: Record<string, number>
}

export interface RecipeThermalReq {
  min_temp_mk: number
  optimal_min_mk: number
  optimal_max_mk: number
  max_temp_mk: number
  heat_per_run_j: number
  efficiency_floor?: number
  quality_floor?: number
  quality_at_max?: number
}

export interface RecipeDef {
  id: string
  inputs: RecipeInput[]
  outputs: Record<string, unknown>[]
  efficiency: number
  thermal_req?: RecipeThermalReq | null
  required_tech?: string | null
  tags?: string[]
}

export interface ModuleFlowStats {
  module_id: string
  recipe_id: string
  runs_in_window: number
  total_input_kg: number
  total_output_kg: number
  total_output_count: number
  last_run_tick: number
  throughput_per_hour: number
  utilization_pct: number
  stall_reason: string | null
}

export interface ItemFlowStats {
  item_id: string
  current_qty: number
  delta_per_hour: number
  trend: 'rising' | 'falling' | 'stable'
  ticks_at_zero: number
}

export interface SlotDef {
  slot_type: string
  label: string
}

export interface HullDef {
  id: string
  name: string
  mass_kg: number
  cargo_capacity_m3: number
  base_speed_ticks_per_au: number
  base_propellant_capacity_kg: number
  slots: SlotDef[]
  bonuses: unknown[]
  tags: string[]
}

export interface ContentResponse {
  techs: TechDef[]
  lab_rates: LabRateInfo[]
  data_rates: Record<string, number>
  minutes_per_tick: number
  recipes: Record<string, RecipeDef>
  hulls: Record<string, HullDef>
}
