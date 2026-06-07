// server/spa-assets.ts — concrete SpaAssetSource backed by the embedded
// frontend bundle (task 3.4).
//
// Two backends, selected at runtime:
//   - SEA build: node:sea getAsset(name) reads the asset embedded in the .exe;
//   - dev/test: read build/assets/* from disk (produced by embed-assets.mjs).
//
// Both consult the asset-manifest.json that maps request paths → embedded asset
// name + content type. The manifest itself is embedded/loaded the same way.

import { readFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

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

function loadSea(): SeaApi | null {
  try {
    const require = createRequire(import.meta.url);
    const sea = require('node:sea') as SeaApi;
    if (typeof sea.isSea === 'function' && sea.isSea()) {
      return sea;
    }
  } catch {
    /* not a SEA binary */
  }
  return null;
}

/** Create a SpaAssetSource for a packaged SEA binary, or null if not in one. */
export function createSeaAssetSource(): SpaAssetSource | null {
  const sea = loadSea();
  if (!sea) return null;

  const manifest = JSON.parse(
    Buffer.from(sea.getAsset('asset-manifest.json')).toString('utf8'),
  ) as AssetManifest;

  const get = (routePath: string): ServedAsset | undefined => {
    const entry = manifest.entries[routePath];
    if (!entry) return undefined;
    return { bytes: new Uint8Array(sea.getAsset(entry.asset)), contentType: entry.contentType };
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
 * Create a SpaAssetSource reading build/assets from disk (dev/test). `baseDir`
 * defaults to the repo's ts-backend/build/assets relative to this module.
 */
export function createDiskAssetSource(baseDir?: string): SpaAssetSource {
  const here = dirname(fileURLToPath(import.meta.url));
  // dist layout: packages/server/dist/spa-assets.js → ../../../build/assets
  const dir = baseDir ?? join(here, '..', '..', '..', 'build', 'assets');
  const manifestPath = join(dir, 'asset-manifest.json');
  if (!existsSync(manifestPath)) {
    throw new Error(`SPA asset manifest not found at ${manifestPath}; run embed-assets first`);
  }
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8')) as AssetManifest;

  const get = (routePath: string): ServedAsset | undefined => {
    const entry = manifest.entries[routePath];
    if (!entry) return undefined;
    return { bytes: new Uint8Array(readFileSync(join(dir, entry.asset))), contentType: entry.contentType };
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
