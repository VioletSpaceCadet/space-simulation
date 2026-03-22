/** Camera state — position in world units, zoom in pixels per world unit. */
export interface Camera {
  x: number;
  y: number;
  zoom: number;
}

/**
 * World coordinate scale factor.
 * 1 world unit = WORLD_SCALE uAU.
 * At WORLD_SCALE = 5000, 1 AU = 200 world units (matches mockup proportions).
 */
export const WORLD_SCALE = 5000;

/** Convert uAU to world units. */
export function auUmToWorld(auUm: number): number {
  return auUm / WORLD_SCALE;
}

/** Camera zoom constants. */
export const MIN_ZOOM = 0.00001;
export const MAX_ZOOM = 50;
export const ZOOM_IN_RATIO = 1.22;
export const ZOOM_OUT_RATIO = 0.82;

/** Camera interpolation rates. */
export const LERP_ZOOM = 0.18;
export const LERP_PAN = 0.12;

/** Starfield constants. */
export const STAR_TILE_SIZE = 1024;
export const PARALLAX_FACTOR = 0.05;

/** Size caps per entity type: { min, max, scale }. */
export const SIZE_CAPS = {
  Star: { min: 5, max: 14, scale: 0.5 },
  Planet: { min: 2, max: 8, scale: 0.5 },
  Moon: { min: 2, max: 4, scale: 0.5 },
  Station: { min: 3, max: 7, scale: 0.6 },
  Ship: { min: 3, max: 6, scale: 0.5 },
  Asteroid: { min: 2, max: 5, scale: 0.35 },
  ScanSite: { min: 3, max: 5, scale: 0.4 },
} as const;

/** Default initial camera view — centered on solar system. */
export const INITIAL_CAMERA: Camera = { x: 0, y: 0, zoom: 0.5 };

// --- LOD system ---

/** LOD tier names ordered from most zoomed-out to most zoomed-in. */
export type LodTier = 'system' | 'region' | 'local';

/** LOD tier zoom thresholds. */
export const LOD_THRESHOLDS = {
  /** Below this: SYSTEM tier. */
  region: 0.15,
  /** Above this: LOCAL tier. Between region and local: REGION tier. */
  local: 0.8,
} as const;

/** Compute the current LOD tier from camera zoom level. */
export function getLodTier(zoom: number): LodTier {
  if (zoom < LOD_THRESHOLDS.region) { return 'system'; }
  if (zoom < LOD_THRESHOLDS.local) { return 'region'; }
  return 'local';
}

/**
 * Hermite smoothstep: returns 0 when zoom <= fadeIn, 1 when zoom >= fullIn,
 * smooth S-curve between. Used for LOD opacity transitions.
 */
export function smoothStep(zoom: number, fadeIn: number, fullIn: number): number {
  if (zoom <= fadeIn) { return 0; }
  if (zoom >= fullIn) { return 1; }
  const t = (zoom - fadeIn) / (fullIn - fadeIn);
  return t * t * (3 - 2 * t);
}
