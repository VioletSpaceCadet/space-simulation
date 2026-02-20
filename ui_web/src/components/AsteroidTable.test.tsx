import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { AsteroidTable } from './AsteroidTable'
import type { AsteroidState } from '../types'

const asteroids: Record<string, AsteroidState> = {
  'asteroid_0001': {
    id: 'asteroid_0001',
    location_node: 'node_belt_inner',
    anomaly_tags: ['IronRich'],
    mass_kg: 5000,
    knowledge: {
      tag_beliefs: [['IronRich', 0.85]],
      composition: { Fe: 0.65, Si: 0.20, He: 0.15 },
    },
  },
  'asteroid_0002': {
    id: 'asteroid_0002',
    location_node: 'node_belt_outer',
    anomaly_tags: ['IronRich'],
    mass_kg: 1000,
    knowledge: {
      tag_beliefs: [['IronRich', 0.90]],
      composition: { Fe: 0.80, Si: 0.10, He: 0.10 },
    },
  },
}

describe('AsteroidTable', () => {
  it('renders asteroid ID', () => {
    render(<AsteroidTable asteroids={asteroids} />)
    expect(screen.getByText('asteroid_0001')).toBeInTheDocument()
  })

  it('renders tag with confidence', () => {
    render(<AsteroidTable asteroids={asteroids} />)
    expect(screen.getAllByText(/IronRich/).length).toBeGreaterThan(0)
  })

  it('renders Fe composition percentage', () => {
    render(<AsteroidTable asteroids={asteroids} />)
    expect(screen.getByText(/65%/)).toBeInTheDocument()
  })

  it('shows empty state when no asteroids', () => {
    render(<AsteroidTable asteroids={{}} />)
    expect(screen.getByText(/no bodies discovered/i)).toBeInTheDocument()
  })

  it('renders sort indicators on column headers', () => {
    render(<AsteroidTable asteroids={asteroids} />)
    const headers = screen.getAllByRole('columnheader')
    expect(headers.some((h) => h.textContent?.includes('⇅'))).toBe(true)
  })

  it('sorts by mass ascending on click', () => {
    render(<AsteroidTable asteroids={asteroids} />)
    const massHeader = screen.getByText(/Mass/)
    fireEvent.click(massHeader)
    const cells = document.querySelectorAll('tbody tr td:nth-child(5)')
    const masses = Array.from(cells).map((c) => c.textContent)
    expect(masses[0]).toMatch(/1,000/)
    expect(masses[1]).toMatch(/5,000/)
  })
})
