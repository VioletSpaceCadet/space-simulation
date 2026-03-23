import { useDraggable } from '@dnd-kit/core';

import { PANEL_LABELS, type PanelId } from '../layout';

interface DraggableTabProps {
  panelId: PanelId
  isDragging?: boolean
  onPopOut?: (panelId: PanelId) => void
}

export function DraggableTab({ panelId, isDragging, onPopOut }: DraggableTabProps) {
  const { attributes, listeners, setNodeRef } = useDraggable({
    id: `tab-${panelId}`,
    data: { panelId },
  });

  return (
    <div
      data-panel-tab={panelId}
      data-testid={`panel-tab-${panelId}`}
      className={`flex items-center text-[11px] uppercase tracking-widest text-label pb-1.5 border-b border-edge shrink-0 select-none transition-opacity ${isDragging ? 'opacity-50' : 'hover:text-dim'}`}
    >
      <div
        ref={setNodeRef}
        {...listeners}
        {...attributes}
        className="flex items-center flex-1 cursor-grab active:cursor-grabbing"
      >
        <span className="mr-1.5 text-[9px] text-muted">⠿</span>
        {PANEL_LABELS[panelId]}
      </div>
      {onPopOut && (
        <button
          type="button"
          onClick={() => onPopOut(panelId)}
          className="text-[9px] text-muted hover:text-dim px-1 cursor-pointer"
          title="Pop out to floating window"
          data-testid={`pop-out-${panelId}`}
        >
          ↗
        </button>
      )}
    </div>
  );
}
