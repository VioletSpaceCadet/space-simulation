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
  research: { unlocked: [], data_pool: {}, evidence: {} },
}

describe('SolarSystemMap', () => {
  it('renders an SVG element', () => {
    const { container } = render(
      <SolarSystemMap snapshot={emptySnapshot} currentTick={100} oreCompositions={{}} />,
    )
    expect(container.querySelector('svg')).toBeInTheDocument()
  })

  it('renders orbital ring labels', () => {
    render(
      <SolarSystemMap snapshot={emptySnapshot} currentTick={100} oreCompositions={{}} />,
    )
    expect(screen.getByText('Earth Orbit')).toBeInTheDocument()
    expect(screen.getByText('Inner Belt')).toBeInTheDocument()
    expect(screen.getByText('Mid Belt')).toBeInTheDocument()
    expect(screen.getByText('Outer Belt')).toBeInTheDocument()
  })
})
