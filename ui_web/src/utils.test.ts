import { describe, expect, it } from 'vitest';

import { getEventKey, getTaskKind } from './utils';

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
