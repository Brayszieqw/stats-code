// server/spa.ts — SPA embedding + catch-all fallback (task 3.4).
// Mirrors crates/agent-server/src/handlers/static_assets.rs::serve.
//
// The embedded frontend (web/dist) is served from memory: in the SEA binary
// via node:sea getAsset, in dev/test from build/assets on disk. The server
// stays pure-routing by accepting an injectable SpaAssetSource.
//
// Fallback contract (Requirement 1.8 / 10.3):
//   - attempt static-asset resolution only for known asset prefixes / files;
//   - any other non-contract route returns index.html so the SPA router (React
//     Router) can handle deep links and refreshes;
//   - /api/* misses return a JSON 404 (never index.html).

import type { FastifyInstance } from 'fastify';

export interface ServedAsset {
  bytes: Uint8Array;
  contentType: string;
}

/** Read-only embedded asset lookup keyed by request path (leading slash). */
export interface SpaAssetSource {
  /** Resolve an exact asset by request path, e.g. "/assets/index-x.js". */
  get(routePath: string): ServedAsset | undefined;
  /** The index.html document used for the SPA fallback. */
  indexHtml(): ServedAsset;
}

/** Request-path prefixes that look like static assets (vs SPA deep links). */
export const ASSET_PREFIXES: readonly string[] = ['/assets/'];

/** Root-level files served directly when requested by exact path. */
export const ROOT_ASSET_FILES: readonly string[] = [
  '/index.html',
  '/favicon.ico',
  '/favicon.png',
  '/robots.txt',
  '/manifest.webmanifest',
];

function looksLikeAsset(routePath: string): boolean {
  if (ROOT_ASSET_FILES.includes(routePath)) return true;
  return ASSET_PREFIXES.some((p) => routePath.startsWith(p));
}

/**
 * Install the SPA catch-all fallback on `app`. Unmatched non-/api routes serve
 * an embedded asset (when the path looks like one and resolves) or fall back to
 * index.html. Unmatched /api routes keep returning a JSON 404.
 */
export function installSpaFallback(app: FastifyInstance, source: SpaAssetSource): void {
  app.setNotFoundHandler((req, reply) => {
    // /api/* misses are genuine 404s, never the SPA shell.
    if (req.url.split('?')[0]!.startsWith('/api/')) {
      return reply.code(404).send({ error_code: 'NotFound', message: 'route not found' });
    }

    // Only GET/HEAD requests serve the SPA; other verbs on unknown routes 404.
    if (req.method !== 'GET' && req.method !== 'HEAD') {
      return reply.code(404).send({ error_code: 'NotFound', message: 'route not found' });
    }

    const routePath = req.url.split('?')[0]!;

    if (looksLikeAsset(routePath)) {
      const asset = source.get(routePath);
      if (asset) {
        return reply
          .code(200)
          .header('content-type', asset.contentType)
          .send(Buffer.from(asset.bytes));
      }
    }

    // SPA fallback: index.html for any other route (deep links, refreshes).
    const index = source.indexHtml();
    return reply.code(200).header('content-type', index.contentType).send(Buffer.from(index.bytes));
  });
}
