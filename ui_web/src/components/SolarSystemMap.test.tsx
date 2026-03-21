import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { SimSnapshot } from '../types';

import { SolarSystemMap } from './SolarSystemMap';

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

describe('SolarSystemMap', () => {
  it('renders an SVG element', () => {
    const { container } = render(
      <SolarSystemMap snapshot={emptySnapshot} currentTick={100} />,
    );
    expect(container.querySelector('svg')).toBeInTheDocument();
  });

  it('renders body labels after config loads', async () => {
    render(<SolarSystemMap snapshot={emptySnapshot} currentTick={100} />);
    await waitFor(() => {
      expect(screen.getByText('Sun')).toBeInTheDocument();
      expect(screen.getByText('Earth')).toBeInTheDocument();
    });
  });

  it('renders station markers', () => {
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
    const { container } = render(
      <SolarSystemMap snapshot={snapshotWithEntities} currentTick={100} />,
    );
    const stationMarkers = container.querySelectorAll('[data-entity-type="station"]');
    expect(stationMarkers.length).toBe(1);
  });

  it('renders ship markers', () => {
    const snapshotWithShip: SimSnapshot = {
      ...emptySnapshot,
      ships: {
        ship_001: {
          id: 'ship_001',
          position: { parent_body: 'earth', radius_au_um: 5_000, angle_mdeg: 90_000 },
          owner: 'player',
          inventory: [],
          cargo_capacity_m3: 20,
          task: null,
        },
      },
    };
    const { container } = render(
      <SolarSystemMap snapshot={snapshotWithShip} currentTick={100} />,
    );
    const shipMarkers = container.querySelectorAll('[data-entity-type="ship"]');
    expect(shipMarkers.length).toBe(1);
  });

  it('renders asteroid markers', () => {
    const snapshotWithAsteroids: SimSnapshot = {
      ...emptySnapshot,
      asteroids: {
        asteroid_0001: {
          id: 'asteroid_0001',
          position: { parent_body: 'inner_belt', radius_au_um: 2_400_000, angle_mdeg: 45_000 },
          anomaly_tags: ['IronRich'],
          mass_kg: 5000,
          knowledge: { tag_beliefs: [['IronRich', 0.85]], composition: null },
        },
      },
    };
    const { container } = render(
      <SolarSystemMap snapshot={snapshotWithAsteroids} currentTick={100} />,
    );
    const markers = container.querySelectorAll('[data-entity-type="asteroid"]');
    expect(markers.length).toBe(1);
  });

  it('renders scan site markers', () => {
    const snapshotWithSites: SimSnapshot = {
      ...emptySnapshot,
      scan_sites: [
        {
          id: 'site_001',
          position: { parent_body: 'inner_belt', radius_au_um: 2_500_000, angle_mdeg: 180_000 },
          template_id: 'tmpl_iron',
        },
      ],
    };
    const { container } = render(
      <SolarSystemMap snapshot={snapshotWithSites} currentTick={100} />,
    );
    const markers = container.querySelectorAll('[data-entity-type="scan-site"]');
    expect(markers.length).toBe(1);
  });

  it('renders zone arcs after config loads', async () => {
    const { container } = render(
      <SolarSystemMap snapshot={emptySnapshot} currentTick={100} />,
    );
    await waitFor(() => {
      const paths = container.querySelectorAll('path[fill-rule="evenodd"]');
      expect(paths.length).toBeGreaterThanOrEqual(1);
    });
  });
});
