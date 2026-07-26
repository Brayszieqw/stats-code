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
// Statistical sub-commands stay internal (invoked by `core`). `replay` is the
// public, read-only snapshot verification entry and bypasses the launcher.

import { VERSION } from './version.js';
import {
  replaySnapshotArchive,
  type ReplaySnapshotArchiveResult,
} from './snapshot/zip_reader.js';

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
  // Hidden internal entry point.
  'parity',
];

export type Invocation =
  | { mode: 'launcher'; args: LauncherArgs }
  | { mode: 'version' }
  | { mode: 'help' }
  | { mode: 'replay'; args: string[] }
  | { mode: 'subcommand'; name: string }
  | { mode: 'invalid_flag'; flag: string };

/** Usage text. Lists only the three public flags (Requirement 11.3). */
export const USAGE = `stats-code — local statistics workbench

Usage:
  stats-code [options]
  stats-code replay <snapshot.zip> [--sha256 <expected-sha256>]

Options:
  --no-browser    Start the server without opening the default browser
                  (used by the desktop shell / headless hosts).
  --version       Print the version and exit.
  --help          Print this help and exit.

Replay:
  Safely extract and verify every snapshot member without starting the server,
  opening a browser, or creating an application lock. Supply the SHA-256
  returned by export to anchor manifest integrity.

Desktop UI (in-app window, no system browser):
  From the repo:  powershell -File scripts/start-desktop.ps1
  Or:             cd desktop && npm start
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

  const replayIndex = userArgs.indexOf('replay');
  if (replayIndex !== -1) {
    return { mode: 'replay', args: userArgs.slice(replayIndex + 1) };
  }

  for (const arg of userArgs) {
    if (arg.startsWith('-')) {
      continue;
    }
    if (KNOWN_SUBCOMMANDS.includes(arg)) {
      return { mode: 'subcommand', name: arg };
    }
  }

  // Unknown public flags must fail loudly instead of silently starting the
  // server (D8, e.g. `stats-code --replay x.zip`). Runs after subcommand
  // detection so internal subcommands keep their own flag namespaces
  // (`parity --filter …`, `replay --sha256 …`).
  for (const arg of userArgs) {
    if (arg.startsWith('-') && arg !== '--no-browser') {
      return { mode: 'invalid_flag', flag: arg };
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

export type ReplayRun = (args: {
  snapshotPath: string;
  expectedSha256?: string;
}) => ReplaySnapshotArchiveResult | Promise<ReplaySnapshotArchiveResult>;

function parseReplayArgs(args: readonly string[]): { snapshotPath: string; expectedSha256?: string } {
  const snapshotPath = args[0];
  if (!snapshotPath || snapshotPath.startsWith('-')) {
    throw new Error('usage: stats-code replay <snapshot.zip> [--sha256 <expected-sha256>]');
  }
  let expectedSha256: string | undefined;
  for (let index = 1; index < args.length; index += 1) {
    const arg = args[index];
    if (arg !== '--sha256' || expectedSha256 !== undefined || index + 1 >= args.length) {
      throw new Error(`invalid replay argument: ${arg ?? ''}`);
    }
    expectedSha256 = args[index + 1];
    index += 1;
  }
  return { snapshotPath, ...(expectedSha256 ? { expectedSha256 } : {}) };
}

/**
 * Pure-ish entry point: classify argv, handle --version/--help synchronously,
 * and delegate launcher mode to the injected runner. Returns the process exit
 * code (does not call process.exit).
 */
export async function main(
  argv: readonly string[],
  opts: { io?: CliIo; runLauncher?: LauncherRun; runReplay?: ReplayRun } = {},
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
    case 'replay': {
      try {
        const args = parseReplayArgs(invocation.args);
        const result = await (opts.runReplay ?? replaySnapshotArchive)(args);
        io.stdout(
          `Replay PASS: ${result.outcome.stepsReplayed} workflow step(s), archive SHA-256 ${result.archiveSha256}.\n`,
        );
        if (result.archiveAnchored) {
          io.stdout('Archive integrity anchor: verified.\n');
        } else {
          io.stderr('warning: no archive SHA-256 anchor supplied; manifest fields are not externally anchored.\n');
        }
        return 0;
      } catch (error) {
        io.stderr(`Replay FAIL: ${error instanceof Error ? error.message : String(error)}\n`);
        return 2;
      }
    }
    case 'subcommand':
      // Internal subcommands are dispatched by `core`; the public binary does
      // not expose them. Reaching here from the public CLI is a usage error.
      io.stderr(`error: '${invocation.name}' is an internal subcommand and is not publicly available.\n`);
      return 1;
    case 'invalid_flag':
      io.stderr(
        `error: unknown option '${invocation.flag}'\n` +
        'Usage: stats-code [--no-browser | --version | --help]\n' +
        '       stats-code replay <snapshot.zip> [--sha256 <expected-sha256>]\n',
      );
      return 2;
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
