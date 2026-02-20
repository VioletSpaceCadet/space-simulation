import { useSortableData } from '../hooks/useSortableData'
import type { InventoryItem, ModuleState, ShipState, StationState } from '../types'
import { SortIndicator } from './SortIndicator'

const QUALITY_TIER_EXCELLENT = 0.8
const QUALITY_TIER_GOOD = 0.5

function qualityTier(quality: number): string {
  if (quality >= QUALITY_TIER_EXCELLENT) return 'excellent'
  if (quality >= QUALITY_TIER_GOOD) return 'good'
  return 'poor'
}

function pct(frac: number): string {
  return `${Math.round(frac * 100)}%`
}

function formatKg(kg: number): string {
  return kg.toLocaleString(undefined, { maximumFractionDigits: 1 })
}

function taskLabel(task: ShipState['task']): string {
  if (!task) return 'idle'
  const key = Object.keys(task.kind)[0] ?? 'idle'
  return key.toLowerCase()
}

function totalInventoryKg(inventory: InventoryItem[]): number {
  return inventory.reduce((sum, i) => sum + ('kg' in i ? (i as { kg: number }).kg : 0), 0)
}

function InventoryDisplay({ inventory }: { inventory: InventoryItem[] }) {
  const hasModules = inventory.some((i) => i.kind === 'Module')
  const totalKg = totalInventoryKg(inventory)

  if (totalKg === 0 && !hasModules) return null

  return (
    <div className="flex flex-wrap gap-x-3 gap-y-0.5 mt-0.5">
      {inventory.map((item, idx) => {
        if (item.kind === 'Ore') {
          return (
            <span key={idx} className="text-cargo">
              ore {formatKg(item.kg)} kg
              <span className="text-faint ml-1">
                ({Object.entries(item.composition)
                  .sort(([, a], [, b]) => b - a)
                  .filter(([, f]) => f > 0.001)
                  .map(([el, f]) => `${el} ${pct(f)}`)
                  .join(', ')})
              </span>
            </span>
          )
        }
        if (item.kind === 'Material') {
          return (
            <span key={idx} className="text-cargo">
              {item.element} {formatKg(item.kg)} kg
              <span className="text-faint ml-1">({qualityTier(item.quality)})</span>
            </span>
          )
        }
        if (item.kind === 'Slag') {
          return (
            <span key={idx} className="text-dim">
              slag {formatKg(item.kg)} kg
            </span>
          )
        }
        if (item.kind === 'Module') {
          return (
            <span key={idx} className="text-faint text-[10px]">
              module: {item.module_def_id}
            </span>
          )
        }
        return null
      })}
    </div>
  )
}

function ModulesDisplay({ modules }: { modules: ModuleState[] }) {
  if (modules.length === 0) return null
  return (
    <div className="flex flex-wrap gap-x-3 gap-y-0.5 mt-0.5">
      {modules.map((m) => {
        const threshold =
          typeof m.kind_state === 'object' && 'Processor' in m.kind_state
            ? m.kind_state.Processor.threshold_kg
            : null
        return (
          <span key={m.id} className="text-[10px] text-dim">
            {m.def_id} · {m.enabled ? 'on' : 'off'}
            {threshold !== null && ` · threshold ${threshold} kg`}
          </span>
        )
      })}
    </div>
  )
}

function TaskProgress({ task, displayTick }: { task: ShipState['task']; displayTick: number }) {
  if (!task) return null
  const total = task.eta_tick - task.started_tick
  if (total <= 0) return null
  const elapsed = Math.max(0, Math.min(displayTick - task.started_tick, total))
  const pctDone = Math.round((elapsed / total) * 100)

  return (
    <div className="flex items-center gap-1.5 min-w-[80px]">
      <div
        role="progressbar"
        aria-valuenow={pctDone}
        aria-valuemin={0}
        aria-valuemax={100}
        className="flex-1 h-1.5 bg-edge rounded-full overflow-hidden"
      >
        <div
          className="h-full bg-accent rounded-full"
          style={{ width: `${pctDone}%` }}
        />
      </div>
      <span className="text-muted text-[10px] w-7 text-right">{pctDone}%</span>
    </div>
  )
}

// --- Ships table ---

interface SortableShip {
  id: string
  location_node: string
  task: string
  cargo_kg: number
  ship: ShipState
}

function ShipsTable({ ships, displayTick }: { ships: ShipState[]; displayTick: number }) {
  const sortableRows: SortableShip[] = ships.map((ship) => ({
    id: ship.id,
    location_node: ship.location_node,
    task: taskLabel(ship.task),
    cargo_kg: totalInventoryKg(ship.inventory),
    ship,
  }))

  const { sortedData, sortConfig, requestSort } = useSortableData(sortableRows)

  const headerClass =
    'text-left text-label px-2 py-1 border-b border-edge font-normal cursor-pointer hover:text-dim transition-colors select-none'

  return (
    <table className="min-w-max w-full border-collapse text-[11px]">
      <thead>
        <tr>
          <th className={headerClass} onClick={() => requestSort('id')}>
            ID<SortIndicator column="id" sortConfig={sortConfig} />
          </th>
          <th className={headerClass} onClick={() => requestSort('location_node')}>
            Location<SortIndicator column="location_node" sortConfig={sortConfig} />
          </th>
          <th className={headerClass} onClick={() => requestSort('task')}>
            Task<SortIndicator column="task" sortConfig={sortConfig} />
          </th>
          <th className="text-left text-label px-2 py-1 border-b border-edge font-normal select-none">
            Progress
          </th>
          <th className={headerClass} onClick={() => requestSort('cargo_kg')}>
            Cargo<SortIndicator column="cargo_kg" sortConfig={sortConfig} />
          </th>
        </tr>
      </thead>
      <tbody>
        {sortedData.map(({ ship, cargo_kg }) => (
          <tr key={ship.id}>
            <td className="px-2 py-0.5 border-b border-surface">{ship.id}</td>
            <td className="px-2 py-0.5 border-b border-surface">{ship.location_node}</td>
            <td className="px-2 py-0.5 border-b border-surface">{taskLabel(ship.task)}</td>
            <td className="px-2 py-0.5 border-b border-surface">
              <TaskProgress task={ship.task} displayTick={displayTick} />
            </td>
            <td className="px-2 py-0.5 border-b border-surface align-top">
              {cargo_kg === 0 ? (
                <span className="text-faint">empty</span>
              ) : (
                <div>
                  <span className="text-cargo">{formatKg(cargo_kg)} kg</span>
                  <InventoryDisplay inventory={ship.inventory} />
                </div>
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

// --- Stations table ---

interface SortableStation {
  id: string
  location_node: string
  cargo_kg: number
  station: StationState
}

function StationsTable({ stations }: { stations: StationState[] }) {
  const sortableRows: SortableStation[] = stations.map((station) => ({
    id: station.id,
    location_node: station.location_node,
    cargo_kg: totalInventoryKg(station.inventory),
    station,
  }))

  const { sortedData, sortConfig, requestSort } = useSortableData(sortableRows)

  const headerClass =
    'text-left text-label px-2 py-1 border-b border-edge font-normal cursor-pointer hover:text-dim transition-colors select-none'

  return (
    <table className="min-w-max w-full border-collapse text-[11px]">
      <thead>
        <tr>
          <th className={headerClass} onClick={() => requestSort('id')}>
            ID<SortIndicator column="id" sortConfig={sortConfig} />
          </th>
          <th className={headerClass} onClick={() => requestSort('location_node')}>
            Location<SortIndicator column="location_node" sortConfig={sortConfig} />
          </th>
          <th className={headerClass} onClick={() => requestSort('cargo_kg')}>
            Cargo<SortIndicator column="cargo_kg" sortConfig={sortConfig} />
          </th>
          <th className={headerClass}>
            Modules
          </th>
        </tr>
      </thead>
      <tbody>
        {sortedData.map(({ station, cargo_kg }) => (
          <tr key={station.id}>
            <td className="px-2 py-0.5 border-b border-surface align-top">{station.id}</td>
            <td className="px-2 py-0.5 border-b border-surface align-top">{station.location_node}</td>
            <td className="px-2 py-0.5 border-b border-surface align-top">
              {cargo_kg === 0 ? (
                <span className="text-faint">empty</span>
              ) : (
                <div>
                  <span className="text-cargo">{formatKg(cargo_kg)} kg</span>
                  <InventoryDisplay inventory={station.inventory} />
                </div>
              )}
            </td>
            <td className="px-2 py-0.5 border-b border-surface align-top">
              {station.modules.length === 0 ? (
                <span className="text-faint">none</span>
              ) : (
                <ModulesDisplay modules={station.modules} />
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

// --- Main panel ---

interface Props {
  ships: Record<string, ShipState>
  stations: Record<string, StationState>
  displayTick: number
}

export function FleetPanel({ ships, stations, displayTick }: Props) {
  const shipRows = Object.values(ships)
  const stationRows = Object.values(stations)

  return (
    <div className="overflow-y-auto flex-1">
      {shipRows.length === 0 ? (
        <div className="text-faint italic py-1">no ships</div>
      ) : (
        <ShipsTable ships={shipRows} displayTick={displayTick} />
      )}

      <div className="text-[10px] uppercase tracking-widest text-label mt-3 mb-1.5 pb-1 border-b border-edge">
        Stations
      </div>

      {stationRows.length === 0 ? (
        <div className="text-faint italic py-1">no stations</div>
      ) : (
        <StationsTable stations={stationRows} />
      )}
    </div>
  )
}
