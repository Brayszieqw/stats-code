import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['tests/**/*.{test,spec}.ts', 'packages/**/*.{test,spec}.ts'],
    environment: 'node',
    globals: false,
    // Vitest 3's fork pool can hit its fixed 60s worker-RPC deadline after all
    // tests pass on Windows. Threads preserve per-file isolation without the
    // flaky cross-process onTaskUpdate channel.
    pool: 'threads',
    // Property/replay suites shell out to Expand-Archive and routinely exceed
    // vitest 3's default 5s when the full suite is under load. Under external
    // machine load (another suite running concurrently) the first PowerShell
    // cold start per worker has been measured at 24-28s, leaving 30s only a
    // 2-6s margin — 60s keeps the hang guard without that flake.
    testTimeout: 60_000,
    // Covers npm scripts, `npx vitest`, and IDE test runners that load this config.
    globalSetup: ['./scripts/vitest-global-setup.mjs'],
  },
});
