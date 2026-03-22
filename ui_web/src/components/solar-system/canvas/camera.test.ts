import { describe, expect, it } from 'vitest';

import { lerpCamera, screenToWorld, worldToScreen } from './camera';
import type { Camera } from './types';

describe('worldToScreen / screenToWorld roundtrip', () => {
  it('converts world to screen and back', () => {
    const camera: Camera = { x: 100, y: 50, zoom: 2 };
    const { sx, sy } = worldToScreen(200, 150, camera, 800, 600);
    const { wx, wy } = screenToWorld(sx, sy, camera, 800, 600);
    expect(wx).toBeCloseTo(200, 5);
    expect(wy).toBeCloseTo(150, 5);
  });

  it('places camera center at screen center', () => {
    const camera: Camera = { x: 0, y: 0, zoom: 1 };
    const { sx, sy } = worldToScreen(0, 0, camera, 1000, 800);
    expect(sx).toBe(500);
    expect(sy).toBe(400);
  });

  it('respects zoom factor', () => {
    const camera: Camera = { x: 0, y: 0, zoom: 2 };
    const { sx, sy } = worldToScreen(100, 0, camera, 1000, 800);
    // 100 * 2 = 200 pixels from center (500)
    expect(sx).toBe(700);
    expect(sy).toBe(400);
  });
});

describe('lerpCamera', () => {
  it('converges current toward target', () => {
    const current: Camera = { x: 0, y: 0, zoom: 1 };
    const target: Camera = { x: 100, y: 100, zoom: 2 };
    const moved = lerpCamera(current, target);
    expect(moved).toBe(true);
    expect(current.x).toBeGreaterThan(0);
    expect(current.y).toBeGreaterThan(0);
    expect(current.zoom).toBeGreaterThan(1);
  });

  it('returns false when already at target', () => {
    const current: Camera = { x: 50, y: 50, zoom: 1 };
    const target: Camera = { x: 50, y: 50, zoom: 1 };
    const moved = lerpCamera(current, target);
    expect(moved).toBe(false);
  });
});
