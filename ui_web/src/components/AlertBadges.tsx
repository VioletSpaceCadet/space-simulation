import { useState } from 'react';

import type { ActiveAlert } from '../types';

const THERMAL_ALERT_IDS = new Set(['OVERHEAT_WARNING', 'OVERHEAT_CRITICAL']);

/** Readable short labels for alert IDs that would otherwise be noisy in ALL_CAPS. */
const ALERT_LABELS: Record<string, string> = {
  OVERHEAT_WARNING: 'Overheat Warning',
  OVERHEAT_CRITICAL: 'Overheat Critical',
};

function alertLabel(alertId: string): string {
  return ALERT_LABELS[alertId] ?? alertId.replace(/_/g, ' ');
}

interface Props {
  alerts: Map<string, ActiveAlert>
  dismissed: Set<string>
  onDismiss: (alertId: string) => void
  onNavigateToPanel?: (panelId: string) => void
}

export function AlertBadges({ alerts, dismissed, onDismiss, onNavigateToPanel }: Props) {
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const visible = [...alerts.values()].filter(a => !dismissed.has(a.alert_id));
  if (visible.length === 0) {return null;}

  function handleBadgeClick(alert: ActiveAlert) {
    setExpandedId(expandedId === alert.alert_id ? null : alert.alert_id);
    if (THERMAL_ALERT_IDS.has(alert.alert_id) && onNavigateToPanel) {
      onNavigateToPanel('fleet');
    }
  }

  return (
    <div className="flex gap-1.5 items-center">
      {visible.map((alert) => {
        const isWarning = alert.severity === 'Warning';
        const bgColor = isWarning ? 'bg-amber-500/20' : 'bg-red-500/20';
        const textColor = isWarning ? 'text-amber-400' : 'text-red-400';
        const borderColor = isWarning ? 'border-amber-500/40' : 'border-red-500/40';
        const isExpanded = expandedId === alert.alert_id;

        return (
          <div key={alert.alert_id} className="relative">
            <button
              type="button"
              onClick={() => handleBadgeClick(alert)}
              className={`flex items-center gap-1.5 px-2 py-0.5 rounded border text-[10px] font-medium uppercase tracking-wide ${bgColor} ${textColor} ${borderColor} cursor-pointer hover:brightness-125 transition-all`}
            >
              <span>{alertLabel(alert.alert_id)}</span>
              <span
                role="button"
                tabIndex={0}
                className="ml-1 opacity-60 hover:opacity-100"
                onClick={(e) => { e.stopPropagation(); onDismiss(alert.alert_id); }}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') { e.stopPropagation(); onDismiss(alert.alert_id); }
                }}
              >
                ×
              </span>
            </button>
            {isExpanded && (
              <div className={`absolute top-full right-0 mt-1 z-50 w-72 p-3 rounded border ${borderColor} bg-surface text-xs shadow-lg`}>
                <p className={`font-medium mb-1 ${textColor}`}>{alert.message}</p>
                <p className="text-dim">{alert.suggested_action}</p>
                <p className="text-muted mt-1.5">Since tick {alert.tick}</p>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
