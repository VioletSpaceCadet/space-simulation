import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { StatusBar } from './StatusBar'

describe('StatusBar', () => {
  it('renders tick number', () => {
    render(<StatusBar tick={1440} connected={true} view="dashboard" onToggleView={vi.fn()} />)
    expect(screen.getByText(/1440/)).toBeInTheDocument()
  })

  it('shows day and hour derived from tick', () => {
    render(<StatusBar tick={1440} connected={true} view="dashboard" onToggleView={vi.fn()} />)
    expect(screen.getByText(/day 1/i)).toBeInTheDocument()
  })

  it('shows connected when connected', () => {
    render(<StatusBar tick={0} connected={true} view="dashboard" onToggleView={vi.fn()} />)
    expect(screen.getByText(/connected/i)).toBeInTheDocument()
  })

  it('shows reconnecting when not connected', () => {
    render(<StatusBar tick={0} connected={false} view="dashboard" onToggleView={vi.fn()} />)
    expect(screen.getByText(/reconnecting/i)).toBeInTheDocument()
  })

  it('renders toggle button showing System Map when in dashboard view', () => {
    render(<StatusBar tick={0} connected={true} view="dashboard" onToggleView={vi.fn()} />)
    expect(screen.getByText(/System Map/)).toBeInTheDocument()
  })

  it('renders toggle button showing Dashboard when in map view', () => {
    render(<StatusBar tick={0} connected={true} view="map" onToggleView={vi.fn()} />)
    expect(screen.getByText(/Dashboard/)).toBeInTheDocument()
  })

  it('calls onToggleView when button clicked', async () => {
    const toggle = vi.fn()
    render(<StatusBar tick={0} connected={true} view="dashboard" onToggleView={toggle} />)
    await userEvent.click(screen.getByText(/System Map/))
    expect(toggle).toHaveBeenCalledOnce()
  })
})
