import type { SimEvent } from '../types'

interface Props {
  events: SimEvent[]
}

function eventType(event: Record<string, unknown>): string {
  return Object.keys(event)[0] ?? 'Unknown'
}

function eventDetail(event: Record<string, unknown>): string {
  const key = Object.keys(event)[0]
  if (!key) return ''
  const value = event[key] as Record<string, unknown>
  if (!value || typeof value !== 'object') return ''
  return Object.entries(value)
    .map(([k, v]) => `${k}=${String(v)}`)
    .join(' ')
}

export function EventsFeed({ events }: Props) {
  if (events.length === 0) {
    return (
      <div className="events-feed">
        <div className="events-empty">no events yet</div>
      </div>
    )
  }

  return (
    <div className="events-feed">
      {events.map((evt) => (
        <div key={evt.id} className="event-row">
          <span className="event-id">{evt.id}</span>
          <span className="event-tick">t={evt.tick}</span>
          <span className="event-type">{eventType(evt.event)}</span>
          <span className="event-detail">{eventDetail(evt.event)}</span>
        </div>
      ))}
    </div>
  )
}
