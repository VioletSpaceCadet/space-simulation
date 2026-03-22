import {
  BODY_COLORS,
  IDLE_COLOR,
  MAP_COLORS,
  TAG_COLORS,
  ZONE_COLORS,
  ZONE_STROKES,
  shipTaskColor,
  tagColor,
} from '../../../config/theme';
import type {
  AbsolutePos,
  AsteroidState,
  OrbitalBodyDef,
  Position,
  ScanSite,
  ShipState,
  SolarSystemConfig,
  StationState,
} from '../../../types';
import { getTaskKind } from '../../../utils';
import { entityAbsolute, mdegToRad, shipTransitAbsolute } from '../../../utils/spatial';

import { worldToScreen } from './camera';
import type { Camera } from './types';
import { SIZE_CAPS, auUmToWorld, smoothStep } from './types';

export interface DrawContext {
  ctx: CanvasRenderingContext2D;
  camera: Camera;
  width: number;
  height: number;
  config: SolarSystemConfig;
  bodyAbsolutes: Record<string, AbsolutePos>;
  currentTick: number;
}

function toWorld(abs: AbsolutePos): { x: number; y: number } {
  return { x: auUmToWorld(abs.x_au_um), y: auUmToWorld(abs.y_au_um) };
}

function toScreen(
  abs: AbsolutePos,
  dc: DrawContext,
): { sx: number; sy: number } {
  const w = toWorld(abs);
  return worldToScreen(w.x, w.y, dc.camera, dc.width, dc.height);
}

function entitySize(
  baseSize: number,
  zoom: number,
  caps: { min: number; max: number; scale: number },
): number {
  return Math.max(caps.min, Math.min(caps.max, baseSize * zoom * caps.scale));
}

/** Main draw function — clears canvas and renders all map layers. */
export function drawMap(
  dc: DrawContext,
  stations: StationState[],
  ships: ShipState[],
  asteroids: AsteroidState[],
  scanSites: ScanSite[],
): void {
  const { ctx, width, height } = dc;

  ctx.clearRect(0, 0, width, height);
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = 'high';

  drawOrbitRings(dc, dc.config.bodies);
  drawZones(dc, dc.config.bodies);
  drawBodies(dc, dc.config.bodies, stations);
  drawTransitLines(dc, ships);
  drawStations(dc, stations);
  drawShips(dc, ships);
  drawAsteroids(dc, asteroids);
  drawScanSites(dc, scanSites);
}

function drawOrbitRings(dc: DrawContext, bodies: OrbitalBodyDef[]): void {
  const { ctx, camera } = dc;

  for (const body of bodies) {
    if (body.body_type === 'Zone' || body.body_type === 'Belt') { continue; }
    if (!body.parent) { continue; }

    const parentAbs = dc.bodyAbsolutes[body.parent];
    if (!parentAbs) { continue; }

    const center = toScreen(parentAbs, dc);
    const radiusWorld = auUmToWorld(body.radius_au_um);
    const radiusPx = radiusWorld * camera.zoom;

    if (radiusPx < 3) { continue; }

    // Fade in based on body type — planets fade at system zoom, moons at region
    const lodMin = body.body_type === 'Moon' ? 0.8 : 0.12;
    const alpha = smoothStep(camera.zoom, lodMin * 0.5, lodMin * 1.5);
    if (alpha < 0.01) { continue; }

    ctx.globalAlpha = alpha * 0.4;
    ctx.beginPath();
    ctx.arc(center.sx, center.sy, radiusPx, 0, Math.PI * 2);
    ctx.strokeStyle = MAP_COLORS.orbitRing;
    ctx.lineWidth = 0.8;
    ctx.setLineDash([4, 6]);
    ctx.stroke();
    ctx.setLineDash([]);

    if (radiusPx > 50) {
      ctx.font = '11px monospace';
      ctx.fillStyle = MAP_COLORS.orbitLabel;
      ctx.textAlign = 'center';
      ctx.fillText(`${body.name} orbit`, center.sx, center.sy - radiusPx - 4);
    }
    ctx.globalAlpha = 1;
  }
}

function drawZones(dc: DrawContext, bodies: OrbitalBodyDef[]): void {
  const { ctx, camera } = dc;

  for (const body of bodies) {
    if (!body.zone) { continue; }

    const parentAbs =
      dc.bodyAbsolutes[body.parent ?? ''] ?? dc.bodyAbsolutes[body.id];
    if (!parentAbs) { continue; }

    const center = toScreen(parentAbs, dc);
    const rMin = auUmToWorld(body.zone.radius_min_au_um) * camera.zoom;
    const rMax = auUmToWorld(body.zone.radius_max_au_um) * camera.zoom;

    if (rMax < 3) { continue; }

    // Zones fade in at system zoom
    const alpha = smoothStep(camera.zoom, 0.04, 0.16);
    if (alpha < 0.01) { continue; }

    const fillColor =
      ZONE_COLORS[body.zone.resource_class] ?? ZONE_COLORS.Mixed;
    const strokeColor =
      ZONE_STROKES[body.zone.resource_class] ?? ZONE_STROKES.Mixed;

    ctx.globalAlpha = alpha;
    ctx.beginPath();
    if (body.zone.angle_span_mdeg >= 360_000) {
      ctx.arc(center.sx, center.sy, rMax, 0, Math.PI * 2);
      ctx.arc(center.sx, center.sy, rMin, 0, Math.PI * 2, true);
    } else {
      const startRad = mdegToRad(body.zone.angle_start_mdeg);
      const endRad = mdegToRad(
        body.zone.angle_start_mdeg + body.zone.angle_span_mdeg,
      );
      ctx.arc(center.sx, center.sy, rMax, startRad, endRad);
      ctx.arc(center.sx, center.sy, rMin, endRad, startRad, true);
      ctx.closePath();
    }
    ctx.fillStyle = fillColor;
    ctx.fill();
    ctx.strokeStyle = strokeColor;
    ctx.lineWidth = 0.8;
    ctx.stroke();

    // Zone label — only at region+ zoom
    const labelAlpha = smoothStep(camera.zoom, 0.2, 0.5);
    if (labelAlpha > 0.01 && rMax > 50) {
      const midR = (rMin + rMax) / 2;
      const startRad = body.zone.angle_span_mdeg >= 360_000
        ? -Math.PI / 4
        : mdegToRad(body.zone.angle_start_mdeg + body.zone.angle_span_mdeg / 2);
      ctx.globalAlpha = alpha * labelAlpha * 0.7;
      ctx.font = '11px monospace';
      ctx.fillStyle = MAP_COLORS.bodyLabelOther;
      ctx.textAlign = 'center';
      ctx.fillText(
        body.name,
        center.sx + midR * Math.cos(startRad),
        center.sy + midR * Math.sin(startRad),
      );
    }

    ctx.globalAlpha = 1;
  }
}

function drawBodies(
  dc: DrawContext,
  bodies: OrbitalBodyDef[],
  stations: StationState[],
): void {
  const { ctx, camera } = dc;

  for (const body of bodies) {
    if (body.body_type === 'Zone' || body.body_type === 'Belt') { continue; }

    const abs = dc.bodyAbsolutes[body.id];
    if (!abs) { continue; }

    const { sx, sy } = toScreen(abs, dc);
    const color = BODY_COLORS[body.body_type] ?? '#888';

    // LOD fade — stars always visible, planets/moons fade in at appropriate zoom
    const lodMin = body.body_type === 'Star' ? 0
      : body.body_type === 'Moon' ? 0.8
        : 0.12;
    const bodyAlpha = body.body_type === 'Star' ? 1
      : smoothStep(camera.zoom, lodMin * 0.5, lodMin * 1.5);
    if (bodyAlpha < 0.01 && body.body_type !== 'Star') { continue; }

    const caps =
      body.body_type === 'Star'
        ? SIZE_CAPS.Star
        : body.body_type === 'Moon'
          ? SIZE_CAPS.Moon
          : SIZE_CAPS.Planet;
    const baseRadius =
      body.body_type === 'Star' ? 10 : body.body_type === 'Moon' ? 3 : 5;
    const screenR = entitySize(baseRadius, camera.zoom, caps);

    // Body circle
    ctx.globalAlpha = body.body_type === 'Star' ? 1 : bodyAlpha;
    ctx.beginPath();
    ctx.arc(sx, sy, screenR, 0, Math.PI * 2);
    ctx.fillStyle = color;
    ctx.fill();

    // Star glow ring + radial glow
    if (body.body_type === 'Star') {
      ctx.beginPath();
      ctx.arc(sx, sy, screenR * 1.6, 0, Math.PI * 2);
      ctx.strokeStyle = `${color}30`;
      ctx.lineWidth = 2;
      ctx.stroke();

      const glowR = Math.max(25, 70 * camera.zoom);
      const grad = ctx.createRadialGradient(sx, sy, 0, sx, sy, glowR);
      grad.addColorStop(0, MAP_COLORS.starGlow);
      grad.addColorStop(0.4, MAP_COLORS.starGlowMid);
      grad.addColorStop(1, 'rgba(245,200,66,0)');
      ctx.beginPath();
      ctx.arc(sx, sy, glowR, 0, Math.PI * 2);
      ctx.fillStyle = grad;
      ctx.fill();
    }

    // Body label — with LOD-based fade and station collision
    const labelAlpha = body.body_type === 'Star' ? 1
      : smoothStep(camera.zoom, lodMin, lodMin * 2);
    if (labelAlpha > 0.01) {
      let hideForStation = false;
      if (body.body_type === 'Planet' || body.body_type === 'Moon') {
        for (const st of stations) {
          const stAbs = entityAbsolute(st.position, dc.bodyAbsolutes);
          const stScreen = toScreen(stAbs, dc);
          if (Math.hypot(sx - stScreen.sx, sy - stScreen.sy) < 40) {
            hideForStation = camera.zoom > 0.5;
            break;
          }
        }
      }

      if (!hideForStation) {
        ctx.globalAlpha = labelAlpha * (body.body_type === 'Star' ? 0.8 : 0.5);
        ctx.font = `${body.body_type === 'Star' ? 12 : 11}px monospace`;
        ctx.fillStyle = body.body_type === 'Star' ? MAP_COLORS.bodyLabelStar : MAP_COLORS.bodyLabelOther;
        if (body.body_type === 'Moon') {
          ctx.textAlign = 'left';
          ctx.fillText(body.name, sx + screenR + 4, sy + 3);
        } else {
          ctx.textAlign = 'center';
          ctx.fillText(body.name, sx, sy - screenR - 6);
        }
      }
    }

    ctx.globalAlpha = 1;
  }
}

function drawTransitLines(dc: DrawContext, ships: ShipState[]): void {
  const { ctx, camera } = dc;

  // Transit lines visible at REGION+ zoom
  const transitAlpha = smoothStep(camera.zoom, 0.15, 0.4);
  if (transitAlpha < 0.01) { return; }

  for (const ship of ships) {
    const taskKind = getTaskKind(ship.task);
    if (taskKind !== 'Transit' || !ship.task || !('Transit' in ship.task.kind)) { continue; }

    const transit = (ship.task.kind as { Transit: { destination: Position } }).Transit;
    const originAbs = entityAbsolute(ship.position, dc.bodyAbsolutes);
    const destAbs = entityAbsolute(transit.destination, dc.bodyAbsolutes);
    const progress = ship.task.eta_tick > ship.task.started_tick
      ? (dc.currentTick - ship.task.started_tick) / (ship.task.eta_tick - ship.task.started_tick)
      : 1;

    const from = toScreen(originAbs, dc);
    const to = toScreen(destAbs, dc);
    const progX = from.sx + (to.sx - from.sx) * Math.max(0, Math.min(1, progress));
    const progY = from.sy + (to.sy - from.sy) * Math.max(0, Math.min(1, progress));
    const color = shipTaskColor('Transit');

    // Full path — dim dashed line
    ctx.globalAlpha = transitAlpha * 0.3;
    ctx.beginPath();
    ctx.moveTo(from.sx, from.sy);
    ctx.lineTo(to.sx, to.sy);
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.5;
    ctx.setLineDash([8, 5]);
    ctx.stroke();
    ctx.setLineDash([]);

    // Completed portion — solid brighter line
    ctx.globalAlpha = transitAlpha * 0.6;
    ctx.beginPath();
    ctx.moveTo(from.sx, from.sy);
    ctx.lineTo(progX, progY);
    ctx.strokeStyle = color;
    ctx.lineWidth = 2;
    ctx.stroke();

    // Progress pip — small glowing dot
    ctx.globalAlpha = transitAlpha;
    ctx.beginPath();
    ctx.arc(progX, progY, 2.5, 0, Math.PI * 2);
    ctx.fillStyle = color;
    ctx.fill();
    // Outer glow
    ctx.beginPath();
    ctx.arc(progX, progY, 5, 0, Math.PI * 2);
    ctx.strokeStyle = `${color}33`;
    ctx.lineWidth = 0.8;
    ctx.stroke();

    // Destination marker — small hollow circle
    ctx.globalAlpha = transitAlpha * 0.4;
    ctx.beginPath();
    ctx.arc(to.sx, to.sy, 3, 0, Math.PI * 2);
    ctx.strokeStyle = color;
    ctx.lineWidth = 1;
    ctx.stroke();

    // ETA label at LOCAL zoom
    const etaFade = smoothStep(camera.zoom, 0.5, 1.0);
    if (etaFade > 0.01) {
      const etaLeft = Math.max(0, ship.task.eta_tick - dc.currentTick);
      const angle = Math.atan2(to.sy - from.sy, to.sx - from.sx);
      const labelX = progX + 12 * Math.cos(angle + Math.PI / 2);
      const labelY = progY + 12 * Math.sin(angle + Math.PI / 2);
      ctx.globalAlpha = transitAlpha * etaFade * 0.7;
      ctx.font = '11px monospace';
      ctx.fillStyle = color;
      ctx.textAlign = 'center';
      ctx.fillText(`${Math.round(progress * 100)}% \u00b7 ${ticksToTime(etaLeft)}`, labelX, labelY);
    }
  }
  ctx.globalAlpha = 1;
  ctx.setLineDash([]);
}

function ticksToTime(ticks: number): string {
  if (ticks < 24) { return `${ticks}h`; }
  const days = Math.floor(ticks / 24);
  return `${days}d ${ticks % 24}h`;
}

function drawStations(dc: DrawContext, stations: StationState[]): void {
  const { ctx, camera } = dc;

  // Stations visible from system zoom as dots, full at region+
  const stationAlpha = smoothStep(camera.zoom, 0.1, 0.3);

  for (const station of stations) {
    const abs = entityAbsolute(station.position, dc.bodyAbsolutes);
    const { sx, sy } = toScreen(abs, dc);

    // At system zoom: tiny dot
    if (camera.zoom < 0.1) {
      ctx.beginPath();
      ctx.arc(sx, sy, 2, 0, Math.PI * 2);
      ctx.fillStyle = MAP_COLORS.stationAccent;
      ctx.globalAlpha = 0.5;
      ctx.fill();
      ctx.globalAlpha = 1;
      continue;
    }

    const size = entitySize(4, camera.zoom, SIZE_CAPS.Station);

    // Diamond shape
    ctx.save();
    ctx.globalAlpha = stationAlpha;
    ctx.translate(sx, sy);
    ctx.rotate(Math.PI / 4);
    ctx.fillStyle = MAP_COLORS.stationAccent;
    ctx.fillRect(-size / 2, -size / 2, size, size);
    ctx.restore();

    // Subtle pulse ring
    const pulse = 0.2 + 0.15 * Math.sin(performance.now() * 0.003);
    ctx.beginPath();
    ctx.arc(sx, sy, size + 3, 0, Math.PI * 2);
    ctx.strokeStyle = `${MAP_COLORS.stationPulse}${pulse * stationAlpha})`;
    ctx.lineWidth = 0.8;
    ctx.stroke();

    // Label — fade in at region zoom
    const labelAlpha = smoothStep(camera.zoom, 0.3, 0.8);
    if (labelAlpha > 0.01) {
      ctx.globalAlpha = labelAlpha;
      ctx.font = '11px sans-serif';
      ctx.fillStyle = MAP_COLORS.stationAccent;
      ctx.textAlign = 'left';
      ctx.fillText(station.id, sx + size + 6, sy + 3);
    }
    ctx.globalAlpha = 1;
  }
}

function drawShips(dc: DrawContext, ships: ShipState[]): void {
  const { ctx, camera } = dc;

  // Ships visible as dots at system, triangles at region+
  const shipAlpha = smoothStep(camera.zoom, 0.15, 0.4);

  for (const ship of ships) {
    const abs = shipAbsolutePos(ship, dc);
    const { sx, sy } = toScreen(abs, dc);
    const kind = getTaskKind(ship.task) ?? 'idle';
    const color = shipTaskColor(kind);

    // At system zoom: tiny dot
    if (camera.zoom < 0.15) {
      ctx.beginPath();
      ctx.arc(sx, sy, 1.5, 0, Math.PI * 2);
      ctx.fillStyle = color;
      ctx.globalAlpha = 0.4;
      ctx.fill();
      ctx.globalAlpha = 1;
      continue;
    }

    const size = entitySize(3.5, camera.zoom, SIZE_CAPS.Ship);

    // Triangle shape
    ctx.globalAlpha = shipAlpha;
    ctx.beginPath();
    ctx.moveTo(sx, sy - size);
    ctx.lineTo(sx - size * 0.6, sy + size * 0.5);
    ctx.lineTo(sx + size * 0.6, sy + size * 0.5);
    ctx.closePath();
    ctx.fillStyle = color;
    ctx.fill();

    // Ship name label — local zoom only
    const nameFade = smoothStep(camera.zoom, 0.6, 1.5);
    if (nameFade > 0.01) {
      ctx.globalAlpha = shipAlpha * nameFade * 0.6;
      ctx.font = '11px monospace';
      ctx.fillStyle = color;
      ctx.textAlign = 'center';
      ctx.fillText(ship.id, sx, sy + size + 10);
    }
    ctx.globalAlpha = 1;
  }
}

/** Compute ship absolute position, handling transit interpolation. */
export function shipAbsolutePos(
  ship: ShipState,
  dc: Pick<DrawContext, 'bodyAbsolutes' | 'currentTick'>,
): AbsolutePos {
  const taskKind = getTaskKind(ship.task);

  if (taskKind === 'Transit' && ship.task && 'Transit' in ship.task.kind) {
    const transit = (
      ship.task.kind as { Transit: { destination: Position } }
    ).Transit;
    const originAbs = entityAbsolute(ship.position, dc.bodyAbsolutes);
    const destAbs = entityAbsolute(transit.destination, dc.bodyAbsolutes);
    const progress =
      ship.task.eta_tick > ship.task.started_tick
        ? (dc.currentTick - ship.task.started_tick) /
          (ship.task.eta_tick - ship.task.started_tick)
        : 1;
    return shipTransitAbsolute(originAbs, destAbs, progress);
  }

  return entityAbsolute(ship.position, dc.bodyAbsolutes);
}

function drawAsteroids(dc: DrawContext, asteroids: AsteroidState[]): void {
  const { ctx, camera } = dc;

  // Asteroids fade in at region zoom
  const astAlpha = smoothStep(camera.zoom, 0.25, 0.7);
  if (astAlpha < 0.01) { return; }

  for (const asteroid of asteroids) {
    const abs = entityAbsolute(asteroid.position, dc.bodyAbsolutes);
    const { sx, sy } = toScreen(abs, dc);

    const massKg = asteroid.mass_kg ?? 1000;
    const size = entitySize(
      Math.log10(massKg),
      camera.zoom,
      SIZE_CAPS.Asteroid,
    );

    const matchedTag = asteroid.anomaly_tags.find(
      (t: string) => TAG_COLORS[t],
    );
    const color = matchedTag ? tagColor(matchedTag) : IDLE_COLOR;

    // Irregular 6-sided polygon with seeded wobble
    ctx.globalAlpha = astAlpha * 0.85;
    ctx.beginPath();
    const sides = 6;
    for (let i = 0; i < sides; i++) {
      const angle = (i / sides) * Math.PI * 2;
      const wobble = 0.7 + 0.3 * Math.sin(i * 2.5 + massKg * 0.001);
      const px = sx + size * wobble * Math.cos(angle);
      const py = sy + size * wobble * Math.sin(angle);
      if (i === 0) {
        ctx.moveTo(px, py);
      } else {
        ctx.lineTo(px, py);
      }
    }
    ctx.closePath();
    ctx.fillStyle = color;
    ctx.fill();
    ctx.globalAlpha = 1;
  }
}

function drawScanSites(dc: DrawContext, scanSites: ScanSite[]): void {
  const { ctx, camera } = dc;

  // Scan sites fade in at region zoom
  const siteAlpha = smoothStep(camera.zoom, 0.3, 0.8);
  if (siteAlpha < 0.01) { return; }

  for (const site of scanSites) {
    const abs = entityAbsolute(site.position, dc.bodyAbsolutes);
    const { sx, sy } = toScreen(abs, dc);
    const r = entitySize(3.5, camera.zoom, SIZE_CAPS.ScanSite);

    ctx.globalAlpha = siteAlpha * 0.8;
    ctx.beginPath();
    ctx.arc(sx, sy, r, 0, Math.PI * 2);
    ctx.fillStyle = MAP_COLORS.scanSiteBg;
    ctx.fill();
    ctx.strokeStyle = MAP_COLORS.scanSiteStroke;
    ctx.lineWidth = 1;
    ctx.stroke();

    // Question mark
    ctx.font = 'bold 10px monospace';
    ctx.fillStyle = MAP_COLORS.scanSiteText;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText('?', sx, sy + 0.5);
    ctx.textBaseline = 'alphabetic';
    ctx.globalAlpha = 1;
  }
}
