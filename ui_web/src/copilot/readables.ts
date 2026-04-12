/**
 * Hierarchical game-state readable hook for the CopilotKit agent.
 *
 * Plan reference: `docs/plans/2026-04-11-001-feat-sim-optimization-variance-plan.md`
 * § Project B, "Readable architecture" subsection.
 *
 * The LLM needs a compact, always-fresh summary of the game. A flat JSON
 * blob of the full `SimSnapshot` would blow past any reasonable token
 * budget at realistic late-game state. Instead we build a **hierarchical
 * summary** that fits in a strict ≤4 KB payload at the top level (pure
 * logic in `snapshotSelector.ts`), and expose drill-down detail via
 * frontend tools (`actions/query.ts`) and, later, MCP tool calls (Mb4).
 *
 * This module is the React hook layer: `useSnapshotReadable` memoizes
 * the selector output on `currentTick` and registers it as
 * CopilotKit agent context. Keeping the pure selector in its own file
 * lets vitest run the selector tests without loading the CopilotKit
 * runtime (whose ESM build has side-effect CSS imports that break
 * node-only test environments).
 *
 * Memoization: the readable object is rebuilt only when `currentTick`
 * changes (not on every parent re-render, not on snapshot reference
 * identity). This matches the plan's "memoize on tick" rule and keeps
 * `useAgentContext` from thrashing the CopilotKit context graph.
 *
 * Availability: always enabled. Command-executing actions gate on pause
 * (Mb3); query actions and the readable itself stay live at all times
 * because the LLM should always be able to answer questions — it just
 * cites a stale `snapshot_tick` when the sim is running.
 */

import { useAgentContext } from '@copilotkit/react-core/v2';
import type { JsonSerializable } from '@copilotkit/react-core/v2';
import { useDeferredValue, useMemo } from 'react';

import { buildSnapshotReadable } from './snapshotSelector';
import type { SnapshotReadableInput } from './snapshotSelector';

// Re-export the types + selector so existing import sites keep working.
export { buildSnapshotReadable } from './snapshotSelector';
export type { SnapshotReadable, SnapshotReadableInput } from './snapshotSelector';

/**
 * Hook that registers the snapshot as agent context with CopilotKit v2.
 *
 * Memoized on `currentTick` only — intentionally NOT on `snapshot` object
 * identity, because the SSE stream produces a new snapshot reference
 * every tick even when the underlying data is unchanged. Tick is the one
 * logical monotonic clock the sim guarantees.
 *
 * The trade-off: if SSE events arrive BETWEEN ticks that change state
 * without bumping the tick (unusual but possible for alerts), the
 * readable momentarily lags by one tick. Acceptable for Mb2 smoke test;
 * revisit if the LLM mis-cites on alert-heavy turns.
 */
export function useSnapshotReadable(args: SnapshotReadableInput): void {
  const { snapshot, activeAlerts, currentTick, paused, connected } = args;

  const readable = useMemo(
    () => buildSnapshotReadable({ snapshot, activeAlerts, currentTick, paused, connected }),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- see function docstring
    [currentTick, paused, connected],
  );

  // `useAgentContext` internally re-registers agent context whenever the
  // JSON-stringified value changes, and CopilotKit's message-history
  // accounting gets confused when context churns mid-conversation: the
  // visible symptom is messages landing out of order (Q1-A1-A2-Q2
  // instead of Q1-A1-Q2-A2) and dropped follow-up responses.
  //
  // `useDeferredValue` tells React to treat the readable update as a
  // low-priority transition: if the user is actively typing or the chat
  // is rendering a response, React holds back the new readable and
  // keeps the previous one in place until the UI is idle. During a sim
  // ticking at 10 Hz this effectively throttles context updates to
  // user-perceivable moments instead of every SSE frame.
  const deferredReadable = useDeferredValue(readable);

  useAgentContext({
    description:
      'Current game state summary. Always include the `snapshot_tick` when ' +
      'answering state questions. When `paused` is false, recommend pausing ' +
      'before proposing commands. Call `query_game_state` for full station, ' +
      'ship, or research detail beyond this summary.',
    // CopilotKit's `JsonSerializable` requires an index signature that
    // our strongly-typed `SnapshotReadable` interface does not declare.
    // All fields of `SnapshotReadable` are transitively JSON-safe, so the
    // cast is sound at runtime — we accept the type assertion at the
    // boundary rather than weakening the interface's type safety.
    value: deferredReadable as unknown as JsonSerializable,
  });
}
