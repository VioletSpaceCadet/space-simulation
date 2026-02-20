interface Props {
  tick: number
  connected: boolean
  measuredTickRate: number
}

export function StatusBar({ tick, connected, measuredTickRate }: Props) {
  const roundedTick = Math.floor(tick)
  const day = Math.floor(roundedTick / 1440)
  const hour = Math.floor((roundedTick % 1440) / 60)
  const minute = roundedTick % 60

  return (
    <div className="flex gap-6 items-center px-4 py-1.5 bg-surface border-b border-edge text-xs shrink-0">
      <span className="text-accent font-bold">tick {roundedTick}</span>
      <span className="text-dim">
        day {day} | {String(hour).padStart(2, '0')}:{String(minute).padStart(2, '0')}
      </span>
      <span className="text-muted">~{measuredTickRate.toFixed(1)} t/s</span>
      <span className={connected ? 'text-online' : 'text-offline'}>
        {connected ? '● connected' : '○ reconnecting...'}
      </span>
    </div>
  )
}
