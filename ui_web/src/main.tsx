import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import './index.css';
import './App.css';
import App from './App.tsx';
import { ErrorBoundary } from './components/ErrorBoundary';
import { CopilotProvider } from './copilot/CopilotProvider';

// Two-layer error boundary:
// - Outer (this file): last-resort catch for failures in the `<CopilotKit>`
//   provider itself or anything in App.tsx's shell (layout, top-level
//   hooks) that isn't covered by a panel-level boundary. Without this the
//   page goes blank-white on any unhandled render error above the panels.
// - Inner (CopilotMissionBridge): narrower catch around the
//   `<CopilotSidebar>` so a co-pilot crash only kills the chat, not the
//   rest of the app. The inner boundary fires first for realistic
//   CopilotKit-side failures (network, schema, sidebar chrome), leaving
//   App panels untouched.
createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ErrorBoundary panelName="Mission Control">
      <CopilotProvider>
        <App />
      </CopilotProvider>
    </ErrorBoundary>
  </StrictMode>,
);
