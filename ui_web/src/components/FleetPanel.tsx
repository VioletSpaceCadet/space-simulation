import type { ShipState } from '../types'

interface Props {
  ships: Record<string, ShipState>
}

function taskLabel(task: ShipState['task']): string {
  if (!task) return 'idle'
  const key = Object.keys(task.kind)[0] ?? 'idle'
  return key.toLowerCase()
}

function totalCargoKg(cargo: Record<string, number>): number {
  return Object.values(cargo).reduce((sum, kg) => sum + kg, 0)
}

export function FleetPanel({ ships }: Props) {
  const rows = Object.values(ships)

  if (rows.length === 0) {
    return (
      <div className="overflow-y-auto flex-1">
        <div className="text-faint italic">no ships</div>
      </div>
    )
  }

  return (
    <div className="overflow-y-auto flex-1">
      {rows.map((ship) => {
        const totalKg = totalCargoKg(ship.cargo)
        const hasCargo = totalKg > 0

        return (
          <div key={ship.id} className="py-1.5 border-b border-surface text-[11px]">
            <div className="text-bright mb-0.5">{ship.id}</div>
            <div className="text-dim">{ship.location_node} · {taskLabel(ship.task)}</div>
            {hasCargo ? (
              <div className="mt-0.5">
                <div className="text-muted mb-0.5">
                  cargo: {totalKg.toLocaleString(undefined, { maximumFractionDigits: 1 })} kg
                </div>
                <div className="flex flex-wrap gap-x-2 text-accent">
                  {Object.entries(ship.cargo).map(([element, kg]) => (
                    <span key={element}>{element} {kg.toFixed(1)}</span>
                  ))}
                </div>
              </div>
            ) : (
              <div className="text-faint mt-0.5">hold empty</div>
            )}
          </div>
        )
      })}
    </div>
  )
}
