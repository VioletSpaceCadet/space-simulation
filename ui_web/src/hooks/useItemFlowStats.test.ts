import { renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { SimSnapshot, StationState } from '../types';

import { useItemFlowStats } from './useItemFlowStats';

function makeStation(overrides: Partial<StationState> = {}): StationState {
  return {
    id: 'station_001',
    position: { parent_body: 'body_a', radius_au_um: 0, angle_mdeg: 0 },
    power_available_per_tick: 10,
    inventory: [],
    cargo_capacity_m3: 100,
    modules: [],
    power: {
      generated_kw: 0, consumed_kw: 0, deficit_kw: 0,
      battery_discharge_kw: 0, battery_charge_kw: 0, battery_stored_kwh: 0,
    },
    ...overrides,
  };
}

function makeSnapshot(overrides: Partial<SimSnapshot> = {}): SimSnapshot {
  return {
    meta: { tick: 10, seed: 42, content_version: '0.0.1', ticks_per_sec: 10, paused: false, minutes_per_tick: 60 },
    balance: 1_000_000_000,
    scan_sites: [],
    asteroids: {},
    ships: {},
    stations: {},
    research: { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} },
    body_absolutes: {},
    ...overrides,
  };
}

describe('useItemFlowStats', () => {
  it('returns empty map for null snapshot', () => {
    const { result } = renderHook(() => useItemFlowStats(null, 60));
    expect(result.current.size).toBe(0);
  });

  it('returns stable trend on first snapshot (no previous data)', () => {
    const snapshot = makeSnapshot({
      stations: {
        station_001: makeStation({
          inventory: [
            { kind: 'Material', element: 'Fe', kg: 500, quality: 1.0 },
          ],
        }),
      },
    });
    const { result } = renderHook(() => useItemFlowStats(snapshot, 60));

    const feStat = result.current.get('Fe');
    expect(feStat).toBeDefined();
    expect(feStat!.current_qty).toBe(500);
    expect(feStat!.trend).toBe('stable');
  });

  it('detects rising trend when quantity increases', () => {
    const snapshot1 = makeSnapshot({
      stations: {
        station_001: makeStation({
          inventory: [{ kind: 'Material', element: 'Fe', kg: 100, quality: 1.0 }],
        }),
      },
    });
    const snapshot2 = makeSnapshot({
      stations: {
        station_001: makeStation({
          inventory: [{ kind: 'Material', element: 'Fe', kg: 200, quality: 1.0 }],
        }),
      },
    });

    const { result, rerender } = renderHook(
      ({ snap }) => useItemFlowStats(snap, 60),
      { initialProps: { snap: snapshot1 } },
    );

    // First render: stable (no previous)
    expect(result.current.get('Fe')!.trend).toBe('stable');

    // Rerender with increased quantity
    rerender({ snap: snapshot2 });
    expect(result.current.get('Fe')!.trend).toBe('rising');
    expect(result.current.get('Fe')!.current_qty).toBe(200);
  });

  it('detects falling trend when quantity decreases', () => {
    const snapshot1 = makeSnapshot({
      stations: {
        station_001: makeStation({
          inventory: [{ kind: 'Material', element: 'Fe', kg: 500, quality: 1.0 }],
        }),
      },
    });
    const snapshot2 = makeSnapshot({
      stations: {
        station_001: makeStation({
          inventory: [{ kind: 'Material', element: 'Fe', kg: 100, quality: 1.0 }],
        }),
      },
    });

    const { result, rerender } = renderHook(
      ({ snap }) => useItemFlowStats(snap, 60),
      { initialProps: { snap: snapshot1 } },
    );

    rerender({ snap: snapshot2 });
    expect(result.current.get('Fe')!.trend).toBe('falling');
  });

  it('increments ticks_at_zero when quantity is zero', () => {
    const snapshot1 = makeSnapshot({
      stations: {
        station_001: makeStation({
          inventory: [{ kind: 'Material', element: 'Fe', kg: 0, quality: 1.0 }],
        }),
      },
    });
    const snapshot2 = makeSnapshot({
      meta: { tick: 11, seed: 42, content_version: '0.0.1', ticks_per_sec: 10, paused: false, minutes_per_tick: 60 },
      stations: {
        station_001: makeStation({
          inventory: [{ kind: 'Material', element: 'Fe', kg: 0, quality: 1.0 }],
        }),
      },
    });

    const { result, rerender } = renderHook(
      ({ snap }) => useItemFlowStats(snap, 60),
      { initialProps: { snap: snapshot1 } },
    );

    expect(result.current.get('Fe')!.ticks_at_zero).toBe(1);

    rerender({ snap: snapshot2 });
    expect(result.current.get('Fe')!.ticks_at_zero).toBe(2);
  });
});
