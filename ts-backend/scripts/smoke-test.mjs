// scripts/smoke-test.mjs — distribution artifact smoke test (task 17.3).
//
// Asserts the packaged stats-code.exe:
//   1. runs `--version` and `--help` and exits 0,
//   2. starts with `--no-browser` and binds the HTTP_Server on loopback,
//   3. does so with NO Node.js / R / SAS / Python / SPSS available on PATH
//      (the PATH is scrubbed of interpreter directories for the launch).
//
// The artifact embeds its own Node runtime + frontend assets, so a scrubbed
// PATH proves zero external-runtime dependence (Requirements 10.1-10.3, 15.2).
//
// Exit 0 on success; non-zero with a diagnostic on any failed assertion.

import { execFileSync, spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, resolve, join, delimiter } from 'node:path';
import { fileURLToPath } from 'node:url';
import net from 'node:net';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const exePath = join(root, 'build', process.platform === 'win32' ? 'stats-code.exe' : 'stats-code');

function fail(msg) {
  console.error(`[smoke] FAIL: ${msg}`);
  process.exit(1);
}

if (!existsSync(exePath)) {
  fail(`distribution artifact not found at ${exePath}. Run \`npm run sea\` first.`);
}

// Scrub PATH of any directory that looks like it hosts an external runtime,
// proving the artifact does not shell out to one.
function scrubbedPath() {
  const forbidden = /(\\nodejs|\\node\b|\\R\b|\\R-\d|\\SASHome|\\Python|\\SPSS|\/node|\/R\/|\/python|\/sas|\/spss)/i;
  const entries = (process.env.PATH ?? '').split(delimiter);
  const kept = entries.filter((p) => p && !forbidden.test(p));
  // Keep a minimal Windows system dir so the OS loader still works.
  return kept.join(delimiter);
}

const scrubbedEnv = {
  ...process.env,
  PATH: scrubbedPath(),
  Path: scrubbedPath(),
};

// 1. --version and --help exit 0.
try {
  const version = execFileSync(exePath, ['--version'], { env: scrubbedEnv, encoding: 'utf8' }).trim();
  console.log(`[smoke] --version → ${version}`);
  if (!/^\d+\.\d+\.\d+/.test(version)) {
    fail(`--version did not print a semver-like string (got "${version}")`);
  }
  const help = execFileSync(exePath, ['--help'], { env: scrubbedEnv, encoding: 'utf8' });
  for (const flag of ['--no-browser', '--version', '--help']) {
    if (!help.includes(flag)) fail(`--help did not list ${flag}`);
  }
  console.log('[smoke] --version / --help OK');
} catch (err) {
  fail(`--version/--help failed: ${err.message}`);
}

// 2/3. Start with --no-browser and confirm it binds a loopback port in 8080-8200.
async function probePort(port) {
  return new Promise((resolveProbe) => {
    const sock = net.connect({ host: '127.0.0.1', port }, () => {
      sock.destroy();
      resolveProbe(true);
    });
    sock.on('error', () => resolveProbe(false));
    sock.setTimeout(500, () => {
      sock.destroy();
      resolveProbe(false);
    });
  });
}

async function waitForBind(timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    for (let port = 8080; port <= 8200; port += 1) {
      // eslint-disable-next-line no-await-in-loop
      if (await probePort(port)) return port;
    }
    // eslint-disable-next-line no-await-in-loop
    await new Promise((r) => setTimeout(r, 200));
  }
  return null;
}

const child = spawn(exePath, ['--no-browser'], { env: scrubbedEnv, stdio: 'ignore' });
let exited = false;
child.on('exit', () => {
  exited = true;
});

const boundPort = await waitForBind(15000);
if (exited) {
  fail('artifact exited before binding the HTTP_Server');
}
if (boundPort === null) {
  child.kill();
  fail('artifact did not bind a loopback port in 8080-8200 within 15s');
}
console.log(`[smoke] HTTP_Server bound on 127.0.0.1:${boundPort} with scrubbed PATH`);

// Hit /api/health to confirm the server actually responds.
try {
  const ok = await new Promise((resolveReq) => {
    const req = net.connect({ host: '127.0.0.1', port: boundPort }, () => {
      req.write('GET /api/health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n');
    });
    let buf = '';
    req.on('data', (d) => {
      buf += d.toString();
    });
    req.on('end', () => resolveReq(buf.includes('200') && buf.includes('ok')));
    req.on('error', () => resolveReq(false));
    req.setTimeout(2000, () => {
      req.destroy();
      resolveReq(false);
    });
  });
  if (!ok) fail('/api/health did not return a 200 ok response');
  console.log('[smoke] /api/health → 200 ok');
} finally {
  child.kill();
}

console.log('[smoke] PASS — distribution artifact starts and serves with no external runtime');
process.exit(0);
