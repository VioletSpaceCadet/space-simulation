import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { SolarSystemMap } from './SolarSystemMap'
import type { SimSnapshot } from '../types'

const emptySnapshot: SimSnapshot = {
  meta: { tick: 100, seed: 42, content_version: '0.0.1' },
  scan_sites: [],
  asteroids: {},
  ships: {},
  stations: {},
  research: { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} },
}

describe('SolarSystemMap', () => {
  it('renders an SVG element', () => {
    const { container } = render(
      <SolarSystemMap snapshot={emptySnapshot} currentTick={100} />,
    )
    expect(container.querySelector('svg')).toBeInTheDocument()
  })

  it('renders station markers', () => {
    const snapshotWithEntities: SimSnapshot = {
      ...emptySnapshot,
      stations: {
        station_001: {
          id: 'station_001',
          location_node: 'node_earth_orbit',
          power_available_per_tick: 100,
          cargo: {},
          cargo_capacity_m3: 10000,
          facilities: { compute_units_total: 10, power_per_compute_unit_per_tick: 1, efficiency: 1 },
        },
      },
    }
    const { container } = render(
      <SolarSystemMap snapshot={snapshotWithEntities} currentTick={100} />
    )
    const stationMarkers = container.querySelectorAll('[data-entity-type="station"]')
    expect(stationMarkers.length).toBe(1)
  })

  it('renders ship markers', () => {
    const snapshotWithShip: SimSnapshot = {
      ...emptySnapshot,
      ships: {
        ship_001: {
          id: 'ship_001',
          location_node: 'node_earth_orbit',
          owner: 'player',
          cargo: {},
          cargo_capacity_m3: 20,
          task: null,
        },
      },
    }
    const { container } = render(
      <SolarSystemMap snapshot={snapshotWithShip} currentTick={100} />
    )
    const shipMarkers = container.querySelectorAll('[data-entity-type="ship"]')
    expect(shipMarkers.length).toBe(1)
  })

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
    }
    const { container } = render(
      <SolarSystemMap snapshot={snapshotWithAsteroids} currentTick={100} />
    )
    const markers = container.querySelectorAll('[data-entity-type="asteroid"]')
    expect(markers.length).toBe(1)
  })

  it('renders scan site markers', () => {
    const snapshotWithSites: SimSnapshot = {
      ...emptySnapshot,
      scan_sites: [
        { id: 'site_001', node: 'node_belt_mid', template_id: 'tmpl_iron' },
      ],
    }
    const { container } = render(
      <SolarSystemMap snapshot={snapshotWithSites} currentTick={100} />
    )
    const markers = container.querySelectorAll('[data-entity-type="scan-site"]')
    expect(markers.length).toBe(1)
  })

  it('renders orbital ring labels', () => {
    render(
      <SolarSystemMap snapshot={emptySnapshot} currentTick={100} />,
    )
    expect(screen.getByText('Earth Orbit')).toBeInTheDocument()
    expect(screen.getByText('Inner Belt')).toBeInTheDocument()
    expect(screen.getByText('Mid Belt')).toBeInTheDocument()
    expect(screen.getByText('Outer Belt')).toBeInTheDocument()
  })
})
