// vitest globalSetup — runs for every vitest entrypoint (npm test, npx vitest, IDE).
// Rebuilds packages/*/dist when missing or stale so @stats-code/* always resolves.

import { ensureBuilt } from './ensure-built.mjs';

export default async function setup() {
  const code = ensureBuilt();
  if (code !== 0) {
    throw new Error(`[vitest-global-setup] ensure-built failed with exit ${code}`);
  }
}
