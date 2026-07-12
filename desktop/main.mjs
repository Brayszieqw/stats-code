/**
 * Stats Code desktop shell (Electron).
 *
 * Starts the existing local backend with --no-browser, loads the SPA inside
 * a BrowserWindow (Codex-style in-app UI), and stops the backend on quit.
 * External http(s) links open in the OS browser; the product UI never leaves
 * this window.
 */

import { app, BrowserWindow, Menu, shell, dialog } from 'electron';
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const isPackaged = app.isPackaged;

/** @type {import('node:child_process').ChildProcess | null} */
let backendChild = null;
/** True when this process spawned the backend (so we own its lifecycle). */
let ownsBackend = false;
/** @type {BrowserWindow | null} */
let mainWindow = null;

const PORT_START = 8080;
const PORT_END = 8200;
const STARTUP_TIMEOUT_MS = 45_000;
const POLL_MS = 200;

function log(...args) {
  console.log('[stats-code-desktop]', ...args);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Resolve the backend command. Preference order:
 * 1. STATS_CODE_BACKEND env (full path to exe or "node" script path)
 * 2. Packaged resource: resources/backend/stats-code.exe
 * 3. Repo build: ../ts-backend/build/stats-code.exe
 * 4. Dev node entry: ../ts-backend/packages/api/dist/bin.js via process.execPath's sibling node
 */
function resolveBackend() {
  const fromEnv = process.env.STATS_CODE_BACKEND?.trim();
  if (fromEnv) {
    if (fromEnv.toLowerCase().endsWith('.js') || fromEnv.toLowerCase().endsWith('.mjs')) {
      return { command: process.env.STATS_CODE_NODE || 'node', args: [fromEnv, '--no-browser'], cwd: path.dirname(fromEnv) };
    }
    return { command: fromEnv, args: ['--no-browser'], cwd: path.dirname(fromEnv) };
  }

  if (isPackaged) {
    const packed = path.join(process.resourcesPath, 'backend', 'stats-code.exe');
    if (existsSync(packed)) {
      return { command: packed, args: ['--no-browser'], cwd: path.dirname(packed) };
    }
  }

  const repoExe = path.resolve(__dirname, '..', 'ts-backend', 'build', 'stats-code.exe');
  if (existsSync(repoExe)) {
    return { command: repoExe, args: ['--no-browser'], cwd: path.dirname(repoExe) };
  }

  const binJs = path.resolve(__dirname, '..', 'ts-backend', 'packages', 'api', 'dist', 'bin.js');
  if (existsSync(binJs)) {
    return {
      command: 'node',
      args: [binJs, '--no-browser'],
      cwd: path.resolve(__dirname, '..', 'ts-backend'),
    };
  }

  throw new Error(
    '找不到本地后端。请先构建 ts-backend/build/stats-code.exe，或设置 STATS_CODE_BACKEND。',
  );
}

async function probeHealth(port) {
  const url = `http://127.0.0.1:${port}/api/health`;
  try {
    const res = await fetch(url, { signal: AbortSignal.timeout(800) });
    if (!res.ok) return null;
    const body = await res.json().catch(() => null);
    if (body && body.status === 'ok') {
      return `http://127.0.0.1:${port}/`;
    }
  } catch {
    /* not up yet */
  }
  return null;
}

async function findExistingServer() {
  for (let port = PORT_START; port <= PORT_END; port += 1) {
    const url = await probeHealth(port);
    if (url) return url;
  }
  return null;
}

async function waitForServer(deadlineMs) {
  const start = Date.now();
  while (Date.now() - start < deadlineMs) {
    const url = await findExistingServer();
    if (url) return url;
    await sleep(POLL_MS);
  }
  throw new Error(`后端在 ${deadlineMs}ms 内未响应 /api/health（端口 ${PORT_START}–${PORT_END}）。`);
}

function stopBackend() {
  if (!ownsBackend || !backendChild || backendChild.killed) {
    backendChild = null;
    ownsBackend = false;
    return;
  }
  const child = backendChild;
  backendChild = null;
  ownsBackend = false;
  const pid = child.pid;
  if (!pid) return;

  try {
    if (process.platform === 'win32') {
      spawn('taskkill', ['/pid', String(pid), '/T', '/F'], {
        detached: true,
        stdio: 'ignore',
        windowsHide: true,
      }).unref();
    } else {
      child.kill('SIGTERM');
      setTimeout(() => {
        try {
          child.kill('SIGKILL');
        } catch {
          /* already gone */
        }
      }, 2000).unref?.();
    }
  } catch (err) {
    log('stopBackend failed', err);
  }
}

function startBackend() {
  const spec = resolveBackend();
  log('starting backend', spec.command, spec.args.join(' '));

  backendChild = spawn(spec.command, spec.args, {
    cwd: spec.cwd,
    env: {
      ...process.env,
      // Desktop owns the UI surface; never open the system browser.
      STATS_CODE_DESKTOP: '1',
    },
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  });
  ownsBackend = true;

  backendChild.stdout?.on('data', (chunk) => {
    const text = chunk.toString();
    for (const line of text.split(/\r?\n/)) {
      if (line.trim()) log('backend:', line.trim());
    }
  });
  backendChild.stderr?.on('data', (chunk) => {
    const text = chunk.toString();
    for (const line of text.split(/\r?\n/)) {
      if (line.trim()) log('backend-err:', line.trim());
    }
  });
  backendChild.on('exit', (code, signal) => {
    log('backend exited', { code, signal });
    backendChild = null;
    if (ownsBackend && mainWindow && !mainWindow.isDestroyed()) {
      dialog.showErrorBox('Stats Code', '本地统计引擎已退出，应用将关闭。');
      app.quit();
    }
    ownsBackend = false;
  });
}

function buildWindow() {
  const win = new BrowserWindow({
    width: 1440,
    height: 960,
    minWidth: 1100,
    minHeight: 720,
    title: 'Stats Code',
    backgroundColor: '#f4f0e7',
    autoHideMenuBar: true,
    show: false,
    webPreferences: {
      preload: path.join(__dirname, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      spellcheck: false,
    },
  });

  // Keep navigation inside the loopback product surface.
  win.webContents.on('will-navigate', (event, url) => {
    try {
      const parsed = new URL(url);
      const loopback =
        parsed.hostname === '127.0.0.1' ||
        parsed.hostname === 'localhost' ||
        parsed.hostname === '[::1]';
      if (!loopback) {
        event.preventDefault();
        void shell.openExternal(url);
      }
    } catch {
      event.preventDefault();
    }
  });

  win.webContents.setWindowOpenHandler(({ url }) => {
    try {
      const parsed = new URL(url);
      const loopback =
        parsed.hostname === '127.0.0.1' ||
        parsed.hostname === 'localhost' ||
        parsed.hostname === '[::1]';
      if (loopback) {
        return { action: 'allow' };
      }
    } catch {
      /* fall through */
    }
    void shell.openExternal(url);
    return { action: 'deny' };
  });

  win.once('ready-to-show', () => {
    win.show();
    win.focus();
  });

  win.on('closed', () => {
    mainWindow = null;
  });

  return win;
}

async function bootstrap() {
  // Single Electron instance — second launch focuses the first window.
  const gotLock = app.requestSingleInstanceLock();
  if (!gotLock) {
    app.quit();
    return;
  }
  app.on('second-instance', () => {
    if (mainWindow) {
      if (mainWindow.isMinimized()) mainWindow.restore();
      mainWindow.focus();
    }
  });

  Menu.setApplicationMenu(null);

  await app.whenReady();

  let url = await findExistingServer();
  if (url) {
    log('reusing existing backend at', url);
    ownsBackend = false;
  } else {
    startBackend();
    url = await waitForServer(STARTUP_TIMEOUT_MS);
    log('backend ready at', url);
  }

  mainWindow = buildWindow();
  await mainWindow.loadURL(url);
}

app.on('before-quit', () => {
  stopBackend();
});

app.on('window-all-closed', () => {
  stopBackend();
  app.quit();
});

bootstrap().catch((err) => {
  const message = err instanceof Error ? err.message : String(err);
  console.error('[stats-code-desktop] fatal:', message);
  dialog.showErrorBox('Stats Code 启动失败', message);
  stopBackend();
  app.exit(1);
});
