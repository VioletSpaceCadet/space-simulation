import { useSortableData } from '../hooks/useSortableData'
import type { OreCompositions } from '../hooks/useSimStream'
import type { ShipState, StationState } from '../types'
import { SortIndicator } from './SortIndicator'

interface Props {
  ships: Record<string, ShipState>
  stations: Record<string, StationState>
  oreCompositions: OreCompositions
}

function taskLabel(task: ShipState['task']): string {
  if (!task) return 'idle'
  const key = Object.keys(task.kind)[0] ?? 'idle'
  return key.toLowerCase()
}

function totalCargoKg(cargo: Record<string, number>): number {
  return Object.values(cargo).reduce((sum, kg) => sum + kg, 0)
}

function formatKg(kg: number): string {
  return kg.toLocaleString(undefined, { maximumFractionDigits: 1 })
}

function pct(frac: number): string {
  return `${Math.round(frac * 100)}%`
}

function CargoDetail({
  cargo,
  oreCompositions,
}: {
  cargo: Record<string, number>
  oreCompositions: OreCompositions
}) {
  const entries = Object.entries(cargo)
  if (entries.length === 0) return null

  return (
    <div className="flex flex-wrap gap-x-3 gap-y-0.5">
      {entries.map(([key, kg]) => {
        const isOre = key.startsWith('ore:')
        const composition = isOre ? oreCompositions[key] : null
        const label = isOre ? 'ore' : key

        return (
          <span key={key} className="text-cargo">
            {label} {formatKg(kg)} kg
            {composition && (
              <span className="text-faint ml-1">
                ({Object.entries(composition)
                  .sort(([, a], [, b]) => b - a)
                  .filter(([, frac]) => frac > 0.001)
                  .map(([el, frac]) => `${el} ${pct(frac)}`)
                  .join(', ')})
              </span>
            )}
          </span>
        )
      })}
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

function ShipsTable({ ships, oreCompositions }: { ships: ShipState[]; oreCompositions: OreCompositions }) {
  const sortableRows: SortableShip[] = ships.map((ship) => ({
    id: ship.id,
    location_node: ship.location_node,
    task: taskLabel(ship.task),
    cargo_kg: totalCargoKg(ship.cargo),
    ship,
  }))

  const { sortedData, sortConfig, requestSort } = useSortableData(sortableRows)

  const headerClass = "text-left text-label px-2 py-1 border-b border-edge font-normal cursor-pointer hover:text-dim transition-colors select-none"

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
            <td className="px-2 py-0.5 border-b border-surface align-top">
              {cargo_kg === 0
                ? <span className="text-faint">empty</span>
                : (
                  <div>
                    <span className="text-cargo">{formatKg(cargo_kg)} kg</span>
                    <CargoDetail cargo={ship.cargo} oreCompositions={oreCompositions} />
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

function StationsTable({ stations, oreCompositions }: { stations: StationState[]; oreCompositions: OreCompositions }) {
  const sortableRows: SortableStation[] = stations.map((station) => ({
    id: station.id,
    location_node: station.location_node,
    cargo_kg: totalCargoKg(station.cargo),
    station,
  }))

  const { sortedData, sortConfig, requestSort } = useSortableData(sortableRows)

  const headerClass = "text-left text-label px-2 py-1 border-b border-edge font-normal cursor-pointer hover:text-dim transition-colors select-none"

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
        </tr>
      </thead>
      <tbody>
        {sortedData.map(({ station, cargo_kg }) => (
          <tr key={station.id}>
            <td className="px-2 py-0.5 border-b border-surface">{station.id}</td>
            <td className="px-2 py-0.5 border-b border-surface">{station.location_node}</td>
            <td className="px-2 py-0.5 border-b border-surface align-top">
              {cargo_kg === 0
                ? <span className="text-faint">empty</span>
                : (
                  <div>
                    <span className="text-cargo">{formatKg(cargo_kg)} kg</span>
                    <CargoDetail cargo={station.cargo} oreCompositions={oreCompositions} />
                  </div>
                )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

// --- Main panel ---

export function FleetPanel({ ships, stations, oreCompositions }: Props) {
  const shipRows = Object.values(ships)
  const stationRows = Object.values(stations)

  return (
    <div className="overflow-y-auto flex-1">
      {shipRows.length === 0 ? (
        <div className="text-faint italic py-1">no ships</div>
      ) : (
        <ShipsTable ships={shipRows} oreCompositions={oreCompositions} />
      )}

      <div className="text-[10px] uppercase tracking-widest text-label mt-3 mb-1.5 pb-1 border-b border-edge">
        Stations
      </div>

      {stationRows.length === 0 ? (
        <div className="text-faint italic py-1">no stations</div>
      ) : (
        <StationsTable stations={stationRows} oreCompositions={oreCompositions} />
      )}
    </div>
  )
}
