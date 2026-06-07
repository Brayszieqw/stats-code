# Stats Code — TypeScript backend

Contract-preserving rewrite of the Rust backend (`api`, `agent-core`,
`agent-server`, `stats-code`) into TypeScript on Node.js 22 LTS. The React 19 +
Vite frontend (`web/`) stays unchanged.

## Layout

```
ts-backend/
├── packages/
│   ├── api/      ← crates/api          (top of chain)
│   ├── core/     ← crates/agent-core
│   ├── server/   ← crates/agent-server
│   └── engine/   ← crates/stats-code   (base; cli, stats, math, sidecar, …)
└── tests/{unit,integration,property,parity}
```

Dependency direction (enforced by `eslint-plugin-import`): **api → core → server → engine**.

## Commands

| Command | Purpose |
|---|---|
| `npm run build` | Build all packages via TS project references. |
| `npm run typecheck` | Type-check without emit. |
| `npm run lint` | ESLint incl. import-boundary rules. |
| `npm test` | Run all vitest suites. |
| `npm run bundle` | esbuild single-file bundle (task 1.3). |
| `npm run sea` | Node SEA blob injection → `stats-code.exe` (task 1.3). |
| `npm run release-meta` | Version + SHA256 metadata (task 1.4). |

## Status

Phase 0 scaffold. Sub-modules are typed placeholders filled in their respective
phases (see `.kiro/specs/typescript-backend-rewrite/tasks.md`).
