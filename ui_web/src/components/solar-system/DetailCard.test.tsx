import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { DetailCard } from './DetailCard'
import type { AsteroidState, ScanSite, ShipState, StationState } from '../../types'

describe('DetailCard', () => {
  it('renders station detail with inventory', () => {
    const station: StationState = {
      id: 'station_earth_orbit',
      location_node: 'node_earth_orbit',
      power_available_per_tick: 100,
      inventory: [{ kind: 'Material', element: 'Fe', kg: 500.0, quality: 0.85 }],
      cargo_capacity_m3: 100.0,
      facilities: { compute_units_total: 10, power_per_compute_unit_per_tick: 1, efficiency: 1.0 },
      modules: [],
    }
    render(<DetailCard entity={{ type: 'station', data: station }} onClose={() => {}} />)
    expect(screen.getByText('node_earth_orbit')).toBeInTheDocument()
    expect(screen.getByText(/500/)).toBeInTheDocument()
    expect(screen.getByText(/kg/)).toBeInTheDocument()
  })

  it('renders ship detail with task', () => {
    const ship: ShipState = {
      id: 'ship_0001',
      location_node: 'node_belt_inner',
      owner: 'principal_autopilot',
      inventory: [],
      cargo_capacity_m3: 20.0,
      task: {
        kind: { Mine: { asteroid: 'asteroid_0001', duration_ticks: 100 } },
        started_tick: 0,
        eta_tick: 100,
      },
    }
    render(<DetailCard entity={{ type: 'ship', data: ship }} onClose={() => {}} />)
    expect(screen.getByText(/node_belt_inner/)).toBeInTheDocument()
    expect(screen.getByText(/mine/)).toBeInTheDocument()
  })

  it('renders asteroid detail with composition', () => {
    const asteroid: AsteroidState = {
      id: 'asteroid_0001',
      location_node: 'node_belt_inner',
      anomaly_tags: ['IronRich'],
      mass_kg: 50000,
      knowledge: {
        tag_beliefs: [['IronRich', 0.85]],
        composition: { Fe: 0.7, Si: 0.2, Ni: 0.1 },
      },
    }
    render(<DetailCard entity={{ type: 'asteroid', data: asteroid }} onClose={() => {}} />)
    expect(screen.getByText('node_belt_inner')).toBeInTheDocument()
    expect(screen.getByText(/Fe 70%/)).toBeInTheDocument()
    expect(screen.getByText(/Si 20%/)).toBeInTheDocument()
  })

  it('renders scan-site detail', () => {
    const site: ScanSite = {
      id: 'site_0001',
      node: 'node_belt_outer',
      template_id: 'template_rocky',
    }
    render(<DetailCard entity={{ type: 'scan-site', data: site }} onClose={() => {}} />)
    expect(screen.getByText('node_belt_outer')).toBeInTheDocument()
    expect(screen.getByText(/template_rocky/)).toBeInTheDocument()
  })

  it('calls onClose when close button clicked', () => {
    const onClose = vi.fn()
    const site: ScanSite = { id: 'site_0001', node: 'node_belt_outer', template_id: 'template_rocky' }
    render(<DetailCard entity={{ type: 'scan-site', data: site }} onClose={onClose} />)
    fireEvent.click(screen.getByText('✕'))
    expect(onClose).toHaveBeenCalledOnce()
  })
})
