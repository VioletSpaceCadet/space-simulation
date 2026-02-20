import type { InventoryItem, ModuleState, ShipState, StationState } from '../types'

function qualityTier(quality: number): string {
  if (quality >= 0.8) return 'excellent'
  if (quality >= 0.5) return 'good'
  return 'poor'
}

function pct(frac: number): string {
  return `${Math.round(frac * 100)}%`
}

function taskLabel(task: ShipState['task']): string {
  if (!task) return 'idle'
  const key = Object.keys(task.kind)[0] ?? 'idle'
  return key.toLowerCase()
}

function InventoryDisplay({ inventory }: { inventory: InventoryItem[] }) {
  const massItems = inventory.filter((i): i is Extract<InventoryItem, { kg: number }> => 'kg' in i)
  const totalKg = massItems.reduce((sum, i) => sum + i.kg, 0)

  const hasModules = inventory.some((i) => i.kind === 'Module')

  if (totalKg === 0 && !hasModules) {
    return <div className="text-faint mt-0.5">hold empty</div>
  }

  return (
    <div className="mt-0.5">
      {inventory.map((item, idx) => {
        if (item.kind === 'Ore') {
          return (
            <div key={idx} className="mb-0.5">
              <div className="flex gap-x-2 text-accent">
                <span>ore</span>
                <span>{item.kg.toLocaleString(undefined, { maximumFractionDigits: 1 })} kg</span>
                <span className="text-faint">← {item.asteroid_id}</span>
              </div>
              <div className="flex flex-wrap gap-x-2 text-[10px] text-dim pl-2">
                {Object.entries(item.composition)
                  .sort(([, a], [, b]) => b - a)
                  .filter(([, f]) => f > 0.001)
                  .map(([el, f]) => (
                    <span key={el}>{el} {pct(f)}</span>
                  ))}
              </div>
            </div>
          )
        }
        if (item.kind === 'Material') {
          return (
            <div key={idx} className="flex gap-x-2 text-accent mb-0.5">
              <span>{item.element}</span>
              <span>{item.kg.toLocaleString(undefined, { maximumFractionDigits: 1 })} kg</span>
              <span className="text-faint">{qualityTier(item.quality)}</span>
            </div>
          )
        }
        if (item.kind === 'Slag') {
          return (
            <div key={idx} className="mb-0.5">
              <div className="flex gap-x-2 text-dim">
                <span>slag</span>
                <span>{item.kg.toLocaleString(undefined, { maximumFractionDigits: 1 })} kg</span>
              </div>
              {Object.keys(item.composition).length > 0 && (
                <div className="flex flex-wrap gap-x-2 text-[10px] text-dim pl-2">
                  {Object.entries(item.composition)
                    .sort(([, a], [, b]) => b - a)
                    .filter(([, f]) => f > 0.001)
                    .map(([el, f]) => (
                      <span key={el}>{el} {pct(f)}</span>
                    ))}
                </div>
              )}
            </div>
          )
        }
        if (item.kind === 'Module') {
          return (
            <div key={idx} className="text-faint text-[10px]">
              module: {item.module_def_id}
            </div>
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
    <div className="mt-1">
      {modules.map((m) => {
        const threshold =
          typeof m.kind_state === 'object' && 'Processor' in m.kind_state
            ? m.kind_state.Processor.threshold_kg
            : null
        return (
          <div key={m.id} className="text-[10px] text-dim">
            {m.def_id} · {m.enabled ? 'on' : 'off'}
            {threshold !== null && ` · threshold ${threshold} kg`}
          </div>
        )
      })}
    </div>
  )
}

interface Props {
  ships: Record<string, ShipState>
  stations: Record<string, StationState>
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
            <InventoryDisplay inventory={ship.inventory} />
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
            <InventoryDisplay inventory={station.inventory} />
            <ModulesDisplay modules={station.modules} />
          </div>
        ))
      )}
    </div>
  )
}
