// @stats-code/engine — public surface of the engine package.
// The engine maps to the Rust `stats-code` crate: algorithm engine, launcher,
// sidecar, snapshot, parity, coverage matrix, and CLI.

export * as math from './math/index.js';
export * as stats from './stats/index.js';
export * as coverage from './coverage/index.js';
export * as sidecar from './sidecar/index.js';
export * as snapshot from './snapshot/index.js';
export * as parity from './parity/index.js';
export * as launcher from './launcher/index.js';
export { redact } from './redact.js';
export {
  guardedSpawn,
  ForbiddenSpawnError,
  FORBIDDEN_RUNTIMES,
  FORBIDDEN_LIBRARIES,
  checkSpawn,
  checkLibraryLoad,
  matchForbiddenCommand,
  normalizeCommand,
  basename,
  isGuardActive,
} from './spawn_policy.js';
export { ALGORITHM_IDS } from './stats/index.js';
export type { AlgorithmId } from './stats/index.js';
export { main, classifyInvocation, KNOWN_SUBCOMMANDS, USAGE } from './cli.js';
export type { LauncherArgs, Invocation, CliIo } from './cli.js';
export { VERSION } from './version.js';
