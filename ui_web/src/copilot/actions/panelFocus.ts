/**
 * `focus_panel` CopilotKit frontend tool — opens and highlights a
 * panel in the mission control UI when the LLM discusses a relevant
 * topic.
 *
 * Uses the same `ensurePanelVisible` mechanism as the sidebar nav
 * and alert badge clicks. The highlight animation (a brief accent
 * glow via CSS) is handled by App.tsx setting a `highlightPanel`
 * state that auto-clears after the animation duration.
 */

import { useFrontendTool } from '@copilotkit/react-core/v2';
import { useRef, useEffect } from 'react';
import { z } from 'zod';

import type { PanelId } from '../../layout';

/** Panel IDs exposed to the LLM. Matches PanelId type in layout.ts. */
const PANEL_IDS = [
  'map',
  'events',
  'asteroids',
  'fleet',
  'research',
  'economy',
  'manufacturing',
] as const;

const panelFocusSchema = z.object({
  panel: z.enum(PANEL_IDS).describe(
    'Which panel to open and highlight in the mission control UI.',
  ),
});

export interface PanelFocusState {
  onFocusPanel: (panelId: PanelId) => void;
}

/**
 * Registers the `focus_panel` frontend tool. The handler calls the
 * `onFocusPanel` callback (wired to App's `ensurePanelVisible` +
 * highlight logic) and returns a confirmation string for the LLM.
 */
export function usePanelFocus(state: PanelFocusState): void {
  const stateRef = useRef(state);
  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  useFrontendTool(
    {
      name: 'focus_panel',
      description:
        'Open and highlight a panel in the mission control UI. Call this ' +
        'when you mention fleet, stations, research, economy, or other ' +
        'panels so the player can see the relevant data alongside your ' +
        'answer. The panel opens if not already visible.',
      parameters: panelFocusSchema,
      handler: async ({ panel }) => {
        stateRef.current.onFocusPanel(panel as PanelId);
        return { status: 'focused', panel };
      },
    },
    [],
  );
}
