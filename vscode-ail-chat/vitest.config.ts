import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // Extension host tests: test/**.test.ts (no webview path)
    // Webview tests:        test/webview/**.test.tsx
    // We rely on per-file environment comments for per-file overrides,
    // but set default to node since most tests are extension-host side.
    environment: 'node',
    environmentMatchGlobs: [
      ['test/webview/**', 'jsdom'],
    ],
    globals: false,
    include: ['test/**/*.test.{ts,tsx}'],
  },
  esbuild: {
    jsx: 'automatic',
  },
});
