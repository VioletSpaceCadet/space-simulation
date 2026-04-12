/**
 * CopilotKit v2 runtime provider.
 *
 * Wraps the app root with `<CopilotKit>` so everything inside — including
 * `<App />` — can register agent context, frontend tools, and render the
 * sidebar. The sidebar itself lives inside `<App />` via
 * `CopilotMissionBridge`, because it needs access to the SSE-driven game
 * state that `<App />` owns.
 *
 * Keeps all CopilotKit glue in `ui_web/src/copilot/` so the rest of the
 * app stays unaware of the chat layer.
 */

import { CopilotKit } from '@copilotkit/react-core/v2';
import '@copilotkit/react-ui/v2/styles.css';
import './copilot-theme.css';
import { useMemo } from 'react';
import type { ReactNode } from 'react';

const DEFAULT_RUNTIME_URL = 'http://localhost:4000/api/copilotkit';
const RUNTIME_URL = import.meta.env.VITE_COPILOT_RUNTIME_URL ?? DEFAULT_RUNTIME_URL;
const SHARED_SECRET = import.meta.env.VITE_COPILOT_RUNTIME_SECRET;
const HAS_SHARED_SECRET = typeof SHARED_SECRET === 'string' && SHARED_SECRET.length > 0;

// Warn loudly at module load when the shared secret is missing. The Vite
// config already tries to hydrate it from Keychain at dev-server startup;
// this is the browser-side safety net when that path fails (e.g. Linux
// dev machine, missing Keychain entry, someone running a production build
// locally without the env var wired up). The sidebar will 401 on every
// request until the secret is supplied, and the console.warn makes the
// failure mode obvious instead of silent.
if (!HAS_SHARED_SECRET) {
  // `console.warn` is allowlisted by the repo eslint config.
  console.warn(
    '[CopilotProvider] VITE_COPILOT_RUNTIME_SECRET is not set. ' +
    'The chat sidebar will 401 on every request until you populate the ' +
    'macOS Keychain entry (see copilot_runtime/README.md).',
  );
}

export function CopilotProvider({ children }: { children: ReactNode }) {
  // Memoize the headers so `<CopilotKit>` does not see a new object on
  // every parent re-render — avoids triggering CopilotKit's fetch/context
  // equality checks unnecessarily. Recomputed only when the module-level
  // constants change (never, in practice, for a given dev session).
  const headers = useMemo<Record<string, string> | undefined>(() => {
    if (typeof SHARED_SECRET === 'string' && SHARED_SECRET.length > 0) {
      return { 'X-Copilot-Runtime-Secret': SHARED_SECRET };
    }
    return undefined;
  }, []);

  // Force multi-route (REST) transport. The default is "auto", which
  // probes `GET basePath/info` and falls back to single-route if the
  // probe fails. In practice, we observed the auto-detect landing on
  // single-route mode on fresh mounts, and single-route mode coalesces
  // multiple agent turns into a single assistant bubble (Mb2 manual
  // test bug). Forcing `useSingleEndpoint={false}` makes the client use
  // the multi-route `POST basePath/agent/{id}/run` transport directly;
  // if that fails the chat will surface a clear network error instead
  // of silently degrading.
  return (
    <CopilotKit runtimeUrl={RUNTIME_URL} headers={headers} useSingleEndpoint={false} showDevConsole={false}>
      {children}
    </CopilotKit>
  );
}
