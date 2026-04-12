/// <reference types="vitest/config" />
import { execFileSync } from 'node:child_process';

import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

/**
 * Populate `VITE_COPILOT_RUNTIME_SECRET` from macOS Keychain if the env var is
 * unset. Vite will inject the resulting `process.env.VITE_*` value into the
 * client bundle, so the CopilotKit provider can forward it as the shared-secret
 * header without the developer manually exporting it every shell session.
 *
 * macOS-only by design (plan decision 13). On Linux/Windows the `security`
 * binary does not exist; `execFileSync` throws ENOENT, we catch it, and the
 * UI still boots — the chat will 401 until the secret is provided some other
 * way. See `copilot_runtime/README.md` for setup.
 */
function hydrateCopilotSecretFromKeychain(): void {
  if (process.env.VITE_COPILOT_RUNTIME_SECRET && process.env.VITE_COPILOT_RUNTIME_SECRET.length > 0) {
    return;
  }
  try {
    const secret = execFileSync(
      'security',
      [
        'find-generic-password',
        '-a',
        'copilot_runtime',
        '-s',
        'COPILOT_RUNTIME_SECRET',
        '-w',
      ],
      { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] },
    ).trim();
    if (secret.length > 0) {
      process.env.VITE_COPILOT_RUNTIME_SECRET = secret;
    }
  } catch {
    // Keychain entry missing or `security` unavailable — warn once so the dev
    // knows why the chat will 401, but do not fail the build. `console.warn`
    // is allowlisted by the repo eslint config.
    console.warn(
      '[vite] VITE_COPILOT_RUNTIME_SECRET not set and Keychain entry missing. ' +
      'The CopilotKit sidebar will 401 until you run the setup in copilot_runtime/README.md.',
    );
  }
}

hydrateCopilotSecretFromKeychain();

export default defineConfig({
  plugins: [react(), tailwindcss()],
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: './src/test-setup.ts',
    coverage: {
      provider: 'v8',
      include: ['src/**/*.{ts,tsx}'],
      exclude: [
        'src/test-setup.ts',
        'src/**/*.test.*',
        'src/**/*.d.ts',
        // Canvas draw code uses CanvasRenderingContext2D which jsdom lacks — tested via Chrome agent
        'src/components/solar-system/canvas/renderer.ts',
        'src/components/solar-system/canvas/starfield.ts',
      ],
      thresholds: {
        lines: 58,
        branches: 47,
        functions: 63,
        statements: 61,
      },
    },
  },
  server: {
    proxy: {
      '/api': {
        target: process.env.VITE_API_TARGET ?? 'http://localhost:3001',
        configure: (proxy) => {
          proxy.on('error', () => {
            // Silently swallow proxy errors (ECONNREFUSED when backend is down)
          });
        },
      },
    },
  },
});
