import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { StatusBar } from './StatusBar'

describe('StatusBar', () => {
  it('renders tick number', () => {
    render(<StatusBar tick={1440} connected={true} measuredTickRate={10} />)
    expect(screen.getByText(/1440/)).toBeInTheDocument()
  })

  it('shows day and hour derived from tick', () => {
    render(<StatusBar tick={1440} connected={true} measuredTickRate={10} />)
    expect(screen.getByText(/day 1/i)).toBeInTheDocument()
  })

  it('shows connected when connected', () => {
    render(<StatusBar tick={0} connected={true} measuredTickRate={10} />)
    expect(screen.getByText(/connected/i)).toBeInTheDocument()
  })

  it('shows reconnecting when not connected', () => {
    render(<StatusBar tick={0} connected={false} measuredTickRate={10} />)
    expect(screen.getByText(/reconnecting/i)).toBeInTheDocument()
  })

  it('displays measured tick rate', () => {
    render(<StatusBar tick={0} connected={true} measuredTickRate={9.7} />)
    expect(screen.getByText(/~9\.7 t\/s/)).toBeInTheDocument()
  })

  it('floors fractional ticks for display', () => {
    render(<StatusBar tick={1440.7} connected={true} measuredTickRate={10} />)
    expect(screen.getByText(/1440/)).toBeInTheDocument()
    // Should not show the fractional part
    expect(screen.queryByText(/1440\.7/)).not.toBeInTheDocument()
  })
})
