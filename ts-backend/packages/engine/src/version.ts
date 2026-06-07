// version.ts — single source for the application version string.
//
// At dev time we read the engine package.json. The SEA bundle (task 1.3)
// replaces this with an embedded constant via esbuild `define`, so no file
// system read happens in the packaged .exe.

import { createRequire } from 'node:module';

declare const __STATS_CODE_VERSION__: string | undefined;

function resolveVersion(): string {
  // Embedded by the bundler in the SEA build.
  if (typeof __STATS_CODE_VERSION__ === 'string' && __STATS_CODE_VERSION__.length > 0) {
    return __STATS_CODE_VERSION__;
  }
  try {
    const require = createRequire(import.meta.url);
    const pkg = require('../package.json') as { version?: string };
    if (pkg.version) {
      return pkg.version;
    }
  } catch {
    // fall through
  }
  return '0.0.0';
}

export const VERSION: string = resolveVersion();
