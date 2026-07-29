# Stats Code — TypeScript backend

The production backend of Stats Code: a contract-preserving TypeScript rewrite
of the original Rust workspace, running on Node.js 22 LTS. The rewrite is
**complete** (all phases of the historical `typescript-backend-rewrite` spec
checked off; spec archived locally under `work/backups/`); the Rust workspace
was retired and removed — its final state is
retired; no archive tag is retained.

The React 19 + Vite frontend lives in `../web/` and talks to this backend over
HTTP/SSE (13 contract routes + SPA fallback).

## Layout

```
ts-backend/
├── dev-server.mjs        ← dev entry: runs the launcher from packages/api/dist
├── packages/
│   ├── api/      ← application composition + SEA binary entry (top of chain)
│   ├── server/   ← HTTP transport (contract routes), conversation/LLM orchestration
│   └── engine/   ← pure computation: 17 algorithms, math core, sidecar,
│                    snapshot/replay, launcher, coverage matrix, templates
└── tests/{unit,integration,property,parity}
```

Dependency direction (enforced by `eslint-plugin-import`): **api → server → engine**.

In-tree data sources (no references outside `ts-backend/`):

- Sidecar templates: `packages/engine/src/sidecar/templates/<software>/<id>.tmpl.txt`
  → embedded into `templates-data.ts` by `scripts/embed-templates.mjs` at build time.
- Coverage matrix: `packages/engine/src/coverage/matrix.toml`
  → embedded into `matrix-data.ts` by `scripts/embed-matrix.mjs`.
- Parity oracle: `tests/parity/known_values/<software>/<algorithm>/baseline.json`
  (32 recorded reference baselines from R/SAS/Python/SPSS).

## Commands

| Command | Purpose |
|---|---|
| `npm run build` | Embed templates/matrix, then build all packages (TS project references). |
| `npm run typecheck` | Type-check without emit. |
| `npm run lint` | ESLint incl. import-boundary rules. |
| `npm test` | Run all vitest suites (unit + integration + property + parity). |
| `npm run bundle` | esbuild single-file bundle. |
| `npm run sea` | Node SEA blob injection → `build/stats-code.exe` (embeds `../web/dist`). |
| `npm run smoke` | Boot the built exe and probe it with no external runtime. |
| `npm run release-meta` | Version + SHA256 metadata → `build/release/`. |

## Running locally

- Day-to-day: run `../启动Stats前端.bat` — it builds this package, then starts
  `node dev-server.mjs` (API on `:8080`) and the Vite dev server (`:5173`).
- `dev-server.mjs` executes **built** output from `packages/api/dist/`; after
  editing sources, re-run `npm run build` (the bat does this for you).
- Full release: `../scripts/release.ps1` (web build → backend build → SEA exe
  → smoke test → zip with install.ps1 + SHA256SUMS).

## Runtime state

All per-user state lives in `%APPDATA%\stats-code\`:
`llm-config.json` (LLM provider/key, atomic writes, corrupt-file backup),
`sessions.json` (file-backed session store), and uploaded datasets.
