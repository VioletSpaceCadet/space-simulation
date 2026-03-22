import type { SimEvent } from '../types';
import { getEventKey } from '../utils';

interface Props {
  events: SimEvent[]
}

function eventDetail(event: Record<string, unknown>): string {
  const key = getEventKey(event);
  if (!key) {return '';}
  const value = event[key] as Record<string, unknown>;
  if (!value || typeof value !== 'object') {return '';}
  return Object.entries(value)
    .map(([k, v]) => `${k}=${String(v)}`)
    .join(' ');
}

export function EventsFeed({ events }: Props) {
  if (events.length === 0) {
    return (
      <div className="overflow-y-auto flex-1">
        <div className="text-faint italic">waiting for stream data</div>
      </div>
    );
  }

  return (
    <div className="overflow-y-auto flex-1">
      {events.map((evt) => (
        <div key={evt.id} className="flex gap-1.5 py-0.5 border-b border-surface text-[11px] overflow-hidden">
          <span className="text-faint min-w-[90px] shrink-0">{evt.id}</span>
          <span className="text-faint min-w-[44px] shrink-0">t={evt.tick}</span>
          <span className="text-accent min-w-[120px] shrink-0">{getEventKey(evt.event)}</span>
          <span className="text-muted overflow-hidden text-ellipsis whitespace-nowrap">{eventDetail(evt.event)}</span>
        </div>
      ))}
    </div>
  );
}
