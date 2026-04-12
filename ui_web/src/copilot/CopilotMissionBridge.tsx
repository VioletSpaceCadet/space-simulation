/**
 * Co-pilot mission bridge — renders inside `<App />` with access to the
 * live SSE state, registers the hierarchical readable + query actions,
 * and mounts the `<CopilotSidebar>` with a dynamic pause/running header.
 *
 * Why this lives inside `<App />` and not alongside `<CopilotKit>` in
 * `CopilotProvider`: the readable and the actions need the output of
 * `useSimStream`, which is called at the top of `<App />`. Keeping this
 * component inside the App subtree means it sees exactly the same state
 * as the rest of the UI, with no second SSE subscription, no prop
 * drilling through the provider, and no shared context to maintain.
 */

import { CopilotSidebar } from '@copilotkit/react-core/v2';

import { ErrorBoundary } from '../components/ErrorBoundary';
import type { ActiveAlert, SimSnapshot } from '../types';

import { useQueryActions } from './actions/query';
import { useSnapshotReadable } from './readables';

interface CopilotMissionBridgeProps {
  snapshot: SimSnapshot | null;
  activeAlerts: Map<string, ActiveAlert>;
  currentTick: number;
  paused: boolean;
  connected: boolean;
}

function buildSidebarTitle(paused: boolean, connected: boolean): string {
  if (!connected) { return 'Mission Co-pilot · disconnected'; }
  return paused ? 'Mission Co-pilot · paused ✓' : 'Mission Co-pilot · running ⟳';
}

function buildWelcomeMessage(paused: boolean): string {
  if (paused) {
    return 'Simulation is paused. Ask me about treasury, fleet, research, alerts, or specific stations.';
  }
  return 'Simulation is running — data will be stale by the time I answer. Pause for live-state questions. I can still summarize recent trends, alerts, and the current state as of the snapshot tick.';
}

/**
 * Empty-disclaimer slot override. CopilotKit ships an "AI can make
 * mistakes" disclaimer under the chat input by default. In a local
 * dev-only mission-control tool the user already knows the model is
 * theirs and the warning is noise — rendering `null` here removes the
 * element without hiding it via CSS workaround.
 */
const NoDisclaimer = () => null;

function MissionBridgeInner(props: CopilotMissionBridgeProps) {
  useSnapshotReadable(props);
  useQueryActions(props);

  const sidebarTitle = buildSidebarTitle(props.paused, props.connected);
  const welcomeMessage = buildWelcomeMessage(props.paused);

  return (
    <CopilotSidebar
      labels={{
        modalHeaderTitle: sidebarTitle,
        welcomeMessageText: welcomeMessage,
      }}
      input={{ disclaimer: NoDisclaimer }}
      throttleMs={250}
    />
  );
}

/**
 * Wraps the inner bridge in an `ErrorBoundary` so a CopilotKit-side
 * render failure (bad tool registration, sidebar chrome crash, schema
 * mismatch after an upstream upgrade) only takes down the sidebar. The
 * App's panels keep rendering unaffected.
 *
 * The outer `<div className="dark">` is load-bearing: CopilotKit's v2
 * CSS uses `:is(.dark *)` selectors to swap light-mode defaults (white
 * input box, white gradient "feather" above the input) for dark-mode
 * equivalents. Without a `.dark` ancestor, the sidebar ships a glaring
 * white chat input and a white gradient band that clash with the rest
 * of the mission control UI. We scope the class to this subtree so the
 * rest of the app is unaffected.
 */
export function CopilotMissionBridge(props: CopilotMissionBridgeProps) {
  return (
    <div className="dark">
      <ErrorBoundary panelName="Mission Co-pilot">
        <MissionBridgeInner {...props} />
      </ErrorBoundary>
    </div>
  );
}
