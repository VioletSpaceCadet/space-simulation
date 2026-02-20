import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { FleetPanel } from './FleetPanel'
import type { ShipState, StationState } from '../types'

const mockShips: Record<string, ShipState> = {
  ship_0001: {
    id: 'ship_0001',
    location_node: 'node_earth_orbit',
    owner: 'principal_autopilot',
    cargo: { Fe: 150.0, Si: 30.0 },
    cargo_capacity_m3: 20.0,
    task: null,
  },
  ship_0002: {
    id: 'ship_0002',
    location_node: 'node_belt_inner',
    owner: 'principal_autopilot',
    cargo: {},
    cargo_capacity_m3: 20.0,
    task: null,
  },
}

const mockStations: Record<string, StationState> = {
  station_earth_orbit: {
    id: 'station_earth_orbit',
    location_node: 'node_earth_orbit',
    power_available_per_tick: 100,
    cargo: { Fe: 500.0 },
    cargo_capacity_m3: 100.0,
    facilities: { compute_units_total: 10, power_per_compute_unit_per_tick: 1, efficiency: 1.0 },
  },
}

describe('FleetPanel', () => {
  it('renders ship id in table', () => {
    render(<FleetPanel ships={mockShips} stations={{}} oreCompositions={{}} />)
    expect(screen.getByText('ship_0001')).toBeInTheDocument()
  })

  it('renders cargo amount for ship', () => {
    render(<FleetPanel ships={mockShips} stations={{}} oreCompositions={{}} />)
    expect(screen.getByText(/180/)).toBeInTheDocument() // 150 + 30
  })

  it('renders empty state when no ships', () => {
    render(<FleetPanel ships={{}} stations={{}} oreCompositions={{}} />)
    expect(screen.getByText(/no ships/i)).toBeInTheDocument()
  })

  it('renders station id in table', () => {
    render(<FleetPanel ships={{}} stations={mockStations} oreCompositions={{}} />)
    expect(screen.getByText('station_earth_orbit')).toBeInTheDocument()
  })

  it('renders sort indicators on ship table headers', () => {
    render(<FleetPanel ships={mockShips} stations={{}} oreCompositions={{}} />)
    const headers = screen.getAllByRole('columnheader')
    expect(headers.some((h) => h.textContent?.includes('⇅'))).toBe(true)
  })

  it('sorts ships by cargo ascending on click', () => {
    render(<FleetPanel ships={mockShips} stations={{}} oreCompositions={{}} />)
    const cargoHeader = screen.getByText(/^Cargo/)
    fireEvent.click(cargoHeader)
    const rows = document.querySelectorAll('tbody tr')
    // ship_0002 (empty) should come before ship_0001 (180 kg)
    expect(rows[0].textContent).toMatch(/ship_0002/)
    expect(rows[1].textContent).toMatch(/ship_0001/)
  })
})
