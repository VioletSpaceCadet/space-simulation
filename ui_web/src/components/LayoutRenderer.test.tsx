import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { DndContext } from '@dnd-kit/core'
import { LayoutRenderer } from './LayoutRenderer'
import type { GroupNode, PanelId } from '../layout'

function renderWithDnd(ui: React.ReactNode) {
  return render(<DndContext>{ui}</DndContext>)
}

const renderPanelMock = vi.fn((id: PanelId) => (
  <div data-testid={`content-${id}`}>{id} content</div>
))

describe('LayoutRenderer', () => {
  it('renders leaf panels with their tab headers', () => {
    const layout: GroupNode = {
      type: 'group',
      direction: 'horizontal',
      children: [
        { type: 'leaf', panelId: 'map' },
        { type: 'leaf', panelId: 'fleet' },
      ],
    }

    renderWithDnd(
      <LayoutRenderer
        layout={layout}
        renderPanel={renderPanelMock}
        isDragging={false}
        activeDragId={null}
      />,
    )

    expect(screen.getByText('Map')).toBeInTheDocument()
    expect(screen.getByText('Fleet')).toBeInTheDocument()
  })

  it('renders nested vertical groups with all labels present', () => {
    const layout: GroupNode = {
      type: 'group',
      direction: 'horizontal',
      children: [
        { type: 'leaf', panelId: 'map' },
        {
          type: 'group',
          direction: 'vertical',
          children: [
            { type: 'leaf', panelId: 'events' },
            { type: 'leaf', panelId: 'asteroids' },
          ],
        },
      ],
    }

    renderWithDnd(
      <LayoutRenderer
        layout={layout}
        renderPanel={renderPanelMock}
        isDragging={false}
        activeDragId={null}
      />,
    )

    expect(screen.getByText('Map')).toBeInTheDocument()
    expect(screen.getByText('Events')).toBeInTheDocument()
    expect(screen.getByText('Asteroids')).toBeInTheDocument()
  })

  it('renders resize handles between panels', () => {
    const layout: GroupNode = {
      type: 'group',
      direction: 'horizontal',
      children: [
        { type: 'leaf', panelId: 'map' },
        { type: 'leaf', panelId: 'events' },
        { type: 'leaf', panelId: 'asteroids' },
      ],
    }

    const { container } = renderWithDnd(
      <LayoutRenderer
        layout={layout}
        renderPanel={renderPanelMock}
        isDragging={false}
        activeDragId={null}
      />,
    )

    const handles = container.querySelectorAll('[data-panel-resize-handle-id]')
    expect(handles).toHaveLength(2)
  })

  it('calls renderPanel for each leaf', () => {
    const mockRender = vi.fn((id: PanelId) => (
      <div data-testid={`content-${id}`}>{id}</div>
    ))

    const layout: GroupNode = {
      type: 'group',
      direction: 'horizontal',
      children: [
        { type: 'leaf', panelId: 'map' },
        { type: 'leaf', panelId: 'research' },
      ],
    }

    renderWithDnd(
      <LayoutRenderer
        layout={layout}
        renderPanel={mockRender}
        isDragging={false}
        activeDragId={null}
      />,
    )

    expect(mockRender).toHaveBeenCalledWith('map')
    expect(mockRender).toHaveBeenCalledWith('research')
    expect(mockRender).toHaveBeenCalledTimes(2)
  })
})
