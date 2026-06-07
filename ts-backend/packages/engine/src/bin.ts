#!/usr/bin/env node
// bin.ts — the actual executable entry. Bundled into stats-code.exe (task 1.3).

import { main } from './cli.js';

main(process.argv.slice(1))
  .then((code) => {
    process.exitCode = code;
  })
  .catch((err: unknown) => {
    process.stderr.write(`fatal: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exitCode = 1;
  });
