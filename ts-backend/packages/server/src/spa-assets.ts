// server/spa-assets.ts — concrete SpaAssetSource backed by the embedded
// frontend bundle (task 3.4).
//
// Two backends, selected at runtime:
//   - SEA build: node:sea getAsset(name) reads the asset embedded in the .exe;
//   - dev/test: read build/assets/* from disk (produced by embed-assets.mjs).
//
// Both consult the asset-manifest.json that maps request paths → embedded asset
// name + content type. The manifest itself is embedded/loaded the same way.
//
// IMPORTANT: the SEA bundle is emitted as CommonJS, where `import.meta.url` is
// empty. Never use import.meta.url for createRequire / path resolution here.

import { readFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';

import type { ServedAsset, SpaAssetSource } from './spa.js';

interface ManifestEntry {
  asset: string;
  contentType: string;
}
interface AssetManifest {
  generatedFrom: string;
  entries: Record<string, ManifestEntry>;
}

const INDEX_HTML_PATH = '/index.html';
const INDEX_HTML_CONTENT_TYPE = 'text/html; charset=utf-8';

/** Minimal node:sea surface (present only inside a packaged SEA binary). */
interface SeaApi {
  isSea?: () => boolean;
  getAsset(name: string, encoding?: string): ArrayBuffer;
}

/**
 * Resolve a working `require` for the CJS SEA bundle.
 * `createRequire(import.meta.url)` is unusable when import.meta is empty.
 */
function getRequire(): NodeRequire {
  // Ambient require exists in the esbuild CJS bundle and in classic CommonJS.
  // eslint-disable-next-line @typescript-eslint/no-implied-eval, no-new-func
  const ambient = typeof require === 'function' ? require : null;
  if (ambient) return ambient as NodeRequire;

  // ESM / test path: invent a stable filename for createRequire.
  const anchor = join(process.cwd(), 'package.json');
  return createRequire(pathToFileURL(anchor).href);
}

function loadSea(): SeaApi | null {
  try {
    const req = getRequire();
    const sea = req('node:sea') as SeaApi;
    if (typeof sea.isSea === 'function' && sea.isSea()) {
      return sea;
    }
  } catch {
    /* not a SEA binary or sea unavailable */
  }
  return null;
}

/** Create a SpaAssetSource for a packaged SEA binary, or null if not in one. */
export function createSeaAssetSource(): SpaAssetSource | null {
  const sea = loadSea();
  if (!sea) return null;

  let manifest: AssetManifest;
  try {
    manifest = JSON.parse(
      Buffer.from(sea.getAsset('asset-manifest.json')).toString('utf8'),
    ) as AssetManifest;
  } catch {
    return null;
  }

  const get = (routePath: string): ServedAsset | undefined => {
    const entry = manifest.entries[routePath];
    if (!entry) return undefined;
    try {
      return { bytes: new Uint8Array(sea.getAsset(entry.asset)), contentType: entry.contentType };
    } catch {
      return undefined;
    }
  };

  return {
    get,
    indexHtml(): ServedAsset {
      const html = get(INDEX_HTML_PATH);
      if (!html) {
        throw new Error('embedded index.html is missing — prod build invariant violated');
      }
      return html;
    },
  };
}

/**
 * Resolve the on-disk assets directory without relying on import.meta.url.
 * Search order:
 *   1. explicit baseDir
 *   2. <exeDir>/assets  (optional sidecar layout next to stats-code.exe)
 *   3. <cwd>/build/assets  (dev: started from ts-backend/)
 *   4. <cwd>/ts-backend/build/assets  (dev: started from repo root)
 */
export function resolveAssetsDir(baseDir?: string): string | null {
  const candidates: string[] = [];
  if (baseDir) candidates.push(baseDir);
  candidates.push(join(dirname(process.execPath), 'assets'));
  candidates.push(join(process.cwd(), 'build', 'assets'));
  candidates.push(join(process.cwd(), 'ts-backend', 'build', 'assets'));

  for (const dir of candidates) {
    if (existsSync(join(dir, 'asset-manifest.json'))) return dir;
  }
  return null;
}

/**
 * Create a SpaAssetSource reading build/assets from disk (dev/test).
 */
export function createDiskAssetSource(baseDir?: string): SpaAssetSource {
  const dir = resolveAssetsDir(baseDir);
  if (!dir) {
    throw new Error(
      'SPA asset manifest not found; run embed-assets (build/assets) or pass baseDir',
    );
  }
  const manifest = JSON.parse(
    readFileSync(join(dir, 'asset-manifest.json'), 'utf8'),
  ) as AssetManifest;

  const get = (routePath: string): ServedAsset | undefined => {
    const entry = manifest.entries[routePath];
    if (!entry) return undefined;
    const file = join(dir, entry.asset);
    if (!existsSync(file)) return undefined;
    return { bytes: new Uint8Array(readFileSync(file)), contentType: entry.contentType };
  };

  return {
    get,
    indexHtml(): ServedAsset {
      const html = get(INDEX_HTML_PATH);
      if (html) return html;
      // Last-resort: a minimal shell so the server still answers (tests may run
      // without a full frontend build).
      return {
        bytes: new TextEncoder().encode('<!doctype html><title>stats-code</title>'),
        contentType: INDEX_HTML_CONTENT_TYPE,
      };
    },
  };
}

/** Pick the SEA source when packaged, else the on-disk dev source. */
export function createDefaultAssetSource(baseDir?: string): SpaAssetSource {
  return createSeaAssetSource() ?? createDiskAssetSource(baseDir);
}
