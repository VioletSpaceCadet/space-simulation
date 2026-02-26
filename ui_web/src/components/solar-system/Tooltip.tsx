interface TooltipProps {
  x: number
  y: number
  children: React.ReactNode
}

export function Tooltip({ x, y, children }: TooltipProps) {
  return (
    <div
      className="fixed pointer-events-none bg-surface border border-edge rounded px-2 py-1 text-[11px] text-fg z-10 max-w-[200px] -translate-x-1/2"
      style={{ left: x, top: y - 4, transform: 'translate(-50%, -100%)' }}
    >
      {children}
    </div>
  );
}
