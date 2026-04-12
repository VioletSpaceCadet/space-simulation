import { describe, expect, it } from 'vitest';

import type { ActiveAlert, SimSnapshot, StationState, ShipState, AsteroidState } from '../types';

import { buildSnapshotReadable } from './snapshotSelector';

function makeSnapshot(overrides: Partial<SimSnapshot> = {}): SimSnapshot {
  const base: SimSnapshot = {
    meta: {
      tick: 42,
      seed: 1,
      content_version: 'test',
      ticks_per_sec: 10,
      paused: false,
      minutes_per_tick: 60,
      trade_unlock_tick: 0,
    },
    balance: 1_000_000_000,
    scan_sites: [],
    asteroids: {},
    ships: {},
    stations: {},
    research: { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} },
    body_absolutes: {},
  };
  return { ...base, ...overrides };
}

function makeStation(overrides: Partial<StationState> = {}): StationState {
  return {
    id: 'station_1',
    position: { parent_body: 'earth', radius_au_um: 1, angle_mdeg: 0 },
    power_available_per_tick: 100,
    inventory: [],
    cargo_capacity_m3: 1000,
    modules: [],
    power: {
      generated_kw: 100,
      consumed_kw: 60,
      deficit_kw: 0,
      battery_discharge_kw: 0,
      battery_charge_kw: 0,
      battery_stored_kwh: 0,
    },
    ...overrides,
  };
}

function makeShip(task: ShipState['task']): ShipState {
  return {
    id: 'ship_1',
    position: { parent_body: 'earth', radius_au_um: 1, angle_mdeg: 0 },
    owner: 'player',
    inventory: [],
    cargo_capacity_m3: 100,
    task,
  };
}

function makeAlert(overrides: Partial<ActiveAlert> = {}): ActiveAlert {
  return {
    alert_id: 'alert_1',
    severity: 'Warning',
    message: 'Test alert',
    suggested_action: 'Do something',
    tick: 10,
    ...overrides,
  };
}

describe('buildSnapshotReadable', () => {
  it('returns an offline placeholder when the snapshot is null and disconnected', () => {
    const readable = buildSnapshotReadable({
      snapshot: null,
      activeAlerts: new Map(),
      currentTick: 0,
      paused: false,
      connected: false,
    });

    expect(readable.snapshot_tick).toBe(0);
    expect(readable.snapshot_age_label).toContain('disconnected');
    expect(readable.treasury_usd).toBe(0);
    expect(readable.stations.total).toBe(0);
    expect(readable.fleet.total).toBe(0);
  });

  it('returns a loading placeholder when connected but snapshot is null', () => {
    const readable = buildSnapshotReadable({
      snapshot: null,
      activeAlerts: new Map(),
      currentTick: 0,
      paused: false,
      connected: true,
    });

    expect(readable.snapshot_age_label).toContain('loading');
  });

  it('surfaces pause state in the age label', () => {
    const readable = buildSnapshotReadable({
      snapshot: makeSnapshot(),
      activeAlerts: new Map(),
      currentTick: 42,
      paused: true,
      connected: true,
    });

    expect(readable.snapshot_age_label).toBe('current (paused)');
    expect(readable.paused).toBe(true);
  });

  it('recommends pausing in the age label when unpaused', () => {
    const readable = buildSnapshotReadable({
      snapshot: makeSnapshot(),
      activeAlerts: new Map(),
      currentTick: 99,
      paused: false,
      connected: true,
    });

    expect(readable.snapshot_age_label).toContain('99');
    expect(readable.snapshot_age_label).toContain('pausing');
  });

  it('counts alerts by severity and surfaces the most recent critical ones', () => {
    const activeAlerts = new Map<string, ActiveAlert>([
      ['a1', makeAlert({ alert_id: 'a1', severity: 'Warning', tick: 5 })],
      ['a2', makeAlert({ alert_id: 'a2', severity: 'Critical', tick: 10 })],
      ['a3', makeAlert({ alert_id: 'a3', severity: 'Critical', tick: 20 })],
      ['a4', makeAlert({ alert_id: 'a4', severity: 'Critical', tick: 15 })],
      ['a5', makeAlert({ alert_id: 'a5', severity: 'Critical', tick: 30 })],
    ]);

    const readable = buildSnapshotReadable({
      snapshot: makeSnapshot(),
      activeAlerts,
      currentTick: 30,
      paused: true,
      connected: true,
    });

    expect(readable.active_alerts.total).toBe(5);
    expect(readable.active_alerts.warnings).toBe(1);
    expect(readable.active_alerts.critical).toBe(4);
    // Top 3 critical alerts by tick descending
    expect(readable.active_alerts.recent_critical.map((a) => a.alert_id)).toEqual(['a5', 'a3', 'a4']);
  });

  it('summarizes stations with module count, crew, net power, and wear', () => {
    const station: StationState = makeStation({
      id: 'hub_alpha',
      crew: { pilot: 3, engineer: 5 },
      modules: [
        {
          id: 'm1',
          def_id: 'processor',
          enabled: true,
          kind_state: { Processor: { threshold_kg: 10, ticks_since_last_run: 0, stalled: false } },
          wear: { wear: 0.2 },
        },
        {
          id: 'm2',
          def_id: 'lab',
          enabled: true,
          kind_state: { Lab: { ticks_since_last_run: 0, assigned_tech: null, starved: false } },
          wear: { wear: 0.4 },
        },
      ],
      power: {
        generated_kw: 200.0,
        consumed_kw: 150.0,
        deficit_kw: 0,
        battery_discharge_kw: 0,
        battery_charge_kw: 0,
        battery_stored_kwh: 0,
      },
    });

    const readable = buildSnapshotReadable({
      snapshot: makeSnapshot({ stations: { hub_alpha: station } }),
      activeAlerts: new Map(),
      currentTick: 100,
      paused: true,
      connected: true,
    });

    expect(readable.stations.total).toBe(1);
    expect(readable.stations.summary).toHaveLength(1);
    const stationSummary = readable.stations.summary[0];
    expect(stationSummary).toBeDefined();
    expect(stationSummary?.id).toBe('hub_alpha');
    expect(stationSummary?.module_count).toBe(2);
    expect(stationSummary?.crew_total).toBe(8);
    expect(stationSummary?.power_net_kw).toBe(50);
    expect(stationSummary?.avg_wear).toBeCloseTo(0.3, 2);
  });

  it('counts ships by task kind', () => {
    const ships: Record<string, ShipState> = {
      s1: makeShip({
        kind: { Idle: {} },
        started_tick: 0,
        eta_tick: 0,
      }),
      s2: makeShip({
        kind: {
          Transit: {
            destination: { parent_body: 'mars', radius_au_um: 1, angle_mdeg: 0 },
            total_ticks: 100,
            then: {},
          },
        },
        started_tick: 0,
        eta_tick: 100,
      }),
      s3: makeShip({
        kind: { Mine: { asteroid: 'rock_1', duration_ticks: 50 } },
        started_tick: 0,
        eta_tick: 50,
      }),
      s4: makeShip(null),
    };

    const readable = buildSnapshotReadable({
      snapshot: makeSnapshot({ ships }),
      activeAlerts: new Map(),
      currentTick: 100,
      paused: true,
      connected: true,
    });

    expect(readable.fleet.total).toBe(4);
    expect(readable.fleet.in_transit).toBe(1);
    expect(readable.fleet.mining).toBe(1);
    expect(readable.fleet.idle).toBe(2);
    expect(readable.fleet.other).toBe(0);
  });

  it('counts Deposit, Survey, and DeepScan tasks in the other bucket', () => {
    const ships: Record<string, ShipState> = {
      s1: makeShip({
        kind: { Deposit: { station: 'station_1' } },
        started_tick: 0,
        eta_tick: 10,
      }),
      s2: makeShip({
        kind: { Survey: { site_id: 'site_1', duration_ticks: 20 } },
        started_tick: 0,
        eta_tick: 20,
      }),
      // eslint-disable-next-line @typescript-eslint/no-explicit-any -- intentional runtime-shape test
      s3: makeShip({ kind: 'DeepScan' as any, started_tick: 0, eta_tick: 30 }),
      s4: makeShip({
        kind: { Idle: {} },
        started_tick: 0,
        eta_tick: 0,
      }),
    };

    const readable = buildSnapshotReadable({
      snapshot: makeSnapshot({ ships }),
      activeAlerts: new Map(),
      currentTick: 100,
      paused: true,
      connected: true,
    });

    expect(readable.fleet.total).toBe(4);
    expect(readable.fleet.idle).toBe(1);
    expect(readable.fleet.other).toBe(3);
    expect(readable.fleet.in_transit).toBe(0);
    expect(readable.fleet.mining).toBe(0);
  });

  it('treats the bare string "Idle" variant as idle (serde unit-variant wire format)', () => {
    // `Task::Idle` is a Rust unit variant — serde's external tagging
    // emits it as the bare JSON string `"Idle"`, not as `{ Idle: {} }`.
    // The TypeScript type in `types.ts` models it as an object wrapper
    // for ergonomic switch statements, but the runtime wire format is a
    // string. The selector must handle both shapes or `in` throws on the
    // primitive.
    const ships: Record<string, ShipState> = {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any -- intentional runtime-shape test
      s1: makeShip({ kind: 'Idle' as any, started_tick: 0, eta_tick: 0 }),
      s2: makeShip({
        kind: { Mine: { asteroid: 'rock_1', duration_ticks: 50 } },
        started_tick: 0,
        eta_tick: 50,
      }),
    };

    const readable = buildSnapshotReadable({
      snapshot: makeSnapshot({ ships }),
      activeAlerts: new Map(),
      currentTick: 100,
      paused: true,
      connected: true,
    });

    expect(readable.fleet.total).toBe(2);
    expect(readable.fleet.idle).toBe(1);
    expect(readable.fleet.mining).toBe(1);
    expect(readable.fleet.other).toBe(0);
  });

  it('summarizes research with unlocked count, recent unlocks, and active domains', () => {
    const readable = buildSnapshotReadable({
      snapshot: makeSnapshot({
        research: {
          unlocked: ['tech_a', 'tech_b', 'tech_c', 'tech_d', 'tech_e', 'tech_f'],
          data_pool: { metallurgy: 10, thermal: 5 },
          evidence: {
            metallurgy: { points: { low_grade: 3, high_grade: 5 } },
            thermal: { points: {} },
            propulsion: { points: { basic: 1 } },
          },
          action_counts: {},
        },
      }),
      activeAlerts: new Map(),
      currentTick: 1,
      paused: true,
      connected: true,
    });

    expect(readable.research.unlocked_count).toBe(6);
    expect(readable.research.recent_unlocks).toEqual(['tech_b', 'tech_c', 'tech_d', 'tech_e', 'tech_f']);
    expect(readable.research.data_pool_kinds).toBe(2);
    expect(readable.research.active_domains).toBe(2); // metallurgy + propulsion have points; thermal does not
  });

  it('summarizes asteroids including tagged count', () => {
    const asteroids: Record<string, AsteroidState> = {
      a1: {
        id: 'a1',
        position: { parent_body: 'belt', radius_au_um: 3, angle_mdeg: 0 },
        anomaly_tags: ['MetalRich'],
        knowledge: { tag_beliefs: [], composition: null },
      },
      a2: {
        id: 'a2',
        position: { parent_body: 'belt', radius_au_um: 3, angle_mdeg: 90 },
        anomaly_tags: [],
        knowledge: { tag_beliefs: [], composition: null },
      },
    };

    const readable = buildSnapshotReadable({
      snapshot: makeSnapshot({ asteroids }),
      activeAlerts: new Map(),
      currentTick: 1,
      paused: true,
      connected: true,
    });

    expect(readable.asteroids.discovered).toBe(2);
    expect(readable.asteroids.tagged).toBe(1);
  });

  it('rounds treasury to an integer to keep the payload compact', () => {
    const readable = buildSnapshotReadable({
      snapshot: makeSnapshot({ balance: 123_456_789.654321 }),
      activeAlerts: new Map(),
      currentTick: 1,
      paused: true,
      connected: true,
    });

    expect(readable.treasury_usd).toBe(123_456_790);
  });

  it('fits under the 4 KB JSON budget with a realistic late-game fixture', () => {
    // 3 stations with 15 modules each (= 45 total), 15 ships mixed tasks,
    // 8 alerts (3 critical), 20 unlocked techs, 50 asteroids with half tagged.
    const stations: Record<string, StationState> = {};
    for (let i = 0; i < 3; i++) {
      const modules = Array.from({ length: 15 }, (_, j) => ({
        id: `s${String(i)}_m${String(j)}`,
        def_id: 'processor',
        enabled: true,
        kind_state: {
          Processor: { threshold_kg: 10, ticks_since_last_run: 0, stalled: false },
        },
        wear: { wear: (j * 0.05) % 1 },
      }));
      stations[`station_${String(i)}`] = makeStation({
        id: `station_${String(i)}`,
        modules,
        crew: { pilot: 3, engineer: 4, scientist: 2 },
        power: {
          generated_kw: 500 + i * 50,
          consumed_kw: 400 + i * 40,
          deficit_kw: 0,
          battery_discharge_kw: 0,
          battery_charge_kw: 0,
          battery_stored_kwh: 0,
        },
      });
    }

    const ships: Record<string, ShipState> = {};
    for (let i = 0; i < 15; i++) {
      const pick = i % 3;
      let task: ShipState['task'];
      if (pick === 0) { task = { kind: { Idle: {} }, started_tick: 0, eta_tick: 0 }; }
      else if (pick === 1) {
        task = {
          kind: {
            Transit: {
              destination: { parent_body: 'mars', radius_au_um: 1, angle_mdeg: 0 },
              total_ticks: 100,
              then: {},
            },
          },
          started_tick: 0,
          eta_tick: 100,
        };
      } else {
        task = {
          kind: { Mine: { asteroid: `asteroid_${String(i)}`, duration_ticks: 50 } },
          started_tick: 0,
          eta_tick: 50,
        };
      }
      ships[`ship_${String(i)}`] = { ...makeShip(task), id: `ship_${String(i)}` };
    }

    const asteroids: Record<string, AsteroidState> = {};
    for (let i = 0; i < 50; i++) {
      asteroids[`asteroid_${String(i)}`] = {
        id: `asteroid_${String(i)}`,
        position: { parent_body: 'belt', radius_au_um: 3, angle_mdeg: i },
        anomaly_tags: i % 2 === 0 ? ['MetalRich'] : [],
        knowledge: { tag_beliefs: [], composition: null },
      };
    }

    const alerts = new Map<string, ActiveAlert>();
    for (let i = 0; i < 8; i++) {
      const alertId = `alert_${String(i)}`;
      alerts.set(alertId, {
        alert_id: alertId,
        severity: i < 3 ? 'Critical' : 'Warning',
        message: `Pretend this is a realistically long alert message describing failure ${String(i)}`,
        suggested_action: `Investigate ${alertId} and consider corrective action`,
        tick: 100 + i,
      });
    }

    const snapshot = makeSnapshot({
      balance: 987_654_321,
      stations,
      ships,
      asteroids,
      research: {
        unlocked: Array.from({ length: 20 }, (_, i) => `tech_${String(i)}`),
        data_pool: { metallurgy: 50, thermal: 30, propulsion: 10 },
        evidence: {
          metallurgy: { points: { low_grade: 10 } },
          thermal: { points: { heat_tolerance: 5 } },
          propulsion: { points: {} },
        },
        action_counts: {},
      },
    });

    const readable = buildSnapshotReadable({
      snapshot,
      activeAlerts: alerts,
      currentTick: 50_000,
      paused: true,
      connected: true,
    });

    const json = JSON.stringify(readable);
    const sizeBytes = new TextEncoder().encode(json).length;

    // Document actual size in the test so a future regression is visible.
    // Current measured payload ≈ 900 bytes; assert well under the 4096 budget.
    expect(sizeBytes).toBeLessThan(4096);
  });
});
