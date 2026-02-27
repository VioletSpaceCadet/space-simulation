import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { SimSnapshot } from '../types';

import { SolarSystemMap } from './SolarSystemMap';

const emptySnapshot: SimSnapshot = {
  meta: { tick: 100, seed: 42, content_version: '0.0.1', ticks_per_sec: 10, paused: false },
  scan_sites: [],
  asteroids: {},
  ships: {},
  stations: {},
  research: { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} },
  balance: 0,
};

describe('SolarSystemMap', () => {
  it('renders an SVG element', () => {
    const { container } = render(
      <SolarSystemMap snapshot={emptySnapshot} currentTick={100} />,
    );
    expect(container.querySelector('svg')).toBeInTheDocument();
  });

  it('renders station markers', () => {
    const snapshotWithEntities: SimSnapshot = {
      ...emptySnapshot,
      stations: {
        station_001: {
          id: 'station_001',
          location_node: 'node_earth_orbit',
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
      <SolarSystemMap snapshot={snapshotWithEntities} currentTick={100} />
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
          location_node: 'node_earth_orbit',
          owner: 'player',
          inventory: [],
          cargo_capacity_m3: 20,
          task: null,
        },
      },
    };
    const { container } = render(
      <SolarSystemMap snapshot={snapshotWithShip} currentTick={100} />
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
          location_node: 'node_belt_inner',
          anomaly_tags: ['IronRich'],
          mass_kg: 5000,
          knowledge: { tag_beliefs: [['IronRich', 0.85]], composition: null },
        },
      },
    };
    const { container } = render(
      <SolarSystemMap snapshot={snapshotWithAsteroids} currentTick={100} />
    );
    const markers = container.querySelectorAll('[data-entity-type="asteroid"]');
    expect(markers.length).toBe(1);
  });

  it('renders scan site markers', () => {
    const snapshotWithSites: SimSnapshot = {
      ...emptySnapshot,
      scan_sites: [
        { id: 'site_001', node: 'node_belt_mid', template_id: 'tmpl_iron' },
      ],
    };
    const { container } = render(
      <SolarSystemMap snapshot={snapshotWithSites} currentTick={100} />
    );
    const markers = container.querySelectorAll('[data-entity-type="scan-site"]');
    expect(markers.length).toBe(1);
  });

  it('renders orbital ring labels', () => {
    render(
      <SolarSystemMap snapshot={emptySnapshot} currentTick={100} />,
    );
    expect(screen.getByText('Earth Orbit')).toBeInTheDocument();
    expect(screen.getByText('Inner Belt')).toBeInTheDocument();
    expect(screen.getByText('Mid Belt')).toBeInTheDocument();
    expect(screen.getByText('Outer Belt')).toBeInTheDocument();
  });
});
