// launcher/paths.ts — %APPDATA%\stats-code\ path resolution (task 3.6).
// Transcribed from crates/stats-code/src/launcher/paths.rs.
//
//   app_data_dir   → %APPDATA%\stats-code\
//   lock_file_path → %APPDATA%\stats-code\running.lock
//   config_file    → %APPDATA%\stats-code\config.toml
//
// On Windows %APPDATA% = FOLDERID_RoamingAppData. On other platforms we fall
// back to the OS config dir convention so unit tests run cross-platform.

import { homedir } from 'node:os';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';

export const APP_DIR_NAME = 'stats-code';
export const LOCK_FILE_NAME = 'running.lock';
export const CONFIG_FILE_NAME = 'config.toml';

export class AppDataUnavailableError extends Error {
  readonly code = 'APP_DATA_UNAVAILABLE';
  constructor() {
    super('cannot resolve user config directory (%APPDATA% not set)');
    this.name = 'AppDataUnavailableError';
  }
}

/** Resolve the OS user-level config base directory (dirs::config_dir equivalent). */
export function configBaseDir(): string {
  if (process.platform === 'win32') {
    const appData = process.env['APPDATA'];
    if (appData && appData.length > 0) {
      return appData;
    }
    throw new AppDataUnavailableError();
  }
  if (process.platform === 'darwin') {
    return join(homedir(), 'Library', 'Application Support');
  }
  // Linux / other: XDG_CONFIG_HOME or ~/.config
  const xdg = process.env['XDG_CONFIG_HOME'];
  if (xdg && xdg.length > 0) {
    return xdg;
  }
  return join(homedir(), '.config');
}

/** %APPDATA%\stats-code\ — does not create the directory. */
export function appDataDir(): string {
  return join(configBaseDir(), APP_DIR_NAME);
}

/** Compute the stats-code dir under an arbitrary base (test hook). */
export function appDataDirIn(base: string): string {
  return join(base, APP_DIR_NAME);
}

export function lockFilePath(): string {
  return join(appDataDir(), LOCK_FILE_NAME);
}

export function configFilePath(): string {
  return join(appDataDir(), CONFIG_FILE_NAME);
}

/** Ensure %APPDATA%\stats-code\ exists; returns its absolute path. */
export function ensureAppDataDir(): string {
  const dir = appDataDir();
  mkdirSync(dir, { recursive: true });
  return dir;
}
