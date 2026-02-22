import { render, screen } from '@testing-library/react'
import { DndContext } from '@dnd-kit/core'
import { describe, expect, it } from 'vitest'
import { DropZoneOverlay } from './DropZoneOverlay'
import { buildDefaultLayout } from '../layout'
import type { GroupNode } from '../layout'

/** Layout: [map, events, fleet] horizontal — all moves are valid */
const threePanel: GroupNode = buildDefaultLayout(['map', 'events', 'fleet'])

function renderWithDnd(ui: React.ReactElement) {
  return render(<DndContext>{ui}</DndContext>)
}

describe('DropZoneOverlay', () => {
  it('renders nothing when active is false', () => {
    renderWithDnd(
      <DropZoneOverlay panelId="map" active={false} layout={threePanel} dragSourceId="fleet" />,
    )
    const positions = ['before', 'after', 'above', 'below'] as const
    for (const position of positions) {
      expect(screen.queryByTestId(`drop-zone-${position}`)).toBeNull()
    }
  })

  it('renders nothing when dragSourceId is null', () => {
    renderWithDnd(
      <DropZoneOverlay panelId="events" active={true} layout={threePanel} dragSourceId={null} />,
    )
    const positions = ['before', 'after', 'above', 'below'] as const
    for (const position of positions) {
      expect(screen.queryByTestId(`drop-zone-${position}`)).toBeNull()
    }
  })

  it('renders drop zones for positions that would change layout', () => {
    renderWithDnd(
      <DropZoneOverlay panelId="events" active={true} layout={threePanel} dragSourceId="fleet" />,
    )
    // fleet is to the right of events, so "after" is a no-op (fleet is already after events)
    // but before, above, below should all be valid
    expect(screen.getByTestId('drop-zone-before')).toBeInTheDocument()
    expect(screen.getByTestId('drop-zone-above')).toBeInTheDocument()
    expect(screen.getByTestId('drop-zone-below')).toBeInTheDocument()
  })

  it('omits drop zone for no-op position', () => {
    // Dragging fleet onto events: fleet is already right of events, so "after" is no-op
    renderWithDnd(
      <DropZoneOverlay panelId="events" active={true} layout={threePanel} dragSourceId="fleet" />,
    )
    expect(screen.queryByTestId('drop-zone-after')).toBeNull()
  })

  it('has correct data-drop-zone attributes', () => {
    renderWithDnd(
      <DropZoneOverlay panelId="events" active={true} layout={threePanel} dragSourceId="map" />,
    )
    // map is to the left of events, so "before" is a no-op
    // after, above, below should show
    const zone = screen.getByTestId('drop-zone-after')
    expect(zone.getAttribute('data-drop-zone')).toBe('events:after')
  })
})
