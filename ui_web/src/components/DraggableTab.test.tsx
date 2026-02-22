import { render, screen } from '@testing-library/react'
import { DndContext } from '@dnd-kit/core'
import { DraggableTab } from './DraggableTab'

function renderTab(props: { panelId: 'map' | 'events' | 'asteroids' | 'fleet' | 'research'; isDragging?: boolean }) {
  return render(
    <DndContext>
      <DraggableTab {...props} />
    </DndContext>,
  )
}

describe('DraggableTab', () => {
  it('renders the panel label text', () => {
    renderTab({ panelId: 'asteroids' })
    expect(screen.getByText('Asteroids')).toBeInTheDocument()
  })

  it('has data-panel-tab attribute matching panelId', () => {
    renderTab({ panelId: 'fleet' })
    expect(screen.getByTestId('panel-tab-fleet')).toHaveAttribute('data-panel-tab', 'fleet')
  })

  it('applies opacity-50 when isDragging is true', () => {
    renderTab({ panelId: 'map', isDragging: true })
    const tab = screen.getByTestId('panel-tab-map')
    expect(tab.className).toContain('opacity-50')
  })

  it('does not apply opacity-50 when isDragging is false', () => {
    renderTab({ panelId: 'map', isDragging: false })
    const tab = screen.getByTestId('panel-tab-map')
    expect(tab.className).not.toContain('opacity-50')
  })
})
