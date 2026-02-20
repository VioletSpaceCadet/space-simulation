interface TooltipProps {
  x: number
  y: number
  children: React.ReactNode
}

export function Tooltip({ x, y, children }: TooltipProps) {
  return (
    <div
      className="absolute pointer-events-none bg-surface border border-edge rounded px-2 py-1 text-[11px] text-fg z-10 max-w-[200px]"
      style={{ left: x + 12, top: y - 8 }}
    >
      {children}
    </div>
  )
}
