// Dev backend entry — runs the fully-wired launcher from built source so the
// frontend talks to a real backend (reads the persisted LLM config from
// %APPDATA%\stats-code\llm-config.json). Binds the first free port in 8080-8200.
import { runLauncher } from './packages/api/dist/index.js';

const code = await runLauncher(
  { noBrowser: true },
  { log: (l) => console.log(`[dev-server] ${l}`) },
);
process.exit(code);
