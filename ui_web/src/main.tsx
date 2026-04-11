import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import './index.css';
import './App.css';
import App from './App.tsx';
import { CopilotProvider } from './copilot/CopilotProvider';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <CopilotProvider>
      <App />
    </CopilotProvider>
  </StrictMode>,
);
