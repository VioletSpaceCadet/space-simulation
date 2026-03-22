import type { Camera } from './types';
import { LERP_PAN, LERP_ZOOM } from './types';

/** Convert world position to screen position. */
export function worldToScreen(
  wx: number,
  wy: number,
  camera: Camera,
  viewWidth: number,
  viewHeight: number,
): { sx: number; sy: number } {
  return {
    sx: (wx - camera.x) * camera.zoom + viewWidth / 2,
    sy: (wy - camera.y) * camera.zoom + viewHeight / 2,
  };
}

/** Convert screen position to world position. */
export function screenToWorld(
  sx: number,
  sy: number,
  camera: Camera,
  viewWidth: number,
  viewHeight: number,
): { wx: number; wy: number } {
  return {
    wx: (sx - viewWidth / 2) / camera.zoom + camera.x,
    wy: (sy - viewHeight / 2) / camera.zoom + camera.y,
  };
}

/** Lerp camera toward target. Returns true if camera moved. */
export function lerpCamera(current: Camera, target: Camera): boolean {
  const zoomDiff = Math.abs(current.zoom - target.zoom);
  const posDiff = Math.abs(current.x - target.x) + Math.abs(current.y - target.y);

  if (zoomDiff < 0.0001 && posDiff < 0.001) {
    current.x = target.x;
    current.y = target.y;
    current.zoom = target.zoom;
    return false;
  }

  // Log-space lerp for zoom (constant perceptual speed)
  const logCurrent = Math.log(current.zoom);
  const logTarget = Math.log(target.zoom);
  current.zoom = Math.exp(logCurrent + (logTarget - logCurrent) * LERP_ZOOM);

  // Linear lerp for position, adaptive for large jumps
  const dx = target.x - current.x;
  const dy = target.y - current.y;
  const dist = Math.hypot(dx, dy);
  const rate = dist > 1000 ? Math.min(LERP_PAN * 3, 0.5) : LERP_PAN;
  current.x += dx * rate;
  current.y += dy * rate;

  return true;
}
