import type { AsteroidState, InventoryItem, OreItem, ScanSite, ShipState, StationState } from '../../types'

type EntityInfo =
  | { type: 'station'; data: StationState }
  | { type: 'ship'; data: ShipState }
  | { type: 'asteroid'; data: AsteroidState }
  | { type: 'scan-site'; data: ScanSite }

interface DetailCardProps {
  entity: EntityInfo
  onClose: () => void
}

export function DetailCard(props: DetailCardProps) {
  const { entity, onClose } = props
  return (
    <div className="absolute top-4 right-4 w-64 bg-surface border border-edge rounded p-3 text-[11px] text-fg z-20">
      <div className="flex justify-between items-center mb-2">
        <span className="text-bright font-bold uppercase tracking-wider">{entity.type}</span>
        <button onClick={onClose} className="text-faint hover:text-bright cursor-pointer">✕</button>
      </div>
      <div className="text-accent mb-1">{entity.data.id}</div>
      {entity.type === 'station' && <StationDetail station={entity.data} />}
      {entity.type === 'ship' && <ShipDetail ship={entity.data} />}
      {entity.type === 'asteroid' && <AsteroidDetail asteroid={entity.data} />}
      {entity.type === 'scan-site' && <ScanSiteDetail site={entity.data} />}
    </div>
  )
}

function inventoryKg(inventory: InventoryItem[]): number {
  return inventory.reduce((s, i) => s + ('kg' in i ? (i as { kg: number }).kg : 0), 0)
}

function StationDetail({ station }: { station: StationState }) {
  const totalKg = inventoryKg(station.inventory)
  const oreItems = station.inventory.filter((i): i is OreItem => i.kind === 'Ore')
  return (
    <>
      <div className="text-dim">{station.location_node}</div>
      <div className="text-muted mt-1">inventory: {totalKg.toLocaleString(undefined, { maximumFractionDigits: 1 })} kg</div>
      {oreItems.length > 0 && (
        <div className="text-dim mt-1">
          {oreItems.map((item) => {
            const compStr = Object.entries(item.composition)
              .sort(([, a], [, b]) => b - a)
              .filter(([, frac]) => frac > 0.001)
              .map(([el, frac]) => `${el} ${Math.round(frac * 100)}%`)
              .join(' · ')
            return (
              <div key={item.lot_id}>
                {item.asteroid_id}: {item.kg.toLocaleString(undefined, { maximumFractionDigits: 1 })} kg ({compStr || 'unknown'})
              </div>
            )
          })}
        </div>
      )}
    </>
  )
}

function ShipDetail({ ship }: { ship: ShipState }) {
  const taskKey = ship.task ? Object.keys(ship.task.kind)[0] : 'idle'
  const totalKg = inventoryKg(ship.inventory)
  return (
    <>
      <div className="text-dim">{ship.location_node} · {taskKey.toLowerCase()}</div>
      <div className="text-muted mt-1">inventory: {totalKg.toLocaleString(undefined, { maximumFractionDigits: 1 })} kg</div>
    </>
  )
}

function AsteroidDetail({ asteroid }: { asteroid: AsteroidState }) {
  return (
    <>
      <div className="text-dim">{asteroid.location_node}</div>
      {asteroid.mass_kg != null && (
        <div className="text-muted mt-1">mass: {asteroid.mass_kg.toLocaleString()} kg</div>
      )}
      {asteroid.anomaly_tags.length > 0 && (
        <div className="text-muted mt-1">tags: {asteroid.anomaly_tags.join(', ')}</div>
      )}
      {asteroid.knowledge.composition && (
        <div className="text-dim mt-1">
          {Object.entries(asteroid.knowledge.composition)
            .sort(([, a], [, b]) => b - a)
            .filter(([, frac]) => frac > 0.001)
            .map(([el, frac]) => `${el} ${Math.round(frac * 100)}%`)
            .join(' · ')}
        </div>
      )}
    </>
  )
}

function ScanSiteDetail({ site }: { site: ScanSite }) {
  return (
    <>
      <div className="text-dim">{site.node}</div>
      <div className="text-muted mt-1">template: {site.template_id}</div>
    </>
  )
}
