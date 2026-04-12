/**
 * Pure handler logic for the `query_game_state` and `diagnose_alert`
 * CopilotKit actions.
 *
 * Split out from `query.ts` so vitest can unit-test the handlers without
 * loading `@copilotkit/react-core/v2` (whose ESM build has side-effect
 * CSS imports incompatible with the Node test environment). All hook
 * wiring lives in `query.ts`.
 */

import type { ActiveAlert, SimSnapshot } from '../../types';
import { buildSnapshotReadable } from '../snapshotSelector';
import type { SnapshotReadableInput } from '../snapshotSelector';

/** Valid `section` values for `query_game_state`. */
export const QUERY_SECTIONS = [
  'summary',
  'treasury',
  'alerts',
  'research',
  'stations',
  'fleet',
  'asteroids',
] as const;

export type QuerySection = typeof QUERY_SECTIONS[number];

/**
 * Extract a named section from the hierarchical readable. Returns a
 * JSON-safe object the LLM can reason about directly.
 */
export function extractSection(section: QuerySection, args: SnapshotReadableInput): unknown {
  const readable = buildSnapshotReadable(args);
  switch (section) {
    case 'summary':
      return readable;
    case 'treasury':
      return { treasury_usd: readable.treasury_usd };
    case 'alerts':
      return readable.active_alerts;
    case 'research':
      return readable.research;
    case 'stations':
      return readable.stations;
    case 'fleet':
      return readable.fleet;
    case 'asteroids':
      return readable.asteroids;
  }
}

export type DiagnoseAlertResult =
  | { status: 'ok'; alert: ActiveAlert; context: string }
  | { status: 'not_found'; alert_id: string };

/**
 * Diagnose a specific alert by ID. Returns the full alert record plus a
 * short human-readable context string. Returns a `not_found` error
 * object rather than throwing so the LLM can recover on its next turn.
 */
export function diagnoseAlertById(
  alertId: string,
  activeAlerts: Map<string, ActiveAlert>,
): DiagnoseAlertResult {
  const alert = activeAlerts.get(alertId);
  if (!alert) {
    return { status: 'not_found', alert_id: alertId };
  }
  const ageLabel = `first raised at tick ${String(alert.tick)}`;
  const context = `${alert.severity} — ${alert.message}. ${ageLabel}. Suggested action: ${alert.suggested_action}`;
  return { status: 'ok', alert, context };
}

export interface QueryActionsState {
  snapshot: SimSnapshot | null;
  activeAlerts: Map<string, ActiveAlert>;
  currentTick: number;
  paused: boolean;
  connected: boolean;
}
