/// <reference types="vitest/config" />
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: './src/test-setup.ts',
    coverage: {
      provider: 'v8',
      include: ['src/**/*.{ts,tsx}'],
      exclude: ['src/test-setup.ts', 'src/**/*.test.*', 'src/**/*.d.ts'],
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
