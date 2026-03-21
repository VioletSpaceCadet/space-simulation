import type { AbsolutePos, Position, SolarSystemConfig } from '../types';

/** Convert milli-degrees to radians. */
export function mdegToRad(mdeg: number): number {
  return (mdeg / 360_000) * 2 * Math.PI;
}

/** Convert radians to milli-degrees. */
export function radToMdeg(rad: number): number {
  return (rad / (2 * Math.PI)) * 360_000;
}

/** Wrapping addition of two milli-degree angles (mod 360,000). */
export function addAngleMdeg(a: number, b: number): number {
  return ((a + b) % 360_000 + 360_000) % 360_000;
}

/** Shortest signed arc from `from` to `to` in milli-degrees (-180,000..180,000]. */
export function signedDeltaMdeg(from: number, to: number): number {
  let delta = ((to - from) % 360_000 + 360_000) % 360_000;
  if (delta > 180_000) {
    delta -= 360_000;
  }
  return delta;
}

/** Check if `angle` lies within the arc starting at `start` spanning `span` milli-degrees. */
export function withinSpanMdeg(angle: number, start: number, span: number): boolean {
  const normalized = ((angle - start) % 360_000 + 360_000) % 360_000;
  return normalized <= span;
}

/** Convert polar (radius in µAU, angle in milli-degrees) to cartesian (x, y) µAU offset. */
export function polarToCartAuUm(radiusAuUm: number, angleMdeg: number): { x: number; y: number } {
  const rad = mdegToRad(angleMdeg);
  return {
    x: radiusAuUm * Math.cos(rad),
    y: radiusAuUm * Math.sin(rad),
  };
}

/** Compute absolute position for an entity given its Position and body_absolutes map. */
export function entityAbsolute(
  position: Position,
  bodyAbsolutes: Record<string, AbsolutePos>,
): AbsolutePos {
  const parentAbs = bodyAbsolutes[position.parent_body];
  if (!parentAbs) {
    return { x_au_um: 0, y_au_um: 0 };
  }
  const offset = polarToCartAuUm(position.radius_au_um, position.angle_mdeg);
  return {
    x_au_um: parentAbs.x_au_um + offset.x,
    y_au_um: parentAbs.y_au_um + offset.y,
  };
}

/** Distance between two absolute positions in µAU. */
export function distanceAuUm(a: AbsolutePos, b: AbsolutePos): number {
  const dx = a.x_au_um - b.x_au_um;
  const dy = a.y_au_um - b.y_au_um;
  return Math.hypot(dx, dy);
}

/** Convert µAU distance to AU (human-readable). */
export function auUmToAu(auUm: number): number {
  return auUm / 1_000_000;
}

/** Estimate travel ticks for a given distance in µAU. */
export function estimateTravelTicks(
  distAuUm: number,
  config: Pick<SolarSystemConfig, 'ticks_per_au' | 'min_transit_ticks'>,
): number {
  const ticks = Math.ceil((distAuUm / 1_000_000) * config.ticks_per_au);
  return Math.max(ticks, config.min_transit_ticks);
}

/** Interpolate a ship's absolute position during transit. */
export function shipTransitAbsolute(
  originAbs: AbsolutePos,
  destAbs: AbsolutePos,
  progress: number,
): AbsolutePos {
  const t = Math.max(0, Math.min(1, progress));
  return {
    x_au_um: originAbs.x_au_um + (destAbs.x_au_um - originAbs.x_au_um) * t,
    y_au_um: originAbs.y_au_um + (destAbs.y_au_um - originAbs.y_au_um) * t,
  };
}
