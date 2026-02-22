import { useDraggable } from '@dnd-kit/core'
import { PANEL_LABELS, type PanelId } from '../layout'

interface DraggableTabProps {
  panelId: PanelId
  isDragging?: boolean
}

export function DraggableTab({ panelId, isDragging }: DraggableTabProps) {
  const { attributes, listeners, setNodeRef } = useDraggable({
    id: `tab-${panelId}`,
    data: { panelId },
  })

  return (
    <div
      ref={setNodeRef}
      {...listeners}
      {...attributes}
      data-panel-tab={panelId}
      data-testid={`panel-tab-${panelId}`}
      className={`flex items-center text-[11px] uppercase tracking-widest text-label pb-1.5 border-b border-edge shrink-0 cursor-grab active:cursor-grabbing select-none transition-opacity ${isDragging ? 'opacity-50' : 'hover:text-dim'}`}
    >
      <span className="mr-1.5 text-[9px] text-muted">⠿</span>
      {PANEL_LABELS[panelId]}
    </div>
  )
}
