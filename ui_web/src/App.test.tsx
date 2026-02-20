import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
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

  it('renders all four panel headings', () => {
    render(<App />)
    expect(screen.getByText('Events')).toBeInTheDocument()
    expect(screen.getByText('Asteroids')).toBeInTheDocument()
    expect(screen.getByText('Fleet')).toBeInTheDocument()
    expect(screen.getByText('Research')).toBeInTheDocument()
  })

  it('renders resize handles between panels', () => {
    render(<App />)
    // react-resizable-panels renders [data-panel-resize-handle-id] attributes
    const handles = document.querySelectorAll('[data-panel-resize-handle-id]')
    expect(handles.length).toBeGreaterThan(0)
  })

  it('shows solar system map when toggled to map view', async () => {
    const { container } = render(<App />)
    await userEvent.click(screen.getByText(/System Map/))
    expect(container.querySelector('svg')).toBeInTheDocument()
    expect(screen.queryByText('Events')).not.toBeInTheDocument()
  })
})
