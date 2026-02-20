import type { OreCompositions } from '../../hooks/useSimStream'
import type { AsteroidState, ScanSite, ShipState, StationState } from '../../types'

type EntityInfo =
  | { type: 'station'; data: StationState }
  | { type: 'ship'; data: ShipState }
  | { type: 'asteroid'; data: AsteroidState }
  | { type: 'scan-site'; data: ScanSite }

interface DetailCardProps {
  entity: EntityInfo
  oreCompositions: OreCompositions
  onClose: () => void
}

export function DetailCard(props: DetailCardProps) {
  const { entity, onClose, oreCompositions } = props
  return (
    <div className="absolute top-4 right-4 w-64 bg-surface border border-edge rounded p-3 text-[11px] text-fg z-20">
      <div className="flex justify-between items-center mb-2">
        <span className="text-bright font-bold uppercase tracking-wider">{entity.type}</span>
        <button onClick={onClose} className="text-faint hover:text-bright cursor-pointer">✕</button>
      </div>
      <div className="text-accent mb-1">{entity.data.id}</div>
      {entity.type === 'station' && <StationDetail station={entity.data} oreCompositions={oreCompositions} />}
      {entity.type === 'ship' && <ShipDetail ship={entity.data} />}
      {entity.type === 'asteroid' && <AsteroidDetail asteroid={entity.data} />}
      {entity.type === 'scan-site' && <ScanSiteDetail site={entity.data} />}
    </div>
  )
}

function StationDetail({ station, oreCompositions }: { station: StationState; oreCompositions: OreCompositions }) {
  const totalKg = Object.values(station.cargo).reduce((s, v) => s + v, 0)
  const oreEntries = Object.entries(station.cargo).filter(([key]) => key.startsWith('ore:'))
  return (
    <>
      <div className="text-dim">{station.location_node}</div>
      <div className="text-muted mt-1">cargo: {totalKg.toLocaleString(undefined, { maximumFractionDigits: 1 })} kg</div>
      {oreEntries.length > 0 && (
        <div className="text-dim mt-1">
          {oreEntries.map(([oreKey, kg]) => {
            const comp = oreCompositions[oreKey]
            const compStr = comp
              ? Object.entries(comp)
                  .sort(([, a], [, b]) => b - a)
                  .filter(([, frac]) => frac > 0.001)
                  .map(([el, frac]) => `${el} ${Math.round(frac * 100)}%`)
                  .join(' · ')
              : 'unknown'
            return <div key={oreKey}>{oreKey}: {kg.toLocaleString(undefined, { maximumFractionDigits: 1 })} kg ({compStr})</div>
          })}
        </div>
      )}
    </>
  )
}

function ShipDetail({ ship }: { ship: ShipState }) {
  const taskKey = ship.task ? Object.keys(ship.task.kind)[0] : 'idle'
  const totalKg = Object.values(ship.cargo).reduce((s, v) => s + v, 0)
  return (
    <>
      <div className="text-dim">{ship.location_node} · {taskKey.toLowerCase()}</div>
      <div className="text-muted mt-1">cargo: {totalKg.toLocaleString(undefined, { maximumFractionDigits: 1 })} kg</div>
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
