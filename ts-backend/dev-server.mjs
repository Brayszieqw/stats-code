// Dev backend entry — runs the fully-wired launcher from built source so the
// frontend talks to a real backend (reads the persisted LLM config from
// %APPDATA%\stats-code\llm-config.json). Binds to 8080 by default so Vite's
// dev proxy always targets the same backend.
import { runLauncher } from './packages/api/dist/index.js';

const rawPort = process.env.STATS_CODE_DEV_PORT ?? process.env.PORT ?? '8080';
const port = Number.parseInt(rawPort, 10);

if (!Number.isInteger(port) || port <= 0 || port > 65535) {
  throw new Error(`invalid dev server port: ${rawPort}`);
}

const code = await runLauncher(
  { noBrowser: true },
  {
    log: (l) => console.log(`[dev-server] ${l}`),
    portRange: { start: port, endExclusive: port + 1 },
  },
);
process.exit(code);
