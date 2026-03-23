import type { SimEvent } from '../types';
import { getEventKey } from '../utils';

interface Props {
  events: SimEvent[]
}

interface SimEventPayload {
  event_def_id: string;
  target: { type: string; station_id?: string; ship_id?: string; zone_id?: string };
  effects_applied: unknown[];
}

function resolveTargetName(target: SimEventPayload['target']): string {
  switch (target.type) {
    case 'station': return target.station_id ?? 'unknown station';
    case 'ship': return target.ship_id ?? 'unknown ship';
    case 'zone': return target.zone_id ?? 'unknown zone';
    case 'global': return 'the system';
    default: return target.type;
  }
}

function isSimEventFired(event: Record<string, unknown>): event is { SimEventFired: SimEventPayload } {
  return 'SimEventFired' in event;
}

function isSimEventExpired(event: Record<string, unknown>): event is { SimEventExpired: { event_def_id: string } } {
  return 'SimEventExpired' in event;
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

/** Render a sim event with category styling and target info. */
function SimEventRow({ evt, payload }: { evt: SimEvent; payload: SimEventPayload }) {
  const targetName = resolveTargetName(payload.target);
  // Use event_def_id as display name (human-readable names come from content API in future)
  const displayName = payload.event_def_id.replace(/^evt_/, '').replace(/_/g, ' ');
  const effectCount = payload.effects_applied.length;

  return (
    <div className="flex gap-1.5 py-0.5 border-b border-surface/50 text-[11px] overflow-hidden bg-surface/30">
      <span className="text-faint min-w-[44px] shrink-0">t={evt.tick}</span>
      <span className="text-amber-300 min-w-[80px] shrink-0 font-medium">EVENT</span>
      <span className="text-slate-200 capitalize">{displayName}</span>
      <span className="text-faint">at {targetName}</span>
      {effectCount > 0 && <span className="text-faint">({effectCount} effects)</span>}
    </div>
  );
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
      {events.map((evt) => {
        const event = evt.event as Record<string, unknown>;

        if (isSimEventFired(event)) {
          return <SimEventRow key={evt.id} evt={evt} payload={event.SimEventFired} />;
        }

        if (isSimEventExpired(event)) {
          const defId = event.SimEventExpired.event_def_id.replace(/^evt_/, '').replace(/_/g, ' ');
          return (
            <div key={evt.id} className="flex gap-1.5 py-0.5 border-b border-surface text-[11px] overflow-hidden">
              <span className="text-faint min-w-[44px] shrink-0">t={evt.tick}</span>
              <span className="text-slate-500 min-w-[80px] shrink-0">EXPIRED</span>
              <span className="text-slate-400 capitalize">{defId} effect ended</span>
            </div>
          );
        }

        return (
          <div key={evt.id} className="flex gap-1.5 py-0.5 border-b border-surface text-[11px] overflow-hidden">
            <span className="text-faint min-w-[90px] shrink-0">{evt.id}</span>
            <span className="text-faint min-w-[44px] shrink-0">t={evt.tick}</span>
            <span className="text-accent min-w-[120px] shrink-0">{getEventKey(event)}</span>
            <span className="text-muted overflow-hidden text-ellipsis whitespace-nowrap">{eventDetail(event)}</span>
          </div>
        );
      })}
    </div>
  );
}
