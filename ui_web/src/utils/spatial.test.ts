import { describe, expect, it } from 'vitest';

import {
  addAngleMdeg,
  auUmToAu,
  distanceAuUm,
  entityAbsolute,
  estimateTravelTicks,
  mdegToRad,
  polarToCartAuUm,
  shipTransitAbsolute,
  signedDeltaMdeg,
  withinSpanMdeg,
} from './spatial';

describe('mdegToRad', () => {
  it('converts 0 mdeg to 0 rad', () => {
    expect(mdegToRad(0)).toBe(0);
  });

  it('converts 90_000 mdeg (90 deg) to PI/2', () => {
    expect(mdegToRad(90_000)).toBeCloseTo(Math.PI / 2);
  });

  it('converts 180_000 mdeg (180 deg) to PI', () => {
    expect(mdegToRad(180_000)).toBeCloseTo(Math.PI);
  });

  it('converts 360_000 mdeg (360 deg) to 2*PI', () => {
    expect(mdegToRad(360_000)).toBeCloseTo(2 * Math.PI);
  });
});

describe('addAngleMdeg', () => {
  it('adds angles without wrapping', () => {
    expect(addAngleMdeg(10_000, 20_000)).toBe(30_000);
  });

  it('wraps past 360_000', () => {
    expect(addAngleMdeg(350_000, 20_000)).toBe(10_000);
  });

  it('handles negative intermediate values', () => {
    expect(addAngleMdeg(0, -10_000)).toBe(350_000);
  });
});

describe('signedDeltaMdeg', () => {
  it('returns positive for forward arc', () => {
    expect(signedDeltaMdeg(10_000, 20_000)).toBe(10_000);
  });

  it('returns negative for backward arc', () => {
    expect(signedDeltaMdeg(20_000, 10_000)).toBe(-10_000);
  });

  it('takes shortest path across 0', () => {
    expect(signedDeltaMdeg(350_000, 10_000)).toBe(20_000);
  });

  it('takes shortest path backwards across 0', () => {
    expect(signedDeltaMdeg(10_000, 350_000)).toBe(-20_000);
  });

  it('returns 180_000 for exactly opposite', () => {
    expect(signedDeltaMdeg(0, 180_000)).toBe(180_000);
  });
});

describe('withinSpanMdeg', () => {
  it('returns true for angle inside span', () => {
    expect(withinSpanMdeg(50_000, 0, 90_000)).toBe(true);
  });

  it('returns false for angle outside span', () => {
    expect(withinSpanMdeg(100_000, 0, 90_000)).toBe(false);
  });

  it('handles wrap-around span', () => {
    // Span from 350_000 covering 20_000 mdeg wraps around 0
    expect(withinSpanMdeg(5_000, 350_000, 20_000)).toBe(true);
    expect(withinSpanMdeg(355_000, 350_000, 20_000)).toBe(true);
    expect(withinSpanMdeg(330_000, 350_000, 20_000)).toBe(false);
  });

  it('returns true for full circle span', () => {
    expect(withinSpanMdeg(180_000, 0, 360_000)).toBe(true);
  });
});

describe('polarToCartAuUm', () => {
  it('converts 0 degrees to (radius, 0)', () => {
    const { x, y } = polarToCartAuUm(1_000_000, 0);
    expect(x).toBeCloseTo(1_000_000);
    expect(y).toBeCloseTo(0);
  });

  it('converts 90 degrees to (0, radius)', () => {
    const { x, y } = polarToCartAuUm(1_000_000, 90_000);
    expect(x).toBeCloseTo(0);
    expect(y).toBeCloseTo(1_000_000);
  });

  it('converts 180 degrees to (-radius, 0)', () => {
    const { x, y } = polarToCartAuUm(1_000_000, 180_000);
    expect(x).toBeCloseTo(-1_000_000);
    expect(y).toBeCloseTo(0);
  });

  it('converts 270 degrees to (0, -radius)', () => {
    const { x, y } = polarToCartAuUm(1_000_000, 270_000);
    expect(x).toBeCloseTo(0);
    expect(y).toBeCloseTo(-1_000_000);
  });
});

describe('entityAbsolute', () => {
  const bodyAbsolutes = {
    sun: { x_au_um: 0, y_au_um: 0 },
    earth: { x_au_um: 1_000_000, y_au_um: 0 },
  };

  it('computes absolute from parent at origin', () => {
    const pos = entityAbsolute(
      { parent_body: 'sun', radius_au_um: 500_000, angle_mdeg: 0 },
      bodyAbsolutes,
    );
    expect(pos.x_au_um).toBeCloseTo(500_000);
    expect(pos.y_au_um).toBeCloseTo(0);
  });

  it('computes absolute from offset parent', () => {
    const pos = entityAbsolute(
      { parent_body: 'earth', radius_au_um: 10_000, angle_mdeg: 90_000 },
      bodyAbsolutes,
    );
    expect(pos.x_au_um).toBeCloseTo(1_000_000);
    expect(pos.y_au_um).toBeCloseTo(10_000);
  });

  it('returns origin for unknown parent', () => {
    const pos = entityAbsolute(
      { parent_body: 'unknown', radius_au_um: 100, angle_mdeg: 0 },
      bodyAbsolutes,
    );
    expect(pos.x_au_um).toBe(0);
    expect(pos.y_au_um).toBe(0);
  });
});

describe('distanceAuUm', () => {
  it('computes distance between two points', () => {
    const d = distanceAuUm(
      { x_au_um: 0, y_au_um: 0 },
      { x_au_um: 3_000_000, y_au_um: 4_000_000 },
    );
    expect(d).toBeCloseTo(5_000_000);
  });
});

describe('auUmToAu', () => {
  it('converts 1_000_000 µAU to 1 AU', () => {
    expect(auUmToAu(1_000_000)).toBe(1);
  });
});

describe('estimateTravelTicks', () => {
  const config = { ticks_per_au: 100, min_transit_ticks: 5 };

  it('computes ticks from distance', () => {
    expect(estimateTravelTicks(2_000_000, config)).toBe(200);
  });

  it('enforces minimum transit ticks', () => {
    expect(estimateTravelTicks(1000, config)).toBe(5);
  });
});

describe('shipTransitAbsolute', () => {
  const origin: { x_au_um: number; y_au_um: number } = { x_au_um: 0, y_au_um: 0 };
  const dest: { x_au_um: number; y_au_um: number } = { x_au_um: 1_000_000, y_au_um: 0 };

  it('returns origin at progress 0', () => {
    const pos = shipTransitAbsolute(origin, dest, 0);
    expect(pos.x_au_um).toBe(0);
    expect(pos.y_au_um).toBe(0);
  });

  it('returns destination at progress 1', () => {
    const pos = shipTransitAbsolute(origin, dest, 1);
    expect(pos.x_au_um).toBe(1_000_000);
    expect(pos.y_au_um).toBe(0);
  });

  it('returns midpoint at progress 0.5', () => {
    const pos = shipTransitAbsolute(origin, dest, 0.5);
    expect(pos.x_au_um).toBe(500_000);
    expect(pos.y_au_um).toBe(0);
  });

  it('clamps progress below 0', () => {
    const pos = shipTransitAbsolute(origin, dest, -0.5);
    expect(pos.x_au_um).toBe(0);
  });

  it('clamps progress above 1', () => {
    const pos = shipTransitAbsolute(origin, dest, 1.5);
    expect(pos.x_au_um).toBe(1_000_000);
  });
});
