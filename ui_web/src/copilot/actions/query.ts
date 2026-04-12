/**
 * Read-only CopilotKit v2 frontend tools: `query_game_state` and
 * `diagnose_alert`.
 *
 * Plan reference: `docs/plans/2026-04-11-001-feat-sim-optimization-variance-plan.md`
 * § Project B, "First actions" subsection (Mb2 scope).
 *
 * Both are pure queries. They never mutate game state and do not gate on
 * pause — the LLM can answer questions about the current (possibly
 * stale) snapshot at any time. Command-executing actions arrive in Mb3
 * with an approval card and a pause gate.
 *
 * The handler closures read from a `useRef` so re-registration on every
 * SSE tick is not required — the latest state is always available
 * without thrashing CopilotKit's tool graph.
 *
 * Pure handler logic lives in `queryHandlers.ts` so vitest can test it
 * without loading the CopilotKit runtime.
 */

import { useFrontendTool } from '@copilotkit/react-core/v2';
import { useRef, useEffect } from 'react';
import { z } from 'zod';

import { DiagnoseAlertRenderer } from '../components/DiagnoseAlertRenderer';
import { QueryResultRenderer } from '../components/QueryResultRenderer';

import { diagnoseAlertById, extractSection, QUERY_SECTIONS } from './queryHandlers';
import type { QueryActionsState } from './queryHandlers';

// Re-export pure helpers so existing import sites keep working.
export {
  diagnoseAlertById,
  extractSection,
  QUERY_SECTIONS,
} from './queryHandlers';
export type { QuerySection, QueryActionsState, DiagnoseAlertResult } from './queryHandlers';

const querySchema = z.object({
  section: z.enum(QUERY_SECTIONS).describe(
    'Which subsection of the game snapshot to return. Use "summary" for the top-level overview.',
  ),
});

const diagnoseSchema = z.object({
  alert_id: z.string().describe('The `alert_id` of an active alert from the top-level readable.'),
});

/**
 * Registers `query_game_state` and `diagnose_alert` as CopilotKit v2
 * frontend tools. Each handler reads from a `useRef` so the LLM always
 * sees the latest snapshot, and the tool definitions are passed with an
 * empty `deps` array so CopilotKit registers each tool exactly ONCE
 * across the component's lifetime — no re-registration thrash on every
 * SSE tick.
 *
 * Previously the tools re-registered on every render, which caused
 * CopilotKit to rebuild its internal tool graph on each tick and
 * produced visible stalls + dropped follow-up responses during
 * multi-turn conversations.
 */
export function useQueryActions(args: QueryActionsState): void {
  const stateRef = useRef(args);
  useEffect(() => {
    stateRef.current = args;
  }, [args]);

  useFrontendTool(
    {
      name: 'query_game_state',
      description:
        'Fetch a subsection of the current game snapshot. Use for questions ' +
        'about stations, ships, research, alerts, treasury, or the fleet. ' +
        'Returns JSON that matches the shape in the top-level readable.',
      parameters: querySchema,
      handler: async ({ section }) => extractSection(section, stateRef.current),
      render: QueryResultRenderer,
    },
    [],
  );

  useFrontendTool(
    {
      name: 'diagnose_alert',
      description:
        'Return details and suggested-action text for a specific active alert. ' +
        'Use when the player asks about a warning or critical notice by ID.',
      parameters: diagnoseSchema,
      handler: async ({ alert_id }) => diagnoseAlertById(alert_id, stateRef.current.activeAlerts),
      render: DiagnoseAlertRenderer,
    },
    [],
  );
}
