import type { ShipState, StationState } from '../types'

interface Props {
  ships: Record<string, ShipState>
  stations: Record<string, StationState>
}

function taskLabel(task: ShipState['task']): string {
  if (!task) return 'idle'
  const key = Object.keys(task.kind)[0] ?? 'idle'
  return key.toLowerCase()
}

function totalCargoKg(cargo: Record<string, number>): number {
  return Object.values(cargo).reduce((sum, kg) => sum + kg, 0)
}

function CargoBreakdown({ cargo }: { cargo: Record<string, number> }) {
  const totalKg = totalCargoKg(cargo)
  if (totalKg === 0) return <div className="text-faint mt-0.5">hold empty</div>
  return (
    <div className="mt-0.5">
      <div className="text-muted mb-0.5">
        cargo: {totalKg.toLocaleString(undefined, { maximumFractionDigits: 1 })} kg
      </div>
      <div className="flex flex-wrap gap-x-2 text-accent">
        {Object.entries(cargo).map(([element, kg]) => (
          <span key={element}>{element} {kg.toFixed(1)}</span>
        ))}
      </div>
    </div>
  )
}

export function FleetPanel({ ships, stations }: Props) {
  const shipRows = Object.values(ships)
  const stationRows = Object.values(stations)

  return (
    <div className="overflow-y-auto flex-1">
      {shipRows.length === 0 ? (
        <div className="text-faint italic py-1">no ships</div>
      ) : (
        shipRows.map((ship) => (
          <div key={ship.id} className="py-1.5 border-b border-surface text-[11px]">
            <div className="text-bright mb-0.5">{ship.id}</div>
            <div className="text-dim">{ship.location_node} · {taskLabel(ship.task)}</div>
            <CargoBreakdown cargo={ship.cargo} />
          </div>
        ))
      )}

      <div className="text-[10px] uppercase tracking-widest text-label mt-3 mb-1.5 pb-1 border-b border-edge">
        Stations
      </div>

      {stationRows.length === 0 ? (
        <div className="text-faint italic py-1">no stations</div>
      ) : (
        stationRows.map((station) => (
          <div key={station.id} className="py-1.5 border-b border-surface text-[11px]">
            <div className="text-bright mb-0.5">{station.id}</div>
            <div className="text-dim">{station.location_node}</div>
            <CargoBreakdown cargo={station.cargo} />
          </div>
        ))
      )}
    </div>
  )
}
