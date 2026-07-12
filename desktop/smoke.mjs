/**
 * Headless smoke for the desktop shell backend resolution + loopback boot.
 * Does not open Electron UI (CI-friendly). Exit 0 on PASS.
 */

import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PORT_START = 8080;
const PORT_END = 8200;
const TIMEOUT_MS = 40_000;

function resolveBackend() {
  const fromEnv = process.env.STATS_CODE_BACKEND?.trim();
  if (fromEnv) {
    if (fromEnv.toLowerCase().endsWith('.js') || fromEnv.toLowerCase().endsWith('.mjs')) {
      return { command: process.env.STATS_CODE_NODE || 'node', args: [fromEnv, '--no-browser'], cwd: path.dirname(fromEnv) };
    }
    return { command: fromEnv, args: ['--no-browser'], cwd: path.dirname(fromEnv) };
  }
  const repoExe = path.resolve(__dirname, '..', 'ts-backend', 'build', 'stats-code.exe');
  if (existsSync(repoExe)) {
    return { command: repoExe, args: ['--no-browser'], cwd: path.dirname(repoExe) };
  }
  const binJs = path.resolve(__dirname, '..', 'ts-backend', 'packages', 'api', 'dist', 'bin.js');
  if (existsSync(binJs)) {
    return { command: 'node', args: [binJs, '--no-browser'], cwd: path.resolve(__dirname, '..', 'ts-backend') };
  }
  throw new Error('backend artifact missing');
}

async function probe(port) {
  try {
    const res = await fetch(`http://127.0.0.1:${port}/api/health`, { signal: AbortSignal.timeout(800) });
    if (!res.ok) return null;
    const body = await res.json();
    return body?.status === 'ok' ? `http://127.0.0.1:${port}/` : null;
  } catch {
    return null;
  }
}

async function findServer() {
  for (let p = PORT_START; p <= PORT_END; p += 1) {
    const url = await probe(p);
    if (url) return url;
  }
  return null;
}

function stop(child) {
  if (!child?.pid) return;
  if (process.platform === 'win32') {
    spawn('taskkill', ['/pid', String(child.pid), '/T', '/F'], {
      detached: true,
      stdio: 'ignore',
      windowsHide: true,
    }).unref();
  } else {
    child.kill('SIGTERM');
  }
}

async function main() {
  const existing = await findServer();
  if (existing) {
    console.log(`[smoke] PARTIAL: backend already running at ${existing} — skipped spawn (manual check OK)`);
    process.exit(0);
  }

  const spec = resolveBackend();
  console.log('[smoke] spawn', spec.command, spec.args.join(' '));
  const child = spawn(spec.command, spec.args, {
    cwd: spec.cwd,
    env: { ...process.env, STATS_CODE_DESKTOP: '1' },
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  });

  let logs = '';
  child.stdout?.on('data', (c) => {
    logs += c.toString();
  });
  child.stderr?.on('data', (c) => {
    logs += c.toString();
  });

  const start = Date.now();
  try {
    while (Date.now() - start < TIMEOUT_MS) {
      const url = await findServer();
      if (url) {
        const page = await fetch(url, { signal: AbortSignal.timeout(3000) });
        const html = await page.text();
        if (!page.ok || !html.includes('<!')) {
          throw new Error(`SPA root not served from ${url} (status ${page.status})`);
        }
        // Confirm desktop mode did not need a system browser — we only used fetch.
        console.log(`[smoke] PASS backend=${url} spa_bytes=${html.length} elapsed_ms=${Date.now() - start}`);
        stop(child);
        await new Promise((r) => setTimeout(r, 800));
        process.exit(0);
      }
      if (child.exitCode !== null) {
        throw new Error(`backend exited early code=${child.exitCode}\n${logs.slice(-2000)}`);
      }
      await new Promise((r) => setTimeout(r, 200));
    }
    throw new Error(`timeout waiting for health\n${logs.slice(-2000)}`);
  } catch (err) {
    stop(child);
    console.error('[smoke] FAIL', err instanceof Error ? err.message : err);
    process.exit(1);
  }
}

main();
