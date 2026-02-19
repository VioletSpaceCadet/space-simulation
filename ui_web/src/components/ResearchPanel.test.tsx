import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { ResearchPanel } from './ResearchPanel'
import type { ResearchState } from '../types'

const research: ResearchState = {
  unlocked: [],
  data_pool: { ScanData: 42.5 },
  evidence: { tech_deep_scan_v1: 120.0 },
}

const researchUnlocked: ResearchState = {
  unlocked: ['tech_deep_scan_v1'],
  data_pool: { ScanData: 200.0 },
  evidence: { tech_deep_scan_v1: 300.0 },
}

describe('ResearchPanel', () => {
  it('renders tech ID', () => {
    render(<ResearchPanel research={research} />)
    expect(screen.getByText(/tech_deep_scan_v1/)).toBeInTheDocument()
  })

  it('renders evidence value', () => {
    render(<ResearchPanel research={research} />)
    expect(screen.getByText(/120/)).toBeInTheDocument()
  })

  it('shows unlocked label when tech is unlocked', () => {
    render(<ResearchPanel research={researchUnlocked} />)
    expect(screen.getByText(/unlocked/i)).toBeInTheDocument()
  })

  it('shows data pool amount', () => {
    render(<ResearchPanel research={research} />)
    expect(screen.getByText(/42/)).toBeInTheDocument()
  })
})
