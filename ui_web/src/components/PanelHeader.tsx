interface Props {
  title: string
  collapsed: boolean
  onToggle: () => void
}

export function PanelHeader({ title, collapsed, onToggle }: Props) {
  return (
    <button
      type="button"
      onClick={onToggle}
      className="flex items-center gap-1.5 w-full text-left text-[11px] uppercase tracking-widest text-label mb-2 pb-1.5 border-b border-edge shrink-0 hover:bg-edge/30 transition-colors cursor-pointer rounded-sm px-1 -mx-1"
    >
      <span className="text-[9px] leading-none">{collapsed ? '▸' : '▾'}</span>
      <span>{title}</span>
    </button>
  )
}
