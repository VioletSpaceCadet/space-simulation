interface Props {
  tick: number
  connected: boolean
}

export function StatusBar({ tick, connected }: Props) {
  const day = Math.floor(tick / 1440)
  const hour = Math.floor((tick % 1440) / 60)
  const minute = tick % 60

  return (
    <div className="flex gap-6 items-center px-4 py-1.5 bg-surface border-b border-edge text-xs shrink-0">
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
