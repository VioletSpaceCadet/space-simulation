import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import App from './App'
import * as api from './api'
import type { SimSnapshot } from './types'

const snapshot: SimSnapshot = {
  meta: { tick: 0, seed: 42, content_version: '0.0.1' },
  scan_sites: [],
  asteroids: {},
  ships: {},
  stations: {},
  research: { unlocked: [], data_pool: {}, evidence: {} },
}

beforeEach(() => {
  vi.spyOn(api, 'fetchSnapshot').mockResolvedValue(snapshot)
  vi.spyOn(api, 'createEventSource').mockReturnValue({
    onopen: null,
    onerror: null,
    onmessage: null,
    close: vi.fn(),
  } as unknown as EventSource)
})

describe('App', () => {
  it('renders without crashing', () => {
    render(<App />)
    expect(document.body).toBeInTheDocument()
  })

  it('renders status bar with tick', () => {
    render(<App />)
    expect(screen.getByText(/tick/i)).toBeInTheDocument()
  })

  it('renders all three panel headings', () => {
    render(<App />)
    expect(screen.getByText(/events/i)).toBeInTheDocument()
    expect(screen.getByText(/asteroids/i)).toBeInTheDocument()
    expect(screen.getByText(/research/i)).toBeInTheDocument()
  })
})
