import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { Tooltip } from './Tooltip'

describe('Tooltip', () => {
  it('renders children', () => {
    render(<Tooltip x={100} y={200}>Hello tooltip</Tooltip>)
    expect(screen.getByText('Hello tooltip')).toBeInTheDocument()
  })

  it('positions at coordinates', () => {
    render(<Tooltip x={150} y={250}>Content</Tooltip>)
    const tooltip = screen.getByText('Content').closest('div')!
    expect(tooltip.style.left).toBe('150px')
    expect(tooltip.style.top).toBe('246px')
  })
})
