// cli.ts — command-line entry point + flag parsing (Phase 0, task 1.2).
//
// Mirrors the Rust launcher CLI contract (crates/stats-code/src/main.rs +
// launcher/args.rs + launcher/mod.rs::classify_invocation):
//
//   stats-code                 → start app, open browser
//   stats-code --no-browser    → start app, do not open browser
//   stats-code --version       → print version, exit 0
//   stats-code --help          → print usage (lists --no-browser/--version/--help), exit 0
//
// Statistical sub-commands stay internal (invoked by `core`), and `parity` /
// `replay` are hidden internal entry points that bypass the launcher. None of
// these are advertised in --help (Requirement 11.5).

import { VERSION } from './version.js';

/** Public launcher flags only (Requirement 11.2-11.4). */
export interface LauncherArgs {
  noBrowser: boolean;
}

/** Internal subcommands; recognized but never advertised in --help. */
export const KNOWN_SUBCOMMANDS: readonly string[] = [
  'config',
  'init',
  'doctor',
  'plan',
  'check',
  'inspect',
  'tableone',
  'rate',
  'power',
  'diagnostic',
  'survival',
  'auth',
  'ai',
  'audit',
  'model',
  'report',
  'open',
  'workflow',
  'run',
  'stats',
  // Hidden internal entry points (bypass the launcher: no port, no browser, no lock).
  'parity',
  'replay',
];

export type Invocation =
  | { mode: 'launcher'; args: LauncherArgs }
  | { mode: 'version' }
  | { mode: 'help' }
  | { mode: 'subcommand'; name: string };

/** Usage text. Lists only the three public flags (Requirement 11.3). */
export const USAGE = `stats-code — local statistics workbench

Usage:
  stats-code [options]

Options:
  --no-browser    Start the server without opening the default browser.
  --version       Print the version and exit.
  --help          Print this help and exit.
`;

/**
 * Classify a raw argv (argv[0] = program name) into an invocation.
 *
 * Precedence mirrors the Rust behavior:
 *  - `--help` / `--version` short-circuit (clap auto-dispatches these);
 *  - any non-flag token matching a known subcommand → subcommand mode;
 *  - otherwise launcher mode, collecting public flags.
 */
export function classifyInvocation(argv: readonly string[]): Invocation {
  const userArgs = argv.slice(1);

  // --help / --version take precedence regardless of position.
  if (userArgs.includes('--help') || userArgs.includes('-h')) {
    return { mode: 'help' };
  }
  if (userArgs.includes('--version') || userArgs.includes('-V')) {
    return { mode: 'version' };
  }

  for (const arg of userArgs) {
    if (arg.startsWith('-')) {
      continue;
    }
    if (KNOWN_SUBCOMMANDS.includes(arg)) {
      return { mode: 'subcommand', name: arg };
    }
  }

  return { mode: 'launcher', args: { noBrowser: userArgs.includes('--no-browser') } };
}

export interface CliIo {
  stdout: (line: string) => void;
  stderr: (line: string) => void;
}

const defaultIo: CliIo = {
  stdout: (line) => process.stdout.write(line),
  stderr: (line) => process.stderr.write(line),
};

/**
 * Hook the launcher actually runs the app. Wired in Phase 1 (tasks 3.6/3.7).
 * Kept injectable so Phase 0 can test the CLI dispatch without a server.
 */
export type LauncherRun = (args: LauncherArgs) => Promise<number>;

/**
 * Pure-ish entry point: classify argv, handle --version/--help synchronously,
 * and delegate launcher mode to the injected runner. Returns the process exit
 * code (does not call process.exit).
 */
export async function main(
  argv: readonly string[],
  opts: { io?: CliIo; runLauncher?: LauncherRun } = {},
): Promise<number> {
  const io = opts.io ?? defaultIo;
  const invocation = classifyInvocation(argv);

  switch (invocation.mode) {
    case 'version':
      io.stdout(`${VERSION}\n`);
      return 0;
    case 'help':
      io.stdout(USAGE);
      return 0;
    case 'subcommand':
      // Internal subcommands are dispatched by `core`; the public binary does
      // not expose them. Reaching here from the public CLI is a usage error.
      io.stderr(`error: '${invocation.name}' is an internal subcommand and is not publicly available.\n`);
      return 1;
    case 'launcher': {
      if (!opts.runLauncher) {
        // Launcher implementation lands in Phase 1; until then this is a no-op
        // success so the Phase 0 scaffold runs end-to-end.
        return 0;
      }
      return opts.runLauncher(invocation.args);
    }
  }
}
