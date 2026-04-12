/**
 * Station summary cards rendered inline in the CopilotKit sidebar.
 * Shows each station's module count, crew, net power, and average wear
 * with wear-band coloring (nominal/degraded/critical).
 */

import { SEMANTIC_COLORS } from '../../config/theme';

interface StationSummaryEntry {
  id: string;
  module_count: number;
  crew_total: number;
  power_net_kw: number;
  avg_wear: number;
}

interface StationsData {
  total: number;
  summary: StationSummaryEntry[];
}

/** Wear-band thresholds matching sim_core's 3-band system. */
function wearColor(wear: number): string {
  if (wear < 0.5) { return SEMANTIC_COLORS.positive; }
  if (wear < 0.8) { return '#d4a44c'; }
  return SEMANTIC_COLORS.negative;
}

function formatWear(wear: number): string {
  return `${(wear * 100).toFixed(0)}%`;
}

export function StationSummary({ data }: { data: StationsData }) {
  if (data.summary.length === 0) {
    return (
      <div style={{ padding: '8px 0', fontSize: '12px', color: 'var(--copilot-foreground, #8a8e98)' }}>
        No stations built yet.
      </div>
    );
  }

  return (
    <div style={{ padding: '8px 0' }}>
      <div style={{
        fontSize: '11px',
        fontWeight: 600,
        textTransform: 'uppercase',
        letterSpacing: '0.05em',
        color: 'var(--copilot-foreground, #c8ccd4)',
        marginBottom: '6px',
      }}>
        Stations — {String(data.total)}
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
        {data.summary.map((station) => (
          <div key={station.id} style={{
            padding: '6px 8px',
            borderRadius: '6px',
            backgroundColor: 'rgba(255,255,255,0.03)',
            borderLeft: `3px solid ${wearColor(station.avg_wear)}`,
          }}>
            <div style={{
              fontSize: '12px',
              fontWeight: 600,
              marginBottom: '4px',
              color: 'var(--copilot-foreground, #e0e2e8)',
            }}>
              {station.id}
            </div>
            <div style={{
              display: 'grid',
              gridTemplateColumns: '1fr 1fr',
              gap: '2px 12px',
              fontSize: '11px',
              color: 'var(--copilot-foreground, #a0a4b0)',
            }}>
              <span>Modules: {String(station.module_count)}</span>
              <span>Crew: {String(station.crew_total)}</span>
              <span>Power: {station.power_net_kw > 0 ? '+' : ''}{String(station.power_net_kw)} kW</span>
              <span style={{ color: wearColor(station.avg_wear) }}>
                Wear: {formatWear(station.avg_wear)}
              </span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
