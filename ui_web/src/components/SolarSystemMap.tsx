import { useRef, useState } from 'react';

import { useSvgZoomPan } from '../hooks/useSvgZoomPan';
import type { AsteroidState, ScanSite, ShipState, SimSnapshot, StationState } from '../types';

import { DetailCard } from './solar-system/DetailCard';
import { angleFromId, polarToCartesian, ringRadiusForNode, transitPosition } from './solar-system/layout';
import { Tooltip } from './solar-system/Tooltip';

interface Props {
  snapshot: SimSnapshot | null
  currentTick: number

}

const RINGS: { nodeId: string; label: string; radius: number; isBelt: boolean }[] = [
  { nodeId: 'node_earth_orbit', label: 'Earth Orbit', radius: 100, isBelt: false },
  { nodeId: 'node_belt_inner', label: 'Inner Belt', radius: 200, isBelt: true },
  { nodeId: 'node_belt_mid', label: 'Mid Belt', radius: 300, isBelt: true },
  { nodeId: 'node_belt_outer', label: 'Outer Belt', radius: 400, isBelt: true },
];

function shipColor(task: ShipState['task']): string {
  if (!task) {return 'var(--color-dim)';}
  const kind = Object.keys(task.kind)[0];
  switch (kind) {
    case 'Survey': return '#5b9bd5';
    case 'DeepScan': return '#7b68ee';
    case 'Mine': return '#d4a44c';
    case 'Deposit': return '#4caf7d';
    case 'Transit': return 'var(--color-accent)';
    default: return 'var(--color-dim)';
  }
}

export function SolarSystemMap({ snapshot, currentTick }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const groupRef = useRef<SVGGElement>(null);

  useSvgZoomPan(svgRef, groupRef);

  const [hovered, setHovered] = useState<{
    type: string
    id: string
    screenX: number
    screenY: number
  } | null>(null);

  const [selected, setSelected] = useState<{ type: string; id: string } | null>(null);

  function entityMouseHandlers(type: string, id: string) {
    return {
      onMouseEnter: (e: React.MouseEvent) => {
        const rect = (e.currentTarget as Element).getBoundingClientRect();
        setHovered({ type, id, screenX: rect.left + rect.width / 2, screenY: rect.top });
      },
      onMouseLeave: () => setHovered(null),
    };
  }

  function lookupEntity(sel: { type: string; id: string }):
    | { type: 'station'; data: StationState }
    | { type: 'ship'; data: ShipState }
    | { type: 'asteroid'; data: AsteroidState }
    | { type: 'scan-site'; data: ScanSite }
    | null {
    if (!snapshot) {return null;}
    if (sel.type === 'station' && snapshot.stations[sel.id])
    {return { type: 'station', data: snapshot.stations[sel.id] };}
    if (sel.type === 'ship' && snapshot.ships[sel.id])
    {return { type: 'ship', data: snapshot.ships[sel.id] };}
    if (sel.type === 'asteroid' && snapshot.asteroids[sel.id])
    {return { type: 'asteroid', data: snapshot.asteroids[sel.id] };}
    if (sel.type === 'scan-site') {
      const site = snapshot.scan_sites.find(s => s.id === sel.id);
      if (site) {return { type: 'scan-site', data: site };}
    }
    return null;
  }

  return (
    <div className="relative w-full h-full bg-void overflow-hidden">
      <svg
        ref={svgRef}
        className="w-full h-full"
        viewBox="-500 -500 1000 1000"
        preserveAspectRatio="xMidYMid meet"
        onClick={(e) => { if (e.target === svgRef.current) {setSelected(null);} }}
      >
        <g ref={groupRef}>
          {/* Starfield */}
          {Array.from({ length: 80 }, (_, starIndex) => {
            const sx = ((starIndex * 7919 + 1) % 1000) - 500;
            const sy = ((starIndex * 6271 + 3) % 1000) - 500;
            const size = (starIndex % 3 === 0) ? 1.5 : 0.8;
            return (
              <circle
                key={`star-${starIndex}`}
                cx={sx}
                cy={sy}
                r={size}
                fill="var(--color-faint)"
                opacity={0.3 + (starIndex % 5) * 0.1}
              />
            );
          })}

          {/* Sun at center */}
          <circle cx={0} cy={0} r={12} fill="#f5c842" opacity={0.9} />
          <circle cx={0} cy={0} r={18} fill="none" stroke="#f5c842" opacity={0.2} strokeWidth={4} />

          {/* Orbital rings */}
          {RINGS.map((ring) => (
            <g key={ring.nodeId}>
              <circle
                cx={0}
                cy={0}
                r={ring.radius}
                fill="none"
                stroke="var(--color-dim)"
                strokeWidth={ring.isBelt ? 0.8 : 1.2}
                strokeDasharray={ring.isBelt ? '6 4' : undefined}
                opacity={0.6}
              />
              <text
                x={0}
                y={-ring.radius - 10}
                textAnchor="middle"
                fill="var(--color-fg)"
                fontSize={12}
                fontFamily="monospace"
                opacity={0.7}
              >
                {ring.label}
              </text>
            </g>
          ))}

          {/* Stations */}
          {snapshot && Object.values(snapshot.stations).map((station) => {
            const radius = ringRadiusForNode(station.location_node);
            const angle = angleFromId(station.id);
            const { x, y } = polarToCartesian(radius, angle);
            return (
              <rect
                key={station.id}
                data-entity-type="station"
                data-entity-id={station.id}
                x={x - 8}
                y={y - 8}
                width={16}
                height={16}
                fill="var(--color-accent)"
                transform={`rotate(45 ${x} ${y})`}
                stroke={selected?.id === station.id ? 'var(--color-bright)' : undefined}
                strokeWidth={selected?.id === station.id ? 2 : undefined}
                className="cursor-pointer"
                {...entityMouseHandlers('station', station.id)}
                onClick={() => setSelected({ type: 'station', id: station.id })}
              />
            );
          })}

          {/* Ships */}
          {snapshot && Object.values(snapshot.ships).map((ship) => {
            let x: number, y: number;
            const taskKind = ship.task ? Object.keys(ship.task.kind)[0] : null;

            if (taskKind === 'Transit' && ship.task) {
              const transit = (ship.task.kind as { Transit: { destination: string } }).Transit;
              const originRadius = ringRadiusForNode(ship.location_node);
              const originAngle = angleFromId(ship.id + ':origin');
              const destRadius = ringRadiusForNode(transit.destination);
              const destAngle = angleFromId(ship.id + ':dest');
              const progress = ship.task.eta_tick > ship.task.started_tick
                ? (currentTick - ship.task.started_tick) / (ship.task.eta_tick - ship.task.started_tick)
                : 1;
              const pos = transitPosition(
                { radius: originRadius, angle: originAngle },
                { radius: destRadius, angle: destAngle },
                progress,
              );
              x = pos.x;
              y = pos.y;
            } else {
              const radius = ringRadiusForNode(ship.location_node);
              const angle = angleFromId(ship.id);
              const pos = polarToCartesian(radius, angle);
              x = pos.x;
              y = pos.y;
            }

            return (
              <polygon
                key={ship.id}
                data-entity-type="ship"
                data-entity-id={ship.id}
                points={`${x},${y - 8} ${x - 6},${y + 5} ${x + 6},${y + 5}`}
                fill={shipColor(ship.task)}
                stroke={selected?.id === ship.id ? 'var(--color-bright)' : undefined}
                strokeWidth={selected?.id === ship.id ? 2 : undefined}
                className="cursor-pointer"
                {...entityMouseHandlers('ship', ship.id)}
                onClick={() => setSelected({ type: 'ship', id: ship.id })}
              />
            );
          })}

          {/* Asteroids */}
          {snapshot && Object.values(snapshot.asteroids).map((asteroid) => {
            const radius = ringRadiusForNode(asteroid.location_node);
            const angle = angleFromId(asteroid.id);
            const { x, y } = polarToCartesian(radius, angle);
            const massKg = asteroid.mass_kg ?? 1000;
            const size = Math.max(4, Math.min(12, Math.log10(massKg) + 1));
            const isIronRich = asteroid.anomaly_tags.includes('IronRich');

            return (
              <circle
                key={asteroid.id}
                data-entity-type="asteroid"
                data-entity-id={asteroid.id}
                cx={x}
                cy={y}
                r={size}
                fill={isIronRich ? '#c47038' : '#8a8e98'}
                opacity={0.9}
                stroke={selected?.id === asteroid.id ? 'var(--color-bright)' : undefined}
                strokeWidth={selected?.id === asteroid.id ? 2 : undefined}
                className="cursor-pointer"
                {...entityMouseHandlers('asteroid', asteroid.id)}
                onClick={() => setSelected({ type: 'asteroid', id: asteroid.id })}
              />
            );
          })}

          {/* Scan sites */}
          {snapshot && snapshot.scan_sites.map((site) => {
            const radius = ringRadiusForNode(site.node);
            const angle = angleFromId(site.id);
            const { x, y } = polarToCartesian(radius, angle);
            const isSelected = selected?.id === site.id;

            return (
              <g
                key={site.id}
                data-entity-type="scan-site"
                data-entity-id={site.id}
                className="cursor-pointer"
                {...entityMouseHandlers('scan-site', site.id)}
                onClick={() => setSelected({ type: 'scan-site', id: site.id })}
              >
                <circle
                  cx={x}
                  cy={y}
                  r={8}
                  fill="var(--color-edge)"
                  stroke={isSelected ? 'var(--color-bright)' : 'var(--color-muted)'}
                  strokeWidth={isSelected ? 1.5 : 0.8}
                  opacity={0.8}
                />
                <text
                  x={x}
                  y={y}
                  textAnchor="middle"
                  dominantBaseline="central"
                  fill={isSelected ? 'var(--color-bright)' : 'var(--color-fg)'}
                  fontSize={11}
                  fontFamily="monospace"
                  fontWeight="bold"
                >
                  ?
                </text>
              </g>
            );
          })}
        </g>
      </svg>

      {hovered && snapshot && (() => {
        const entity = lookupEntity(hovered);
        if (!entity) {return null;}
        return (
          <Tooltip x={hovered.screenX} y={hovered.screenY}>
            <div className="text-accent">{entity.data.id}</div>
            <div className="text-dim">{entity.type}</div>
          </Tooltip>
        );
      })()}

      {selected && snapshot && (() => {
        const entity = lookupEntity(selected);
        if (!entity) {return null;}
        return <DetailCard entity={entity} onClose={() => setSelected(null)} />;
      })()}
    </div>
  );
}
