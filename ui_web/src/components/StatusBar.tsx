interface Props {
  tick: number
  connected: boolean
}

export function StatusBar({ tick, connected }: Props) {
  const day = Math.floor(tick / 1440)
  const hour = Math.floor((tick % 1440) / 60)
  const minute = tick % 60

  return (
    <div className="status-bar">
      <span className="status-tick">tick {tick}</span>
      <span className="status-time">
        day {day} | {String(hour).padStart(2, '0')}:{String(minute).padStart(2, '0')}
      </span>
      <span className={`status-connection ${connected ? 'connected' : 'disconnected'}`}>
        {connected ? '● connected' : '○ reconnecting...'}
      </span>
    </div>
  )
}
