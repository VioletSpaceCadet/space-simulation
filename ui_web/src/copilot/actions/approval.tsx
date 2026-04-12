/**
 * Approval-card CopilotKit actions using `useHumanInTheLoop` (v2).
 *
 * Each action renders an ApprovalCard inline in the chat. The LLM
 * proposes an action, the player sees a card with approve/reject
 * buttons, and the response goes back to the LLM. The actual side
 * effect (pause, speed change, command) fires inside the onApprove
 * handler BEFORE calling respond().
 *
 * Card status is derived from CopilotKit's tool status + result
 * string, not from React local state, so it survives re-renders.
 */

import { ToolCallStatus } from '@copilotkit/core';
import { useHumanInTheLoop } from '@copilotkit/react-core/v2';
import { useRef, useEffect } from 'react';
import { z } from 'zod';

import { API_PATHS } from '../../api';
import { IDLE_COLOR } from '../../config/theme';
import { ApprovalCard } from '../components/ApprovalCard';

export interface ApprovalActionsState {
  paused: boolean;
  onTogglePause: () => void;
  onSetSpeed: (tps: number) => void;
}

/** Derive card visual status from CopilotKit tool lifecycle. */
export function deriveCardStatus(
  status: ToolCallStatus,
  result?: string,
): 'pending' | 'approved' | 'rejected' | 'error' {
  if (status !== ToolCallStatus.Complete) { return 'pending'; }
  if (typeof result !== 'string') { return 'pending'; }
  if (result.startsWith('APPROVED')) { return 'approved'; }
  if (result.startsWith('ERROR')) { return 'error'; }
  return 'rejected';
}

const noop = () => { /* placeholder for non-responding states */ };

// ---------------------------------------------------------------------------
// Schemas
// ---------------------------------------------------------------------------

const pauseSchema = z.object({
  action: z.enum(['pause', 'resume']).describe(
    'Whether to pause or resume the simulation.',
  ),
});

const speedSchema = z.object({
  ticks_per_sec: z.number().min(1).max(100000).describe(
    'Target simulation speed in ticks per second. Common values: 100, 1000, 10000, 100000.',
  ),
});

const commandSchema = z.object({
  command_type: z.string().describe('The Command variant name (e.g., "SetModuleEnabled", "Import").'),
  command_json: z.string().describe('The full Command JSON to send to POST /api/v1/command.'),
  summary: z.string().describe('One-line human-readable summary of what the command does.'),
});

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export function useApprovalActions(args: ApprovalActionsState): void {
  const stateRef = useRef(args);
  useEffect(() => {
    stateRef.current = args;
  }, [args]);

  // --- propose_pause ---
  useHumanInTheLoop(
    {
      name: 'propose_pause',
      description:
        'Propose pausing or resuming the simulation. Shows an approval card.',
      parameters: pauseSchema,
      render: (props) => {
        if (props.status === ToolCallStatus.InProgress) {
          return <div style={{ fontSize: '12px', color: IDLE_COLOR }}>Preparing...</div>;
        }

        const action = props.args.action;
        const cardStatus = deriveCardStatus(props.status, props.result);
        const respond = props.status === ToolCallStatus.Executing ? props.respond : null;

        return (
          <ApprovalCard
            title={action === 'pause' ? 'Pause the simulation?' : 'Resume the simulation?'}
            status={cardStatus}
            approveLabel={action === 'pause' ? 'Pause' : 'Resume'}
            onApprove={respond ? () => {
              // Guard: only toggle if state differs from requested action
              const currentlyPaused = stateRef.current.paused;
              const wantPause = action === 'pause';
              if (currentlyPaused !== wantPause) {
                stateRef.current.onTogglePause();
              }
              void respond(`APPROVED: ${action}`);
            } : noop}
            onReject={respond
              ? () => { void respond(`REJECTED: player cancelled ${action}`); }
              : noop}
          />
        );
      },
    },
    [],
  );

  // --- propose_set_speed ---
  useHumanInTheLoop(
    {
      name: 'propose_set_speed',
      description:
        'Propose changing the simulation speed. Shows an approval card.',
      parameters: speedSchema,
      render: (props) => {
        if (props.status === ToolCallStatus.InProgress) {
          return <div style={{ fontSize: '12px', color: IDLE_COLOR }}>Preparing...</div>;
        }

        const tps = props.args.ticks_per_sec;
        const cardStatus = deriveCardStatus(props.status, props.result);
        const respond = props.status === ToolCallStatus.Executing ? props.respond : null;

        return (
          <ApprovalCard
            title={`Change speed to ${String(tps)} ticks/sec?`}
            status={cardStatus}
            approveLabel="Change Speed"
            onApprove={respond ? () => {
              stateRef.current.onSetSpeed(tps);
              void respond(`APPROVED: speed set to ${String(tps)} tps`);
            } : noop}
            onReject={respond
              ? () => { void respond('REJECTED: player cancelled speed change'); }
              : noop}
          />
        );
      },
    },
    [],
  );

  // --- propose_command (generic sim command) ---
  useHumanInTheLoop(
    {
      name: 'propose_command',
      description:
        'Propose executing a simulation command. The command is sent to ' +
        'the daemon on approval. Use for SetModuleEnabled, Import, Export, ' +
        'AssignShipTask, etc. The simulation should be paused first.',
      parameters: commandSchema,
      render: (props) => {
        if (props.status === ToolCallStatus.InProgress) {
          return <div style={{ fontSize: '12px', color: IDLE_COLOR }}>Preparing command...</div>;
        }

        const cardStatus = deriveCardStatus(props.status, props.result);
        const respond = props.status === ToolCallStatus.Executing ? props.respond : null;
        const summary = props.args.summary ?? props.args.command_type ?? 'Execute command';

        return (
          <ApprovalCard
            title={summary}
            status={cardStatus}
            approveLabel="Execute"
            onApprove={respond ? async () => {
              if (!props.args.command_json) {
                void respond('ERROR: missing command data');
                return;
              }
              try {
                const command = JSON.parse(props.args.command_json) as unknown;
                const response = await fetch(API_PATHS.command, {
                  method: 'POST',
                  headers: { 'Content-Type': 'application/json' },
                  body: JSON.stringify({ command }),
                });
                if (!response.ok) {
                  const body = await response.json().catch(() => ({})) as { error?: string };
                  void respond(`ERROR: ${body.error ?? String(response.status)}`);
                  return;
                }
                const result = await response.json() as { command_id: number };
                void respond(`APPROVED: command ${String(result.command_id)} submitted`);
              } catch (err) {
                void respond(`ERROR: ${err instanceof Error ? err.message : 'unknown'}`);
              }
            } : noop}
            onReject={respond
              ? () => { void respond('REJECTED: player cancelled command'); }
              : noop}
          >
            {props.args.command_type && (
              <div style={{ fontSize: '12px', color: 'var(--copilot-foreground, #a0a4b0)' }}>
                Command: {props.args.command_type}
              </div>
            )}
          </ApprovalCard>
        );
      },
    },
    [],
  );
}
