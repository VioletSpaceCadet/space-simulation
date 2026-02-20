import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { FleetPanel } from './FleetPanel'
import type { ShipState, StationState } from '../types'

const mockShips: Record<string, ShipState> = {
  ship_0001: {
    id: 'ship_0001',
    location_node: 'node_earth_orbit',
    owner: 'principal_autopilot',
    inventory: [
      { kind: 'Ore', lot_id: 'lot_0001', asteroid_id: 'asteroid_0001', kg: 150.0, composition: { Fe: 0.7, Si: 0.3 } },
      { kind: 'Material', element: 'Fe', kg: 30.0, quality: 0.7 },
    ],
    cargo_capacity_m3: 20.0,
    task: null,
  },
  ship_0002: {
    id: 'ship_0002',
    location_node: 'node_belt_inner',
    owner: 'principal_autopilot',
    inventory: [],
    cargo_capacity_m3: 20.0,
    task: null,
  },
}

const mockStations: Record<string, StationState> = {
  station_earth_orbit: {
    id: 'station_earth_orbit',
    location_node: 'node_earth_orbit',
    power_available_per_tick: 100,
    inventory: [{ kind: 'Material', element: 'Fe', kg: 500.0, quality: 0.85 }],
    cargo_capacity_m3: 100.0,
    facilities: { compute_units_total: 10, power_per_compute_unit_per_tick: 1, efficiency: 1.0 },
    modules: [],
  },
}

describe('FleetPanel', () => {
  it('renders ship id in table', () => {
    render(<FleetPanel ships={mockShips} stations={{}} displayTick={0} />)
    expect(screen.getByText('ship_0001')).toBeInTheDocument()
  })

  it('renders cargo amount for ship', () => {
    render(<FleetPanel ships={mockShips} stations={{}} displayTick={0} />)
    // ship_0001 has 150 + 30 = 180 kg total
    expect(screen.getByText(/180/)).toBeInTheDocument()
  })

  it('renders empty state when no ships', () => {
    render(<FleetPanel ships={{}} stations={{}} displayTick={0} />)
    expect(screen.getByText(/no ships/i)).toBeInTheDocument()
  })

  it('renders station id in table', () => {
    render(<FleetPanel ships={{}} stations={mockStations} displayTick={0} />)
    expect(screen.getByText('station_earth_orbit')).toBeInTheDocument()
  })

  it('renders sort indicators on ship table headers', () => {
    render(<FleetPanel ships={mockShips} stations={{}} displayTick={0} />)
    const headers = screen.getAllByRole('columnheader')
    expect(headers.some((h) => h.textContent?.includes('⇅'))).toBe(true)
  })

  it('sorts ships by cargo ascending on click', () => {
    render(<FleetPanel ships={mockShips} stations={{}} displayTick={0} />)
    const cargoHeader = screen.getByText(/^Cargo/)
    fireEvent.click(cargoHeader)
    const rows = document.querySelectorAll('tbody tr')
    // ship_0002 (empty, 0 kg) should come before ship_0001 (180 kg)
    expect(rows[0].textContent).toMatch(/ship_0002/)
    expect(rows[1].textContent).toMatch(/ship_0001/)
  })

  it('shows progress bar for active task', () => {
    const ships: Record<string, ShipState> = {
      ship_0001: {
        id: 'ship_0001',
        location_node: 'node_earth_orbit',
        owner: 'principal_autopilot',
        inventory: [],
        cargo_capacity_m3: 20,
        task: {
          kind: { Mine: { asteroid: 'asteroid_0001', duration_ticks: 100 } },
          started_tick: 0,
          eta_tick: 100,
        },
      },
    }
    render(
      <FleetPanel ships={ships} stations={{}} displayTick={50} />,
    )
    const progressBar = document.querySelector('[role="progressbar"]')
    expect(progressBar).toBeInTheDocument()
    expect(progressBar?.getAttribute('aria-valuenow')).toBe('50')
  })

  it('shows no progress bar for idle ship', () => {
    const ships: Record<string, ShipState> = {
      ship_0001: {
        id: 'ship_0001',
        location_node: 'node_earth_orbit',
        owner: 'principal_autopilot',
        inventory: [],
        cargo_capacity_m3: 20,
        task: null,
      },
    }
    render(
      <FleetPanel ships={ships} stations={{}} displayTick={50} />,
    )
    expect(document.querySelector('[role="progressbar"]')).not.toBeInTheDocument()
  })
})
