import { render, screen } from '@testing-library/react'
import { FleetPanel } from './FleetPanel'
import type { ShipState } from '../types'

const mockShip: ShipState = {
  id: 'ship_0001',
  location_node: 'node_earth_orbit',
  owner: 'principal_autopilot',
  cargo: { Fe: 150.0, Si: 30.0 },
  cargo_capacity_m3: 20.0,
  task: null,
}

it('renders ship id', () => {
  render(<FleetPanel ships={{ ship_0001: mockShip }} stations={{}} oreCompositions={{ ships: {}, stations: {} }} />)
  expect(screen.getByText(/ship_0001/)).toBeInTheDocument()
})

it('renders cargo elements', () => {
  render(<FleetPanel ships={{ ship_0001: mockShip }} stations={{}} oreCompositions={{ ships: {}, stations: {} }} />)
  expect(screen.getByText(/Fe/)).toBeInTheDocument()
})

it('renders empty state when no ships', () => {
  render(<FleetPanel ships={{}} stations={{}} oreCompositions={{ ships: {}, stations: {} }} />)
  expect(screen.getByText(/no ships/i)).toBeInTheDocument()
})
