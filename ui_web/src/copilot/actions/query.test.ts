import { describe, expect, it } from 'vitest';

import type { ActiveAlert, SimSnapshot } from '../../types';

import { diagnoseAlertById, extractSection } from './queryHandlers';

function makeSnapshot(overrides: Partial<SimSnapshot> = {}): SimSnapshot {
  return {
    meta: {
      tick: 10,
      seed: 1,
      content_version: 'test',
      ticks_per_sec: 10,
      paused: true,
      minutes_per_tick: 60,
      trade_unlock_tick: 0,
    },
    balance: 500_000,
    scan_sites: [],
    asteroids: {},
    ships: {},
    stations: {},
    research: { unlocked: ['tech_a'], data_pool: {}, evidence: {}, action_counts: {} },
    body_absolutes: {},
    ...overrides,
  };
}

const defaultArgs = {
  snapshot: makeSnapshot(),
  activeAlerts: new Map<string, ActiveAlert>(),
  currentTick: 10,
  paused: true,
  connected: true,
};

describe('extractSection', () => {
  it('returns the full readable when section is "summary"', () => {
    const result = extractSection('summary', defaultArgs) as { snapshot_tick: number; treasury_usd: number };
    expect(result.snapshot_tick).toBe(10);
    expect(result.treasury_usd).toBe(500_000);
  });

  it('returns a treasury-only object when section is "treasury"', () => {
    const result = extractSection('treasury', defaultArgs);
    expect(result).toEqual({ treasury_usd: 500_000 });
  });

  it('returns the alerts subsection shape', () => {
    const activeAlerts = new Map<string, ActiveAlert>([
      ['a1', {
        alert_id: 'a1',
        severity: 'Critical',
        message: 'Reactor leak',
        suggested_action: 'Shut down',
        tick: 5,
      }],
    ]);
    const result = extractSection('alerts', { ...defaultArgs, activeAlerts }) as {
      total: number;
      critical: number;
    };
    expect(result.total).toBe(1);
    expect(result.critical).toBe(1);
  });

  it('returns the research subsection shape', () => {
    const result = extractSection('research', defaultArgs) as { unlocked_count: number };
    expect(result.unlocked_count).toBe(1);
  });

  it('returns the stations subsection shape', () => {
    const result = extractSection('stations', defaultArgs) as { total: number };
    expect(result.total).toBe(0);
  });

  it('returns the fleet subsection shape', () => {
    const result = extractSection('fleet', defaultArgs) as { total: number };
    expect(result.total).toBe(0);
  });

  it('returns the asteroids subsection shape', () => {
    const result = extractSection('asteroids', defaultArgs) as { discovered: number };
    expect(result.discovered).toBe(0);
  });
});

describe('diagnoseAlertById', () => {
  const alerts = new Map<string, ActiveAlert>([
    ['alert_storm', {
      alert_id: 'alert_storm',
      severity: 'Critical',
      message: 'Solar storm inbound',
      suggested_action: 'Shield ships',
      tick: 42,
    }],
  ]);

  it('returns ok + context for an existing alert', () => {
    const result = diagnoseAlertById('alert_storm', alerts);
    expect(result.status).toBe('ok');
    if (result.status === 'ok') {
      expect(result.alert.alert_id).toBe('alert_storm');
      expect(result.context).toContain('Critical');
      expect(result.context).toContain('Solar storm');
      expect(result.context).toContain('tick 42');
      expect(result.context).toContain('Shield ships');
    }
  });

  it('returns not_found for an unknown alert instead of throwing', () => {
    const result = diagnoseAlertById('missing', alerts);
    expect(result.status).toBe('not_found');
    if (result.status === 'not_found') {
      expect(result.alert_id).toBe('missing');
    }
  });
});
