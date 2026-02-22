import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { StatusBar } from './StatusBar'

const defaultAlertProps = {
  alerts: new Map(),
  dismissedAlerts: new Set<string>(),
  onDismissAlert: () => {},
}

describe('StatusBar', () => {
  it('renders tick number', () => {
    render(<StatusBar tick={1440} connected={true} measuredTickRate={10} {...defaultAlertProps} />)
    expect(screen.getByText(/1440/)).toBeInTheDocument()
  })

  it('shows day and hour derived from tick', () => {
    render(<StatusBar tick={1440} connected={true} measuredTickRate={10} {...defaultAlertProps} />)
    expect(screen.getByText(/day 1/i)).toBeInTheDocument()
  })

  it('shows connected when connected', () => {
    render(<StatusBar tick={0} connected={true} measuredTickRate={10} {...defaultAlertProps} />)
    expect(screen.getByText(/connected/i)).toBeInTheDocument()
  })

  it('shows reconnecting when not connected', () => {
    render(<StatusBar tick={0} connected={false} measuredTickRate={10} {...defaultAlertProps} />)
    expect(screen.getByText(/reconnecting/i)).toBeInTheDocument()
  })

  it('displays measured tick rate', () => {
    render(<StatusBar tick={0} connected={true} measuredTickRate={9.7} {...defaultAlertProps} />)
    expect(screen.getByText(/~9\.7 t\/s/)).toBeInTheDocument()
  })

  it('floors fractional ticks for display', () => {
    render(<StatusBar tick={1440.7} connected={true} measuredTickRate={10} {...defaultAlertProps} />)
    expect(screen.getByText(/1440/)).toBeInTheDocument()
    // Should not show the fractional part
    expect(screen.queryByText(/1440\.7/)).not.toBeInTheDocument()
  })

  it('renders a save button', () => {
    render(<StatusBar tick={0} connected={true} measuredTickRate={10} {...defaultAlertProps} />)
    expect(screen.getByRole('button', { name: /save/i })).toBeInTheDocument()
  })
})
