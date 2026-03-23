import { describe, expect, it } from 'vitest';

import { displayName, formatQty, getEventKey, getTaskKind } from './utils';

describe('getEventKey', () => {
  it('extracts the discriminant key from a tagged union event', () => {
    expect(getEventKey({ ShipTaskCompleted: { ship_id: 's1' } })).toBe('ShipTaskCompleted');
  });

  it('returns "Unknown" for an empty object', () => {
    expect(getEventKey({})).toBe('Unknown');
  });
});

describe('getTaskKind', () => {
  it('returns the task kind string', () => {
    const task = { kind: { Transit: { destination: {} } }, started_tick: 0 };
    expect(getTaskKind(task)).toBe('Transit');
  });

  it('returns null when task is null', () => {
    expect(getTaskKind(null)).toBeNull();
  });

  it('returns null when task is undefined', () => {
    expect(getTaskKind(undefined as never)).toBeNull();
  });
});

describe('formatQty', () => {
  it('shows integers without decimals', () => {
    expect(formatQty(0)).toBe('0');
    expect(formatQty(5)).toBe('5');
    expect(formatQty(100)).toBe('100');
  });

  it('shows one decimal for small fractional values', () => {
    expect(formatQty(3.7)).toBe('3.7');
  });

  it('abbreviates thousands', () => {
    expect(formatQty(1200)).toBe('1.2k');
  });
});

describe('displayName', () => {
  it('strips recipe_ prefix and converts to title case', () => {
    expect(displayName('recipe_hull_panel')).toBe('Hull Panel');
  });

  it('converts plain snake_case to title case', () => {
    expect(displayName('structural_beam')).toBe('Structural Beam');
  });

  it('capitalizes single words', () => {
    expect(displayName('ore')).toBe('Ore');
    expect(displayName('Fe')).toBe('Fe');
  });
});
