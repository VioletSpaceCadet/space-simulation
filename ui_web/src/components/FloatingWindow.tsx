import { useCallback, useRef } from 'react';

import { PANEL_LABELS, type PanelId } from '../layout';

interface FloatingWindowProps {
  id: string
  panelId: PanelId
  x: number
  y: number
  width: number
  height: number
  zIndex: number
  onClose: (id: string) => void
  onUpdate: (id: string, updates: { x?: number; y?: number; width?: number; height?: number }) => void
  onFocus: (id: string) => void
  onDock: (id: string, panelId: PanelId) => void
  children: React.ReactNode
}

const MIN_WIDTH = 200;
const MIN_HEIGHT = 150;
const VISIBLE_MARGIN = 50; // px that must remain on-screen

export function FloatingWindow({
  id, panelId, x, y, width, height, zIndex,
  onClose, onUpdate, onFocus, onDock, children,
}: FloatingWindowProps) {
  const dragRef = useRef<{
    startX: number; startY: number; origX: number; origY: number
  } | null>(null);
  const resizeRef = useRef<{
    startX: number; startY: number; origW: number; origH: number
  } | null>(null);

  const handleDragStart = useCallback((e: React.PointerEvent) => {
    e.preventDefault();
    onFocus(id);
    dragRef.current = {
      startX: e.clientX, startY: e.clientY,
      origX: x, origY: y,
    };

    const onMove = (me: PointerEvent) => {
      if (!dragRef.current) { return; }
      const dx = me.clientX - dragRef.current.startX;
      const dy = me.clientY - dragRef.current.startY;
      const maxX = window.innerWidth - VISIBLE_MARGIN;
      const maxY = window.innerHeight - VISIBLE_MARGIN;
      onUpdate(id, {
        x: Math.min(maxX, Math.max(0, dragRef.current.origX + dx)),
        y: Math.min(maxY, Math.max(0, dragRef.current.origY + dy)),
      });
    };

    const onUp = () => {
      dragRef.current = null;
      document.removeEventListener('pointermove', onMove);
      document.removeEventListener('pointerup', onUp);
    };

    document.addEventListener('pointermove', onMove);
    document.addEventListener('pointerup', onUp);
  }, [id, x, y, onFocus, onUpdate]);

  const handleResizeStart = useCallback((e: React.PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    onFocus(id);
    resizeRef.current = {
      startX: e.clientX, startY: e.clientY,
      origW: width, origH: height,
    };

    const onMove = (me: PointerEvent) => {
      if (!resizeRef.current) { return; }
      const dw = me.clientX - resizeRef.current.startX;
      const dh = me.clientY - resizeRef.current.startY;
      onUpdate(id, {
        width: Math.max(MIN_WIDTH, resizeRef.current.origW + dw),
        height: Math.max(MIN_HEIGHT, resizeRef.current.origH + dh),
      });
    };

    const onUp = () => {
      resizeRef.current = null;
      document.removeEventListener('pointermove', onMove);
      document.removeEventListener('pointerup', onUp);
    };

    document.addEventListener('pointermove', onMove);
    document.addEventListener('pointerup', onUp);
  }, [id, width, height, onFocus, onUpdate]);

  return (
    <div
      data-testid={`floating-window-${panelId}`}
      className="absolute bg-void border border-edge rounded-lg shadow-2xl flex flex-col overflow-hidden"
      style={{ left: x, top: y, width, height, zIndex }}
      onPointerDown={() => onFocus(id)}
    >
      {/* Title bar — drag handle */}
      <div
        className="flex items-center px-3 py-1.5 bg-surface border-b border-edge shrink-0
          cursor-grab active:cursor-grabbing select-none"
        onPointerDown={handleDragStart}
        data-testid={`floating-titlebar-${panelId}`}
      >
        <span className="text-[11px] uppercase tracking-widest text-label flex-1">
          {PANEL_LABELS[panelId]}
        </span>
        <button
          type="button"
          onClick={() => onDock(id, panelId)}
          className="text-[10px] text-muted hover:text-dim px-1.5 py-0.5 rounded cursor-pointer"
          title="Dock to layout"
        >
          dock
        </button>
        <button
          type="button"
          onClick={() => onClose(id)}
          className="text-muted hover:text-dim ml-1 text-sm leading-none cursor-pointer"
          title="Close"
          data-testid={`floating-close-${panelId}`}
        >
          ×
        </button>
      </div>

      {/* Content area */}
      <div className="flex-1 min-h-0 overflow-y-auto p-3">
        {children}
      </div>

      {/* Resize handle (bottom-right corner) */}
      <div
        className="absolute bottom-0 right-0 w-4 h-4 cursor-se-resize"
        onPointerDown={handleResizeStart}
        data-testid={`floating-resize-${panelId}`}
      >
        <svg
          viewBox="0 0 16 16"
          className="w-full h-full text-muted opacity-50"
        >
          <path
            d="M14 14L8 14M14 14L14 8M14 14L6 6"
            stroke="currentColor"
            strokeWidth="1.5"
            fill="none"
          />
        </svg>
      </div>
    </div>
  );
}
