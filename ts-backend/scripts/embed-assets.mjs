// scripts/embed-assets.mjs — collect web/dist into SEA-embeddable assets
// (Phase 0, task 1.3).
//
// Node SEA embeds named assets accessible at runtime via
// `require('node:sea').getAsset(name)`. We embed each frontend file under its
// forward-slash relative path, plus a manifest enumerating them so the
// HTTP_Server (task 3.4) can serve the SPA from memory with no web server or
// build step on the end-user machine.
//
// The manifest maps each served route path → { asset, contentType }.

import {
  readdirSync,
  statSync,
  mkdirSync,
  copyFileSync,
  writeFileSync,
  rmSync,
  existsSync,
} from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve, join, relative, extname } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const webDist = resolve(root, '../web/dist');
const outAssets = resolve(root, 'build/assets');

const CONTENT_TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.webp': 'image/webp',
  '.ico': 'image/x-icon',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.ttf': 'font/ttf',
  '.map': 'application/json; charset=utf-8',
  '.txt': 'text/plain; charset=utf-8',
  '.wasm': 'application/wasm',
};

function walk(dir, acc = []) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) {
      walk(full, acc);
    } else {
      acc.push(full);
    }
  }
  return acc;
}

if (!existsSync(webDist)) {
  console.error(`error: frontend build not found at ${webDist}. Run the web build first.`);
  process.exit(1);
}

rmSync(outAssets, { recursive: true, force: true });
mkdirSync(outAssets, { recursive: true });

const files = walk(webDist);
const manifest = {};

for (const full of files) {
  const rel = relative(webDist, full).split('\\').join('/');
  const flat = rel.split('/').join('__');
  copyFileSync(full, join(outAssets, flat));
  manifest[`/${rel}`] = {
    asset: flat,
    contentType: CONTENT_TYPES[extname(full).toLowerCase()] ?? 'application/octet-stream',
  };
}

writeFileSync(
  join(outAssets, 'asset-manifest.json'),
  JSON.stringify({ generatedFrom: 'web/dist', entries: manifest }, null, 2),
);

console.log(`embedded ${files.length} frontend asset(s) → ${outAssets}`);
