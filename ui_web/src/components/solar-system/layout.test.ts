import { describe, expect, it } from 'vitest';

import { angleFromId, polarToCartesian, transitPosition, ringRadiusForNode } from './layout';

describe('angleFromId', () => {
  it('returns a number between 0 and 2*PI', () => {
    const angle = angleFromId('asteroid_0001');
    expect(angle).toBeGreaterThanOrEqual(0);
    expect(angle).toBeLessThan(Math.PI * 2);
  });

  it('returns the same angle for the same ID', () => {
    expect(angleFromId('asteroid_0001')).toBe(angleFromId('asteroid_0001'));
  });

  it('returns different angles for different IDs', () => {
    expect(angleFromId('asteroid_0001')).not.toBe(angleFromId('asteroid_0002'));
  });
});

describe('polarToCartesian', () => {
  it('converts radius and angle 0 to (radius, 0)', () => {
    const { x, y } = polarToCartesian(100, 0);
    expect(x).toBeCloseTo(100);
    expect(y).toBeCloseTo(0);
  });

  it('converts angle PI/2 to (0, radius)', () => {
    const { x, y } = polarToCartesian(100, Math.PI / 2);
    expect(x).toBeCloseTo(0);
    expect(y).toBeCloseTo(100);
  });
});

describe('transitPosition', () => {
  it('returns origin position at progress 0', () => {
    const pos = transitPosition(
      { radius: 100, angle: 0 },
      { radius: 200, angle: Math.PI },
      0,
    );
    expect(pos.x).toBeCloseTo(100);
    expect(pos.y).toBeCloseTo(0);
  });

  it('returns destination position at progress 1', () => {
    const pos = transitPosition(
      { radius: 100, angle: 0 },
      { radius: 200, angle: Math.PI },
      1,
    );
    expect(pos.x).toBeCloseTo(-200);
    expect(pos.y).toBeCloseTo(0, 0);
  });

  it('returns midpoint at progress 0.5', () => {
    const pos = transitPosition(
      { radius: 100, angle: 0 },
      { radius: 300, angle: 0 },
      0.5,
    );
    expect(pos.x).toBeCloseTo(200);
    expect(pos.y).toBeCloseTo(0);
  });
});

describe('ringRadiusForNode', () => {
  it('returns correct radius for known nodes', () => {
    expect(ringRadiusForNode('node_earth_orbit')).toBe(100);
    expect(ringRadiusForNode('node_belt_inner')).toBe(200);
    expect(ringRadiusForNode('node_belt_mid')).toBe(300);
    expect(ringRadiusForNode('node_belt_outer')).toBe(400);
  });

  it('returns fallback for unknown nodes', () => {
    expect(ringRadiusForNode('unknown_node')).toBe(250);
  });
});
