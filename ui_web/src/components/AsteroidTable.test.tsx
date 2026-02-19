import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { AsteroidTable } from './AsteroidTable'
import type { AsteroidState } from '../types'

const asteroids: Record<string, AsteroidState> = {
  'asteroid_0001': {
    id: 'asteroid_0001',
    location_node: 'node_belt_inner',
    anomaly_tags: ['IronRich'],
    knowledge: {
      tag_beliefs: [['IronRich', 0.85]],
      composition: { Fe: 0.65, Si: 0.20, He: 0.15 },
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
    expect(screen.getByText(/IronRich/)).toBeInTheDocument()
  })

  it('renders Fe composition percentage', () => {
    render(<AsteroidTable asteroids={asteroids} />)
    expect(screen.getByText(/65%/)).toBeInTheDocument()
  })

  it('shows empty state when no asteroids', () => {
    render(<AsteroidTable asteroids={{}} />)
    expect(screen.getByText(/no asteroids/i)).toBeInTheDocument()
  })
})
