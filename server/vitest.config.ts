import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['test/**/*.test.ts'],
    // The first PGlite start in each worker compiles WASM; with all files running in parallel that
    // can exceed vitest's 5 s default on Windows and on CI runners.
    testTimeout: 20_000,
    hookTimeout: 30_000,
  },
});
