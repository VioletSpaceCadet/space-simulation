/* eslint-disable react-refresh/only-export-components -- inline renders need stateRef closure */
/**
 * Read-only CopilotKit v2 frontend tools: `query_game_state` and
 * `diagnose_alert`.
 *
 * The handler returns a SHORT acknowledgment string — NOT the full
 * data. This prevents the LLM from narrating what the visual card
 * already shows. The render component reads state directly from the
 * stateRef closure instead of parsing `result`.
 *
 * Pure handler logic lives in `queryHandlers.ts` so vitest can test it
 * without loading the CopilotKit runtime.
 */

import { ToolCallStatus } from '@copilotkit/core';
import { useFrontendTool } from '@copilotkit/react-core/v2';
import { useRef, useEffect } from 'react';
import { z } from 'zod';

import { IDLE_COLOR, SEMANTIC_COLORS } from '../../config/theme';
import { AlertDetailCard, AlertsSummaryCard } from '../components/AlertDetail';
import { FleetTable } from '../components/FleetTable';
import { StationSummary } from '../components/StationSummary';

import { diagnoseAlertById, extractSection, QUERY_SECTIONS } from './queryHandlers';
import type { QueryActionsState, QuerySection } from './queryHandlers';

const querySchema = z.object({
  section: z.enum(QUERY_SECTIONS).describe(
    'Which subsection of the game snapshot to return. Use "summary" for the top-level overview.',
  ),
});

const diagnoseSchema = z.object({
  alert_id: z.string().describe('The `alert_id` of an active alert from the top-level readable.'),
});

/** Loading spinner shown during InProgress/Executing states. */
function LoadingIndicator({ label }: { label: string }) {
  return (
    <div style={{
      padding: '8px',
      fontSize: '12px',
      color: IDLE_COLOR,
      display: 'flex',
      alignItems: 'center',
      gap: '6px',
    }}>
      <span style={{
        display: 'inline-block',
        width: '12px',
        height: '12px',
        border: '2px solid ${IDLE_COLOR}40',
        borderTopColor: '#8a8e98',
        borderRadius: '50%',
        animation: 'cpk-spin 0.8s linear infinite',
      }} />
      {label}
    </div>
  );
}

/** Renders key-value pairs for sections without a dedicated component. */
function GenericResult({ section, data }: { section: string; data: unknown }) {
  if (typeof data !== 'object' || data === null) {
    return <div style={{ fontSize: '12px' }}>{String(data)}</div>;
  }

  const entries = Object.entries(data as Record<string, unknown>);
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
        {section}
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: '2px' }}>
        {entries.map(([key, value]) => (
          <div key={key} style={{
            display: 'flex',
            justifyContent: 'space-between',
            padding: '2px 8px',
            fontSize: '12px',
            borderRadius: '3px',
            backgroundColor: 'rgba(255,255,255,0.03)',
          }}>
            <span style={{ color: '#8a8e98' }}>{key.replace(/_/g, ' ')}</span>
            <span style={{ fontWeight: 500, fontVariantNumeric: 'tabular-nums' }}>
              {typeof value === 'object' ? JSON.stringify(value) : String(value)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

/**
 * Registers `query_game_state` and `diagnose_alert` as CopilotKit v2
 * frontend tools.
 *
 * Key design: the render components read state directly from stateRef
 * (closure capture), NOT from `result`. The handler returns a short
 * acknowledgment so the LLM has nothing to narrate — the visual card
 * IS the answer.
 */
export function useQueryActions(args: QueryActionsState): void {
  const stateRef = useRef(args);
  useEffect(() => {
    stateRef.current = args;
  }, [args]);

  // --- query_game_state ---
  // Render reads state directly; handler returns acknowledgment only.
  function QueryRenderer(props: {
    args: { section?: QuerySection };
    status: ToolCallStatus;
  }) {
    if (props.status !== ToolCallStatus.Complete) {
      return <LoadingIndicator label={`Querying ${props.args.section ?? 'game state'}...`} />;
    }

    const section = props.args.section;
    if (!section) {
      return <div style={{ fontSize: '12px', color: SEMANTIC_COLORS.negative }}>Missing section.</div>;
    }

    const data = extractSection(section, stateRef.current);
    switch (section) {
      case 'fleet':
        return <FleetTable data={data as Parameters<typeof FleetTable>[0]['data']} />;
      case 'stations':
        return <StationSummary data={data as Parameters<typeof StationSummary>[0]['data']} />;
      case 'alerts':
        return <AlertsSummaryCard data={data as Parameters<typeof AlertsSummaryCard>[0]['data']} />;
      default:
        return <GenericResult section={section} data={data} />;
    }
  }

  useFrontendTool(
    {
      name: 'query_game_state',
      description:
        'Show a visual card for a game snapshot section. The card is rendered ' +
        'inline in the chat — do NOT describe its contents in text. Sections: ' +
        'stations, fleet, alerts, treasury, research, asteroids, summary.',
      parameters: querySchema,
      handler: async ({ section }) => `[${section} card displayed]`,
      render: QueryRenderer,
    },
    [],
  );

  // --- diagnose_alert ---
  function DiagnoseRenderer(props: {
    args: { alert_id?: string };
    status: ToolCallStatus;
  }) {
    if (props.status !== ToolCallStatus.Complete) {
      return <LoadingIndicator label={`Diagnosing alert ${props.args.alert_id ?? ''}...`} />;
    }

    const alertId = props.args.alert_id;
    if (!alertId) {
      return <div style={{ fontSize: '12px', color: SEMANTIC_COLORS.negative }}>Missing alert ID.</div>;
    }

    const data = diagnoseAlertById(alertId, stateRef.current.activeAlerts);
    return <AlertDetailCard data={data} />;
  }

  useFrontendTool(
    {
      name: 'diagnose_alert',
      description:
        'Show a visual card diagnosing a specific alert. The card is rendered ' +
        'inline — do NOT describe its contents in text.',
      parameters: diagnoseSchema,
      handler: async ({ alert_id }) => {
        const result = diagnoseAlertById(alert_id, stateRef.current.activeAlerts);
        return result.status === 'ok'
          ? `[alert card displayed: ${result.alert.severity}]`
          : `[alert ${alert_id} not found]`;
      },
      render: DiagnoseRenderer,
    },
    [],
  );
}
