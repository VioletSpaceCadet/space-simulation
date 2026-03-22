import { useEffect, useRef, useState } from 'react';

import { fetchSpatialConfig } from '../api';
import {
  BODY_COLORS,
  TAG_COLORS,
  ZONE_COLORS,
  ZONE_STROKES,
  shipTaskColor,
  tagColor,
} from '../config/theme';
import { useSvgZoomPan } from '../hooks/useSvgZoomPan';
import type {
  AbsolutePos,
  AsteroidState,
  OrbitalBodyDef,
  Position,
  ScanSite,
  ShipState,
  SimSnapshot,
  SolarSystemConfig,
  StationState,
} from '../types';
import { getTaskKind } from '../utils';
import {
  auUmToAu,
  distanceAuUm,
  entityAbsolute,
  estimateTravelTicks,
  mdegToRad,
  shipTransitAbsolute,
} from '../utils/spatial';

import { DetailCard } from './solar-system/DetailCard';
import { Tooltip } from './solar-system/Tooltip';

interface Props {
  snapshot: SimSnapshot | null
  currentTick: number
}

/** Scale factor: 1 SVG unit = 10,000 µAU */
const SVG_SCALE = 10_000;

function toSvg(auUm: number): number {
  return auUm / SVG_SCALE;
}

function toSvgPos(abs: AbsolutePos): { x: number; y: number } {
  return { x: toSvg(abs.x_au_um), y: toSvg(abs.y_au_um) };
}

function shipColor(task: ShipState['task']): string {
  if (!task) {return 'var(--color-dim)';}
  const kind = getTaskKind(task) ?? 'idle';
  return shipTaskColor(kind);
}

/** Build an SVG path for a zone arc (annular sector). */
function zoneArcPath(
  centerX: number,
  centerY: number,
  rMin: number,
  rMax: number,
  startMdeg: number,
  spanMdeg: number,
): string {
  if (spanMdeg >= 360_000) {
    // Full circle donut — use two semicircles to avoid degenerate arc
    return [
      `M ${centerX + rMax} ${centerY}`,
      `A ${rMax} ${rMax} 0 1 1 ${centerX - rMax} ${centerY}`,
      `A ${rMax} ${rMax} 0 1 1 ${centerX + rMax} ${centerY}`,
      'Z',
      `M ${centerX + rMin} ${centerY}`,
      `A ${rMin} ${rMin} 0 1 0 ${centerX - rMin} ${centerY}`,
      `A ${rMin} ${rMin} 0 1 0 ${centerX + rMin} ${centerY}`,
      'Z',
    ].join(' ');
  }

  const startRad = mdegToRad(startMdeg);
  const endRad = mdegToRad(startMdeg + spanMdeg);
  const largeArc = spanMdeg > 180_000 ? 1 : 0;

  const outerX1 = centerX + rMax * Math.cos(startRad);
  const outerY1 = centerY + rMax * Math.sin(startRad);
  const outerX2 = centerX + rMax * Math.cos(endRad);
  const outerY2 = centerY + rMax * Math.sin(endRad);
  const innerX1 = centerX + rMin * Math.cos(endRad);
  const innerY1 = centerY + rMin * Math.sin(endRad);
  const innerX2 = centerX + rMin * Math.cos(startRad);
  const innerY2 = centerY + rMin * Math.sin(startRad);

  return [
    `M ${outerX1} ${outerY1}`,
    `A ${rMax} ${rMax} 0 ${largeArc} 1 ${outerX2} ${outerY2}`,
    `L ${innerX1} ${innerY1}`,
    `A ${rMin} ${rMin} 0 ${largeArc} 0 ${innerX2} ${innerY2}`,
    'Z',
  ].join(' ');
}

function formatDistance(distAuUm: number): string {
  const au = auUmToAu(distAuUm);
  if (au < 0.01) {
    return `${(au * 1000).toFixed(1)} mAU`;
  }
  return `${au.toFixed(2)} AU`;
}

export function SolarSystemMap({ snapshot, currentTick }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const groupRef = useRef<SVGGElement>(null);

  useSvgZoomPan(svgRef, groupRef, { minZoom: 0.1, maxZoom: 50 });

  const [config, setConfig] = useState<SolarSystemConfig | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetchSpatialConfig()
      .then((c) => { if (!cancelled) { setConfig(c); } })
      .catch((err) => console.error('Failed to load spatial config:', err));
    return () => { cancelled = true; };
  }, []);

  const [hovered, setHovered] = useState<{
    type: string
    id: string
    screenX: number
    screenY: number
  } | null>(null);

  const [selected, setSelected] = useState<{ type: string; id: string } | null>(null);

  // Merge: config provides baseline, snapshot overrides with tick-fresh values
  const bodyAbsolutes = { ...(config?.body_absolutes ?? {}), ...(snapshot?.body_absolutes ?? {}) };

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
    if (!snapshot) { return null; }
    if (sel.type === 'station' && snapshot.stations[sel.id]) {
      return { type: 'station', data: snapshot.stations[sel.id] };
    }
    if (sel.type === 'ship' && snapshot.ships[sel.id]) {
      return { type: 'ship', data: snapshot.ships[sel.id] };
    }
    if (sel.type === 'asteroid' && snapshot.asteroids[sel.id]) {
      return { type: 'asteroid', data: snapshot.asteroids[sel.id] };
    }
    if (sel.type === 'scan-site') {
      const site = snapshot.scan_sites.find(s => s.id === sel.id);
      if (site) { return { type: 'scan-site', data: site }; }
    }
    return null;
  }

  function getEntityAbsolute(sel: { type: string; id: string }): AbsolutePos | null {
    if (!snapshot) { return null; }
    const entity = lookupEntity(sel);
    if (!entity) { return null; }
    return entityAbsolute(entity.data.position, bodyAbsolutes);
  }

  // Compute ship SVG position, handling transit interpolation
  function shipSvgPos(ship: ShipState): { x: number; y: number } {
    const taskKind = getTaskKind(ship.task);

    if (taskKind === 'Transit' && ship.task && 'Transit' in ship.task.kind) {
      const transit = (ship.task.kind as { Transit: { destination: Position } }).Transit;
      const originAbs = entityAbsolute(ship.position, bodyAbsolutes);
      const destAbs = entityAbsolute(transit.destination, bodyAbsolutes);
      const progress = ship.task.eta_tick > ship.task.started_tick
        ? (currentTick - ship.task.started_tick) / (ship.task.eta_tick - ship.task.started_tick)
        : 1;
      return toSvgPos(shipTransitAbsolute(originAbs, destAbs, progress));
    }

    return toSvgPos(entityAbsolute(ship.position, bodyAbsolutes));
  }

  // Zone bodies from config
  const zoneBodies = config?.bodies.filter((b: OrbitalBodyDef) => b.zone !== null) ?? [];

  return (
    <div className="relative w-full h-full bg-void overflow-hidden">
      <svg
        ref={svgRef}
        className="w-full h-full"
        viewBox="-600 -600 1200 1200"
        preserveAspectRatio="xMidYMid meet"
        onClick={(e) => { if (e.target === svgRef.current) { setSelected(null); } }}
      >
        <g ref={groupRef}>
          {/* Starfield */}
          {Array.from({ length: 80 }, (_, starIndex) => {
            const sx = ((starIndex * 7919 + 1) % 1200) - 600;
            const sy = ((starIndex * 6271 + 3) % 1200) - 600;
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

          {/* Zone arcs */}
          {zoneBodies.map((body) => {
            if (!body.zone) { return null; }
            const parentAbs = bodyAbsolutes[body.parent ?? ''] ?? bodyAbsolutes[body.id];
            if (!parentAbs) { return null; }
            const center = toSvgPos(parentAbs);
            const rMin = toSvg(body.zone.radius_min_au_um);
            const rMax = toSvg(body.zone.radius_max_au_um);
            if (rMax < 0.1) { return null; }
            const fillColor = ZONE_COLORS[body.zone.resource_class] ?? ZONE_COLORS.Mixed;
            const strokeColor = ZONE_STROKES[body.zone.resource_class] ?? ZONE_STROKES.Mixed;
            return (
              <path
                key={`zone-${body.id}`}
                d={zoneArcPath(center.x, center.y, rMin, rMax, body.zone.angle_start_mdeg, body.zone.angle_span_mdeg)}
                fill={fillColor}
                stroke={strokeColor}
                strokeWidth={0.5}
                fillRule="evenodd"
              />
            );
          })}

          {/* Orbital body markers */}
          {config?.bodies.map((body) => {
            const abs = bodyAbsolutes[body.id];
            if (!abs) { return null; }
            const { x, y } = toSvgPos(abs);
            const color = BODY_COLORS[body.body_type] ?? '#888';
            if (body.body_type === 'Zone' || body.body_type === 'Belt') { return null; }

            const radius = body.body_type === 'Star' ? 8 : body.body_type === 'Moon' ? 3 : 5;
            return (
              <g key={`body-${body.id}`}>
                <circle cx={x} cy={y} r={radius} fill={color} opacity={0.9} />
                {body.body_type === 'Star' && (
                  <circle cx={x} cy={y} r={radius * 1.5} fill="none" stroke={color} opacity={0.2} strokeWidth={3} />
                )}
                <text
                  x={x}
                  y={y - radius - 4}
                  textAnchor="middle"
                  fill="var(--color-fg)"
                  fontSize={8}
                  fontFamily="monospace"
                  opacity={0.7}
                >
                  {body.name}
                </text>
              </g>
            );
          })}

          {/* Stations */}
          {snapshot && Object.values(snapshot.stations).map((station) => {
            const { x, y } = toSvgPos(entityAbsolute(station.position, bodyAbsolutes));
            return (
              <rect
                key={station.id}
                data-entity-type="station"
                data-entity-id={station.id}
                x={x - 5}
                y={y - 5}
                width={10}
                height={10}
                fill="var(--color-accent)"
                transform={`rotate(45 ${x} ${y})`}
                stroke={selected?.id === station.id ? 'var(--color-bright)' : undefined}
                strokeWidth={selected?.id === station.id ? 1.5 : undefined}
                className="cursor-pointer"
                {...entityMouseHandlers('station', station.id)}
                onClick={() => setSelected({ type: 'station', id: station.id })}
              />
            );
          })}

          {/* Ships */}
          {snapshot && Object.values(snapshot.ships).map((ship) => {
            const { x, y } = shipSvgPos(ship);
            return (
              <polygon
                key={ship.id}
                data-entity-type="ship"
                data-entity-id={ship.id}
                points={`${x},${y - 5} ${x - 4},${y + 3} ${x + 4},${y + 3}`}
                fill={shipColor(ship.task)}
                stroke={selected?.id === ship.id ? 'var(--color-bright)' : undefined}
                strokeWidth={selected?.id === ship.id ? 1.5 : undefined}
                className="cursor-pointer"
                {...entityMouseHandlers('ship', ship.id)}
                onClick={() => setSelected({ type: 'ship', id: ship.id })}
              />
            );
          })}

          {/* Asteroids */}
          {snapshot && Object.values(snapshot.asteroids).map((asteroid) => {
            const { x, y } = toSvgPos(entityAbsolute(asteroid.position, bodyAbsolutes));
            const massKg = asteroid.mass_kg ?? 1000;
            const size = Math.max(2.5, Math.min(8, Math.log10(massKg)));
            const matchedTag = asteroid.anomaly_tags.find((t: string) => TAG_COLORS[t]);
            const asteroidFill = matchedTag
              ? tagColor(matchedTag)
              : '#8a8e98';

            return (
              <circle
                key={asteroid.id}
                data-entity-type="asteroid"
                data-entity-id={asteroid.id}
                cx={x}
                cy={y}
                r={size}
                fill={asteroidFill}
                opacity={0.9}
                stroke={selected?.id === asteroid.id ? 'var(--color-bright)' : undefined}
                strokeWidth={selected?.id === asteroid.id ? 1.5 : undefined}
                className="cursor-pointer"
                {...entityMouseHandlers('asteroid', asteroid.id)}
                onClick={() => setSelected({ type: 'asteroid', id: asteroid.id })}
              />
            );
          })}

          {/* Scan sites */}
          {snapshot && snapshot.scan_sites.map((site) => {
            const { x, y } = toSvgPos(entityAbsolute(site.position, bodyAbsolutes));
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
                  r={5}
                  fill="var(--color-edge)"
                  stroke={isSelected ? 'var(--color-bright)' : 'var(--color-muted)'}
                  strokeWidth={isSelected ? 1 : 0.5}
                  opacity={0.8}
                />
                <text
                  x={x}
                  y={y}
                  textAnchor="middle"
                  dominantBaseline="central"
                  fill={isSelected ? 'var(--color-bright)' : 'var(--color-fg)'}
                  fontSize={7}
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
        if (!entity) { return null; }
        const hoveredAbs = getEntityAbsolute(hovered);
        const selectedAbs = selected ? getEntityAbsolute(selected) : null;
        return (
          <Tooltip x={hovered.screenX} y={hovered.screenY}>
            <div className="text-accent">{entity.data.id}</div>
            <div className="text-dim">{entity.type}</div>
            {hoveredAbs && selectedAbs && selected?.id !== hovered.id && config && (() => {
              const dist = distanceAuUm(hoveredAbs, selectedAbs);
              return (
                <div className="text-muted">
                  {formatDistance(dist)} (~{estimateTravelTicks(dist, config)} ticks)
                </div>
              );
            })()}
          </Tooltip>
        );
      })()}

      {selected && snapshot && (() => {
        const entity = lookupEntity(selected);
        if (!entity) { return null; }
        return <DetailCard entity={entity} onClose={() => setSelected(null)} />;
      })()}
    </div>
  );
}
