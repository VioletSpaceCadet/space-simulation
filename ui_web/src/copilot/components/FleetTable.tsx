/**
 * Fleet task breakdown table rendered inline in the CopilotKit sidebar.
 * Shows ship counts by task kind with colored dot indicators from
 * config/theme.ts.
 */

import { IDLE_COLOR, SHIP_TASK_COLORS } from '../../config/theme';

interface FleetData {
  total: number;
  in_transit: number;
  mining: number;
  idle: number;
  other: number;
}

const FLEET_ROWS: Array<{ label: string; key: keyof FleetData; color: string }> = [
  { label: 'Idle', key: 'idle', color: IDLE_COLOR },
  { label: 'In Transit', key: 'in_transit', color: SHIP_TASK_COLORS.Transit },
  { label: 'Mining', key: 'mining', color: SHIP_TASK_COLORS.Mine },
  { label: 'Other', key: 'other', color: SHIP_TASK_COLORS.Deposit },
];

export function FleetTable({ data }: { data: FleetData }) {
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
        Fleet — {String(data.total)} ships
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: '3px' }}>
        {FLEET_ROWS.map((row) => (
          <div key={row.key} style={{
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            padding: '3px 8px',
            borderRadius: '4px',
            backgroundColor: 'rgba(255,255,255,0.03)',
          }}>
            <span style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '12px' }}>
              <span style={{
                width: '8px',
                height: '8px',
                borderRadius: '50%',
                backgroundColor: row.color,
                display: 'inline-block',
              }} />
              {row.label}
            </span>
            <span style={{
              fontSize: '13px',
              fontWeight: 600,
              fontVariantNumeric: 'tabular-nums',
            }}>
              {String(data[row.key])}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
