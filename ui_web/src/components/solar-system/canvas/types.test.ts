import { describe, expect, it } from 'vitest';

import { auUmToWorld, getLodTier, smoothStep } from './types';

describe('auUmToWorld', () => {
  it('converts 1 AU (1_000_000 uAU) to 200 world units', () => {
    expect(auUmToWorld(1_000_000)).toBe(200);
  });

  it('converts 0 to 0', () => {
    expect(auUmToWorld(0)).toBe(0);
  });
});

describe('getLodTier', () => {
  it('returns system for low zoom', () => {
    expect(getLodTier(0.05)).toBe('system');
    expect(getLodTier(0.14)).toBe('system');
  });

  it('returns region for medium zoom', () => {
    expect(getLodTier(0.15)).toBe('region');
    expect(getLodTier(0.5)).toBe('region');
    expect(getLodTier(0.79)).toBe('region');
  });

  it('returns local for high zoom', () => {
    expect(getLodTier(0.8)).toBe('local');
    expect(getLodTier(5.0)).toBe('local');
  });
});

describe('smoothStep', () => {
  it('returns 0 below fadeIn threshold', () => {
    expect(smoothStep(0.1, 0.2, 0.5)).toBe(0);
    expect(smoothStep(0.2, 0.2, 0.5)).toBe(0);
  });

  it('returns 1 at or above fullIn threshold', () => {
    expect(smoothStep(0.5, 0.2, 0.5)).toBe(1);
    expect(smoothStep(1.0, 0.2, 0.5)).toBe(1);
  });

  it('returns smooth intermediate values', () => {
    const mid = smoothStep(0.35, 0.2, 0.5);
    expect(mid).toBeGreaterThan(0);
    expect(mid).toBeLessThan(1);
    // At t=0.5 (midpoint), smoothstep = 0.5
    expect(mid).toBeCloseTo(0.5, 1);
  });

  it('is monotonically increasing', () => {
    const a = smoothStep(0.25, 0.2, 0.5);
    const b = smoothStep(0.35, 0.2, 0.5);
    const c = smoothStep(0.45, 0.2, 0.5);
    expect(b).toBeGreaterThan(a);
    expect(c).toBeGreaterThan(b);
  });
});
