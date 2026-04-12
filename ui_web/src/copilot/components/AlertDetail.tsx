/**
 * Alert detail card rendered inline in the CopilotKit sidebar.
 * Used by both diagnose_alert (single alert) and query_game_state
 * alerts section (summary + recent critical list).
 */

import { SEMANTIC_COLORS } from '../../config/theme';

interface AlertEntry {
  alert_id: string;
  message: string;
  suggested_action: string;
  tick: number;
}

interface AlertsSummaryData {
  total: number;
  warnings: number;
  critical: number;
  recent_critical: AlertEntry[];
}

interface DiagnoseOkResult {
  status: 'ok';
  alert: {
    alert_id: string;
    severity: string;
    message: string;
    suggested_action: string;
    tick: number;
  };
  context: string;
}

interface DiagnoseNotFoundResult {
  status: 'not_found';
  alert_id: string;
}

type DiagnoseResult = DiagnoseOkResult | DiagnoseNotFoundResult;

const SEVERITY_STYLES: Record<string, { bg: string; text: string; label: string }> = {
  Critical: { bg: `${SEMANTIC_COLORS.negative}18`, text: SEMANTIC_COLORS.negative, label: 'CRITICAL' },
  Warning: { bg: `${SEMANTIC_COLORS.warning}18`, text: SEMANTIC_COLORS.warning, label: 'WARNING' },
};

function SeverityBadge({ severity }: { severity: string }) {
  const style = SEVERITY_STYLES[severity] ?? { bg: '#8a8e9818', text: '#8a8e98', label: severity };
  return (
    <span style={{
      display: 'inline-block',
      padding: '1px 6px',
      borderRadius: '3px',
      fontSize: '10px',
      fontWeight: 700,
      letterSpacing: '0.05em',
      backgroundColor: style.bg,
      color: style.text,
    }}>
      {style.label}
    </span>
  );
}

/** Renders a single alert diagnosis card. */
export function AlertDetailCard({ data }: { data: DiagnoseResult }) {
  if (data.status === 'not_found') {
    return (
      <div style={{
        padding: '8px',
        borderRadius: '6px',
        backgroundColor: 'rgba(255,255,255,0.03)',
        fontSize: '12px',
        color: '#8a8e98',
      }}>
        Alert {data.alert_id} not found — it may have been resolved.
      </div>
    );
  }

  return (
    <div style={{
      padding: '8px',
      borderRadius: '6px',
      backgroundColor: 'rgba(255,255,255,0.03)',
      borderLeft: `3px solid ${SEVERITY_STYLES[data.alert.severity]?.text ?? '#8a8e98'}`,
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '4px' }}>
        <SeverityBadge severity={data.alert.severity} />
        <span style={{ fontSize: '11px', color: '#8a8e98' }}>
          tick {String(data.alert.tick)}
        </span>
      </div>
      <div style={{
        fontSize: '12px',
        fontWeight: 500,
        color: 'var(--copilot-foreground, #e0e2e8)',
        marginBottom: '4px',
      }}>
        {data.alert.message}
      </div>
      <div style={{
        fontSize: '11px',
        color: 'var(--copilot-foreground, #a0a4b0)',
        fontStyle: 'italic',
      }}>
        Suggested: {data.alert.suggested_action}
      </div>
    </div>
  );
}

/** Renders the alerts summary (total, warning/critical counts, recent critical list). */
export function AlertsSummaryCard({ data }: { data: AlertsSummaryData }) {
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
        Alerts — {String(data.total)} active
      </div>
      <div style={{
        display: 'flex',
        gap: '12px',
        marginBottom: data.recent_critical.length > 0 ? '8px' : '0',
        fontSize: '12px',
      }}>
        <span style={{ color: SEMANTIC_COLORS.negative }}>
          {String(data.critical)} critical
        </span>
        <span style={{ color: SEMANTIC_COLORS.warning }}>
          {String(data.warnings)} warnings
        </span>
      </div>
      {data.recent_critical.length > 0 && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
          {data.recent_critical.map((alert) => (
            <div key={alert.alert_id} style={{
              padding: '4px 8px',
              borderRadius: '4px',
              backgroundColor: `${SEMANTIC_COLORS.negative}08`,
              borderLeft: `2px solid ${SEMANTIC_COLORS.negative}`,
              fontSize: '11px',
            }}>
              <span style={{ color: 'var(--copilot-foreground, #e0e2e8)' }}>
                {alert.message}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
