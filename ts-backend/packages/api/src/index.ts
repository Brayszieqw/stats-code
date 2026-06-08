// @stats-code/api — application composition: launcher that wires the server
// (HTTP + orchestration) over the engine, plus the bin entry for the SEA
// artifact. Top of the dependency chain (api → server → engine).
// Depends on: @stats-code/server, @stats-code/engine.

export const API_PACKAGE = '@stats-code/api' as const;

export { runLauncher, defaultState, type LauncherArgs, type RunLauncherOptions } from './launcher.js';
