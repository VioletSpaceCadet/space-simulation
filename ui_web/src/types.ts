export interface MetaInfo {
  tick: number
  seed: number
  content_version: string
}

export interface FacilitiesState {
  compute_units_total: number
  power_per_compute_unit_per_tick: number
  efficiency: number
}

export interface StationState {
  id: string
  location_node: string
  power_available_per_tick: number
  facilities: FacilitiesState
}

export interface TaskState {
  kind: { Idle: Record<string, never> } | { Survey: { site: string } } | { DeepScan: { asteroid: string } }
  started_tick: number
  eta_tick: number
}

export interface ShipState {
  id: string
  location_node: string
  owner: string
  task: TaskState | null
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
