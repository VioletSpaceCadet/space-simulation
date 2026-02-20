import { fireEvent, render, screen } from '@testing-library/react'
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
  localStorage.clear()
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

  it('renders nav with all four panel names', () => {
    render(<App />)
    const nav = screen.getByRole('navigation')
    expect(nav).toBeInTheDocument()
    const buttons = Array.from(nav.querySelectorAll('button'))
    const labels = buttons.map((b) => b.textContent)
    expect(labels).toEqual(['Events', 'Asteroids', 'Fleet', 'Research'])
  })

  it('renders all four panel headings by default', () => {
    render(<App />)
    expect(screen.getAllByText('Events')).toHaveLength(2) // nav + panel heading
    expect(screen.getAllByText('Asteroids')).toHaveLength(2)
    expect(screen.getAllByText('Fleet')).toHaveLength(2)
    expect(screen.getAllByText('Research')).toHaveLength(2)
  })

  it('hides panel when nav button clicked', () => {
    render(<App />)
    const nav = screen.getByRole('navigation')
    const eventsButton = Array.from(nav.querySelectorAll('button')).find(
      (b) => b.textContent === 'Events',
    )!
    fireEvent.click(eventsButton)
    // Events should now only appear in nav, not as a panel heading
    expect(screen.getAllByText('Events')).toHaveLength(1)
  })

  it('renders resize handles between panels', () => {
    render(<App />)
    const handles = document.querySelectorAll('[data-panel-resize-handle-id]')
    expect(handles.length).toBeGreaterThan(0)
  })
})
