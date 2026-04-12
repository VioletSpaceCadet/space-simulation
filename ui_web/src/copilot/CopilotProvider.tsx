/**
 * CopilotKit provider + Mb1 smoke-test sidebar.
 *
 * Wraps the app root with the v2 `<CopilotKit>` context and mounts a
 * `<CopilotSidebar>` so the player can open the chat. A child component runs
 * `useAgentContext` to inject a minimal placeholder readable — enough for the
 * LLM to answer "what tick are we on?" during the Mb1 smoke test. Mb2
 * replaces this stub with the real hierarchical game-state snapshot.
 *
 * Keeps all CopilotKit glue in `ui_web/src/copilot/` so the rest of the app
 * stays unaware of the chat layer.
 */

import { CopilotKit, CopilotSidebar, useAgentContext } from '@copilotkit/react-core/v2';
import '@copilotkit/react-ui/v2/styles.css';
import type { ReactNode } from 'react';

import { ErrorBoundary } from '../components/ErrorBoundary';

const DEFAULT_RUNTIME_URL = 'http://localhost:4000/api/copilotkit';
const RUNTIME_URL = import.meta.env.VITE_COPILOT_RUNTIME_URL ?? DEFAULT_RUNTIME_URL;
const SHARED_SECRET = import.meta.env.VITE_COPILOT_RUNTIME_SECRET;

function buildHeaders(): Record<string, string> | undefined {
  if (typeof SHARED_SECRET === 'string' && SHARED_SECRET.length > 0) {
    return { 'X-Copilot-Runtime-Secret': SHARED_SECRET };
  }
  return undefined;
}

/**
 * Registers the Mb1 stub readable with the agent and renders the sidebar.
 * Runs as a child of `<CopilotKit>` so the `useAgentContext` hook has access
 * to the provider context. Wrapped in an `ErrorBoundary` by the parent so a
 * CopilotKit-side failure (bad network response, schema mismatch, etc.)
 * only takes down the sidebar — the rest of the app stays up.
 */
function StubbedSidebar() {
  useAgentContext({
    description:
      'Mb1 smoke-test placeholder game snapshot. Real hierarchical readable ships in Mb2.',
    value: {
      current_tick: 0,
      provider: 'copilot_runtime sidecar',
      note: 'This is a stub — ask the co-pilot to remind you that Mb1 only wires the round-trip.',
    },
  });
  return (
    <CopilotSidebar
      labels={{
        modalHeaderTitle: 'Mission Co-pilot (Mb1)',
        welcomeMessageText:
          'Mb1 smoke test — ask about the current tick to verify the round-trip.',
      }}
    />
  );
}

export function CopilotProvider({ children }: { children: ReactNode }) {
  const headers = buildHeaders();

  // `children` renders OUTSIDE the ErrorBoundary so a crash in the co-pilot
  // layer (CopilotKit internals, useAgentContext, sidebar chrome) cannot
  // take down the rest of the app. This satisfies the repo's checklist item
  // #15: new top-level components must be wrapped in an ErrorBoundary.
  return (
    <CopilotKit runtimeUrl={RUNTIME_URL} headers={headers}>
      {children}
      <ErrorBoundary panelName="Mission Co-pilot">
        <StubbedSidebar />
      </ErrorBoundary>
    </CopilotKit>
  );
}
