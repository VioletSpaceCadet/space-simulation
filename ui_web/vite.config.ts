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
  },
  server: {
    proxy: {
      '/api': {
        target: 'http://localhost:3001',
        configure: (proxy) => {
          proxy.on('error', () => {
            // Silently swallow proxy errors (ECONNREFUSED when backend is down)
          });
        },
      },
    },
  },
});
