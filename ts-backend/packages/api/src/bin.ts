#!/usr/bin/env node
// @stats-code/api bin.ts — the distribution artifact entry (bundled into
// stats-code.exe). Delegates CLI dispatch to the engine's `main`, injecting the
// real launcher runner that starts the HTTP_Server and serves the embedded SPA.

import { spawn } from 'node:child_process';
import { main } from '@stats-code/engine';
import { runLauncher } from './launcher.js';

/** Open a URL in the default browser (Windows/macOS/Linux), outside any guard. */
function openBrowser(url: string): void {
  try {
    if (process.platform === 'win32') {
      spawn('cmd', ['/c', 'start', '', url], { detached: true, stdio: 'ignore' }).unref();
    } else if (process.platform === 'darwin') {
      spawn('open', [url], { detached: true, stdio: 'ignore' }).unref();
    } else {
      spawn('xdg-open', [url], { detached: true, stdio: 'ignore' }).unref();
    }
  } catch {
    /* best-effort; --no-browser or a headless host simply skips this */
  }
}

main(process.argv.slice(1), {
  runLauncher: (args) => runLauncher(args, { openBrowser }),
})
  .then((code) => {
    process.exitCode = code;
  })
  .catch((err: unknown) => {
    process.stderr.write(`fatal: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exitCode = 1;
  });
