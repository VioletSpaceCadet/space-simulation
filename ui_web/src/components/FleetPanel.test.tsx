import { render, screen } from '@testing-library/react'
import { FleetPanel } from './FleetPanel'
import type { ShipState } from '../types'

const mockShip: ShipState = {
  id: 'ship_0001',
  location_node: 'node_earth_orbit',
  owner: 'principal_autopilot',
  inventory: [
    {
      kind: 'Ore',
      lot_id: 'lot_0001',
      asteroid_id: 'asteroid_0001',
      kg: 180.0,
      composition: { Fe: 0.83, Si: 0.17 },
    },
  ],
  cargo_capacity_m3: 20.0,
  task: null,
}

it('renders ship id', () => {
  render(<FleetPanel ships={{ ship_0001: mockShip }} stations={{}} />)
  expect(screen.getByText(/ship_0001/)).toBeInTheDocument()
})

it('renders ore item', () => {
  render(<FleetPanel ships={{ ship_0001: mockShip }} stations={{}} />)
  expect(screen.getByText(/ore/)).toBeInTheDocument()
})

it('renders empty state when no ships', () => {
  render(<FleetPanel ships={{}} stations={{}} />)
  expect(screen.getByText(/no ships/i)).toBeInTheDocument()
})
