interface Props {
  tick: number
  connected: boolean
  view: 'dashboard' | 'map'
  onToggleView: () => void
}

export function StatusBar({ tick, connected, view, onToggleView }: Props) {
  const day = Math.floor(tick / 1440)
  const hour = Math.floor((tick % 1440) / 60)
  const minute = tick % 60

  return (
    <div className="flex gap-6 items-center px-4 py-1.5 bg-surface border-b border-edge text-xs shrink-0">
      <button
        onClick={onToggleView}
        className="text-accent hover:text-bright transition-colors cursor-pointer"
      >
        {view === 'dashboard' ? '◈ System Map' : '☰ Dashboard'}
      </button>
      <span className="text-bright font-bold">tick {tick}</span>
      <span className="text-dim">
        day {day} | {String(hour).padStart(2, '0')}:{String(minute).padStart(2, '0')}
      </span>
      <span className={connected ? 'text-online' : 'text-offline'}>
        {connected ? '● connected' : '○ reconnecting...'}
      </span>
    </div>
  )
}
