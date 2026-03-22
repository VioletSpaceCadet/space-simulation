import '@testing-library/jest-dom';

// jsdom lacks ResizeObserver — provide a minimal stub for canvas map tests
if (typeof globalThis.ResizeObserver === 'undefined') {
  globalThis.ResizeObserver = class ResizeObserver {
    observe() { /* noop */ }
    unobserve() { /* noop */ }
    disconnect() { /* noop */ }
  };
}
