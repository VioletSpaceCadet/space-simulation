import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { StatusBar } from './StatusBar'

describe('StatusBar', () => {
  it('renders tick number', () => {
    render(<StatusBar tick={1440} connected={true} />)
    expect(screen.getByText(/1440/)).toBeInTheDocument()
  })

  it('shows day and hour derived from tick', () => {
    render(<StatusBar tick={1440} connected={true} />)
    expect(screen.getByText(/day 1/i)).toBeInTheDocument()
  })

  it('shows connected when connected', () => {
    render(<StatusBar tick={0} connected={true} />)
    expect(screen.getByText(/connected/i)).toBeInTheDocument()
  })

  it('shows reconnecting when not connected', () => {
    render(<StatusBar tick={0} connected={false} />)
    expect(screen.getByText(/reconnecting/i)).toBeInTheDocument()
  })
})
