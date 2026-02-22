import { useDroppable } from '@dnd-kit/core'
import { wouldMoveChange } from '../layout'
import type { LayoutNode, PanelId } from '../layout'

type Position = 'before' | 'after' | 'above' | 'below'

/** Thin hit areas along each edge for drop detection */
const HIT_CLASSES: Record<Position, string> = {
  before: 'left-0 top-0 w-[20%] h-full',
  after: 'right-0 top-0 w-[20%] h-full',
  above: 'top-0 left-[20%] w-[60%] h-[30%]',
  below: 'bottom-0 left-[20%] w-[60%] h-[30%]',
}

/** Highlighted preview region showing where the panel will land */
const PREVIEW_CLASSES: Record<Position, string> = {
  before: 'left-0 top-0 w-1/2 h-full',
  after: 'right-0 top-0 w-1/2 h-full',
  above: 'top-0 left-0 w-full h-1/2',
  below: 'bottom-0 left-0 w-full h-1/2',
}

const POSITIONS: Position[] = ['before', 'after', 'above', 'below']

function Zone({ panelId, position }: { panelId: PanelId; position: Position }) {
  const droppableId = `${panelId}:${position}`
  const { isOver, setNodeRef } = useDroppable({
    id: droppableId,
    data: { targetPanelId: panelId, position },
  })

  return (
    <>
      {/* Invisible hit area along the edge */}
      <div
        ref={setNodeRef}
        data-testid={`drop-zone-${position}`}
        data-drop-zone={droppableId}
        className={`absolute z-50 ${HIT_CLASSES[position]}`}
      />
      {/* Visible preview highlight when hovered */}
      {isOver && (
        <div
          className={`absolute z-40 pointer-events-none transition-all duration-150 rounded-sm bg-accent/15 border-2 border-accent/40 ${PREVIEW_CLASSES[position]}`}
        />
      )}
    </>
  )
}

interface DropZoneOverlayProps {
  panelId: PanelId
  active: boolean
  layout: LayoutNode
  dragSourceId: PanelId | null
}

export function DropZoneOverlay({ panelId, active, layout, dragSourceId }: DropZoneOverlayProps) {
  if (!active || !dragSourceId) return null

  const validPositions = POSITIONS.filter((position) =>
    wouldMoveChange(layout, dragSourceId, panelId, position),
  )

  return (
    <>
      {validPositions.map((position) => (
        <Zone key={position} panelId={panelId} position={position} />
      ))}
    </>
  )
}
