import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { SimSnapshot } from '../types';

import { SolarSystemMapCanvas } from './SolarSystemMapCanvas';

// Mock fetchSpatialConfig to return test data
vi.mock('../api', () => ({
  fetchSpatialConfig: vi.fn(() => Promise.resolve({
    bodies: [
      {
        id: 'sun', name: 'Sun', parent: null, body_type: 'Star',
        radius_au_um: 0, angle_mdeg: 0, solar_intensity: 1.0, zone: null,
      },
      {
        id: 'earth', name: 'Earth', parent: 'sun', body_type: 'Planet',
        radius_au_um: 1_000_000, angle_mdeg: 0, solar_intensity: 1.0, zone: null,
      },
      {
        id: 'inner_belt', name: 'Inner Belt', parent: 'sun', body_type: 'Belt',
        radius_au_um: 2_450_000, angle_mdeg: 0, solar_intensity: 0.4,
        zone: {
          radius_min_au_um: 2_100_000, radius_max_au_um: 2_800_000,
          angle_start_mdeg: 0, angle_span_mdeg: 360_000,
          resource_class: 'MetalRich', scan_site_weight: 3,
        },
      },
    ],
    body_absolutes: {
      sun: { x_au_um: 0, y_au_um: 0 },
      earth: { x_au_um: 1_000_000, y_au_um: 0 },
      inner_belt: { x_au_um: 2_450_000, y_au_um: 0 },
    },
    ticks_per_au: 100,
    min_transit_ticks: 5,
    docking_range_au_um: 10_000,
  })),
}));

const emptySnapshot: SimSnapshot = {
  meta: {
    tick: 100, seed: 42, content_version: '0.0.1',
    ticks_per_sec: 10, paused: false, minutes_per_tick: 60, trade_unlock_tick: 500,
  },
  scan_sites: [],
  asteroids: {},
  ships: {},
  stations: {},
  research: { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} },
  body_absolutes: {
    sun: { x_au_um: 0, y_au_um: 0 },
    earth: { x_au_um: 1_000_000, y_au_um: 0 },
    inner_belt: { x_au_um: 2_450_000, y_au_um: 0 },
  },
  balance: 0,
};

describe('SolarSystemMapCanvas', () => {
  it('renders a canvas element', () => {
    const { container } = render(
      <SolarSystemMapCanvas snapshot={emptySnapshot} currentTick={100} />,
    );
    expect(container.querySelector('canvas')).toBeInTheDocument();
  });

  it('renders starfield background div', () => {
    const { container } = render(
      <SolarSystemMapCanvas snapshot={emptySnapshot} currentTick={100} />,
    );
    const bgDiv = container.querySelector('[style*="background-repeat"]');
    expect(bgDiv).toBeInTheDocument();
  });

  it('renders with null snapshot', () => {
    const { container } = render(
      <SolarSystemMapCanvas snapshot={null} currentTick={0} />,
    );
    expect(container.querySelector('canvas')).toBeInTheDocument();
  });

  it('renders station entity markers (canvas — no DOM markers)', () => {
    const snapshotWithEntities: SimSnapshot = {
      ...emptySnapshot,
      stations: {
        station_001: {
          id: 'station_001',
          position: { parent_body: 'earth', radius_au_um: 5_000, angle_mdeg: 0 },
          power_available_per_tick: 100,
          inventory: [],
          cargo_capacity_m3: 10000,
          modules: [],
          power: {
            generated_kw: 0, consumed_kw: 0, deficit_kw: 0,
            battery_discharge_kw: 0, battery_charge_kw: 0, battery_stored_kwh: 0,
          },
        },
      },
    };
    // Canvas-based rendering — entities are drawn on canvas, not DOM nodes.
    // Just verify the component renders without errors.
    const { container } = render(
      <SolarSystemMapCanvas snapshot={snapshotWithEntities} currentTick={100} />,
    );
    expect(container.querySelector('canvas')).toBeInTheDocument();
  });
});
