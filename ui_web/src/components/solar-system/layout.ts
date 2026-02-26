/**
 * Deterministic angle from an entity ID. Uses a simple hash to spread
 * entities around their orbital ring.
 */
export function angleFromId(id: string): number {
  let hash = 0;
  for (let i = 0; i < id.length; i++) {
    hash = ((hash << 5) - hash + id.charCodeAt(i)) | 0;
  }
  return ((hash >>> 0) / 0xffffffff) * Math.PI * 2;
}

export function polarToCartesian(radius: number, angle: number): { x: number; y: number } {
  return {
    x: radius * Math.cos(angle),
    y: radius * Math.sin(angle),
  };
}

/**
 * Interpolate between two polar positions for transit animation.
 */
export function transitPosition(
  origin: { radius: number; angle: number },
  destination: { radius: number; angle: number },
  progress: number,
): { x: number; y: number } {
  const t = Math.max(0, Math.min(1, progress));
  const radius = origin.radius + (destination.radius - origin.radius) * t;
  const angle = origin.angle + (destination.angle - origin.angle) * t;
  return polarToCartesian(radius, angle);
}

/** Node ID → ring radius lookup */
const RING_RADII: Record<string, number> = {
  node_earth_orbit: 100,
  node_belt_inner: 200,
  node_belt_mid: 300,
  node_belt_outer: 400,
};

export function ringRadiusForNode(nodeId: string): number {
  return RING_RADII[nodeId] ?? 250;
}
