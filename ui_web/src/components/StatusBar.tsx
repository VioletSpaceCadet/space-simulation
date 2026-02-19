interface Props {
  tick: number
  connected: boolean
}

export function StatusBar({ tick, connected }: Props) {
  const day = Math.floor(tick / 1440)
  const hour = Math.floor((tick % 1440) / 60)
  const minute = tick % 60

  return (
    <div className="flex gap-6 items-center px-4 py-1.5 bg-[#0d1226] border-b border-[#1e2d50] text-xs shrink-0">
      <span className="text-[#a8c4e8] font-bold">tick {tick}</span>
      <span className="text-[#7a9cc8]">
        day {day} | {String(hour).padStart(2, '0')}:{String(minute).padStart(2, '0')}
      </span>
      <span className={connected ? 'text-[#4caf7d]' : 'text-[#e05555]'}>
        {connected ? '● connected' : '○ reconnecting...'}
      </span>
    </div>
  )
}
