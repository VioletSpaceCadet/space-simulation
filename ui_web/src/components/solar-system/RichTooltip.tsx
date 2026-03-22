import { SEMANTIC_COLORS, shipTaskColor, tagColor } from '../../config/theme';
import type {
  AsteroidState,
  ScanSite,
  ShipState,
  StationState,
} from '../../types';
import { getTaskKind } from '../../utils';

import type { EntityInfo } from './DetailCard';

interface RichTooltipProps {
  entity: EntityInfo;
  x: number;
  y: number;
  currentTick: number;
}

export function RichTooltip({ entity, x, y, currentTick }: RichTooltipProps) {
  return (
    <div
      className="fixed z-50 pointer-events-none max-w-[240px]"
      style={{
        left: x,
        top: y - 12,
        transform: 'translate(-50%, -100%)',
      }}
    >
      <div className="bg-surface/95 border border-edge rounded-md px-3.5 py-2.5 backdrop-blur-[12px] shadow-[0_8px_32px_rgba(0,0,0,0.5)]">
        <div className="text-[13px] font-medium text-bright mb-0.5" style={{ fontFamily: 'system-ui, sans-serif' }}>
          {entity.data.id}
        </div>
        <div className="text-[9px] uppercase tracking-[1px] text-muted mb-1.5">
          {entity.type}
        </div>
        {entity.type === 'station' && <StationContent station={entity.data} />}
        {entity.type === 'ship' && <ShipContent ship={entity.data} currentTick={currentTick} />}
        {entity.type === 'asteroid' && <AsteroidContent asteroid={entity.data} />}
        {entity.type === 'scan-site' && <ScanSiteContent site={entity.data} />}
      </div>
      {/* Arrow pointer */}
      <div
        className="absolute left-1/2 -bottom-[5px] -translate-x-1/2 rotate-45 w-2 h-2 bg-surface/95 border-r border-b border-edge"
      />
    </div>
  );
}

function Row({ label, value, color }: { label: string; value: string; color?: string }) {
  return (
    <div className="flex justify-between text-[10px] py-0.5 gap-4">
      <span className="text-muted whitespace-nowrap">{label}</span>
      <span className="text-fg text-right whitespace-nowrap" style={color ? { color } : undefined}>{value}</span>
    </div>
  );
}

function StationContent({ station }: { station: StationState }) {
  const totalKg = station.inventory.reduce(
    (s, i) => s + ('kg' in i ? (i as { kg: number }).kg : 0), 0,
  );
  const powerSurplus = station.power.generated_kw - station.power.consumed_kw;
  const powerColor = powerSurplus >= 0 ? SEMANTIC_COLORS.positive : SEMANTIC_COLORS.negative;

  return (
    <>
      <Row label="Orbit" value={station.position.parent_body} />
      <Row
        label="Power"
        value={`${powerSurplus >= 0 ? '+' : ''}${powerSurplus.toFixed(0)} kW`}
        color={powerColor}
      />
      <Row label="Cargo" value={`${totalKg.toLocaleString(undefined, { maximumFractionDigits: 0 })} / ${station.cargo_capacity_m3} m\u00B3`} />
      <Row label="Modules" value={`${station.modules.filter((m) => m.enabled).length} / ${station.modules.length}`} />
    </>
  );
}

function ShipContent({ ship, currentTick }: { ship: ShipState; currentTick: number }) {
  const taskKind = getTaskKind(ship.task) ?? 'idle';
  const color = shipTaskColor(taskKind);
  const totalKg = ship.inventory.reduce(
    (s, i) => s + ('kg' in i ? (i as { kg: number }).kg : 0), 0,
  );

  return (
    <>
      <Row label="Task" value={taskKind.toLowerCase()} color={color} />
      <Row label="Location" value={ship.position.parent_body} />
      {ship.task && ship.task.eta_tick > currentTick && (
        <>
          <div className="h-[3px] bg-edge rounded-full my-1 overflow-hidden">
            <div
              className="h-full rounded-full"
              style={{
                width: `${Math.min(100, Math.round(((currentTick - ship.task.started_tick) / (ship.task.eta_tick - ship.task.started_tick)) * 100))}%`,
                background: color,
              }}
            />
          </div>
          <Row label="ETA" value={`${Math.max(0, ship.task.eta_tick - currentTick)} ticks`} />
        </>
      )}
      <Row label="Cargo" value={`${totalKg.toLocaleString(undefined, { maximumFractionDigits: 0 })} / ${ship.cargo_capacity_m3} m\u00B3`} />
    </>
  );
}

function AsteroidContent({ asteroid }: { asteroid: AsteroidState }) {
  return (
    <>
      <Row label="Location" value={asteroid.position.parent_body} />
      {asteroid.mass_kg != null && (
        <Row label="Mass" value={`${asteroid.mass_kg.toLocaleString()} kg`} />
      )}
      {asteroid.anomaly_tags.length > 0 && (
        <div className="flex gap-1 flex-wrap mt-1">
          {asteroid.anomaly_tags.map((tag) => {
            const color = tagColor(tag);
            return (
              <span
                key={tag}
                className="text-[9px] px-1.5 rounded-sm font-medium"
                style={{ color, background: `${color}22` }}
              >
                {tag}
              </span>
            );
          })}
        </div>
      )}
      {asteroid.knowledge.composition && (
        <div className="text-[10px] text-dim mt-1">
          {Object.entries(asteroid.knowledge.composition)
            .sort(([, a], [, b]) => b - a)
            .filter(([, frac]) => frac > 0.001)
            .map(([el, frac]) => `${el} ${Math.round(frac * 100)}%`)
            .join(' \u00b7 ')}
        </div>
      )}
    </>
  );
}

function ScanSiteContent({ site }: { site: ScanSite }) {
  return (
    <>
      <Row label="Location" value={site.position.parent_body} />
      <Row label="Template" value={site.template_id} />
    </>
  );
}
