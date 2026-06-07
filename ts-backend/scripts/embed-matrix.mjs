// scripts/embed-matrix.mjs — embed coverage/matrix.toml as a TS string constant
// (task 13.1). Mirrors the Rust include_str!() approach so the matrix survives
// esbuild/SEA bundling with no file-system read at runtime.

import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const src = resolve(root, 'packages/engine/src/coverage/matrix.toml');
const out = resolve(root, 'packages/engine/src/coverage/matrix-data.ts');

const toml = readFileSync(src, 'utf8');
const json = JSON.stringify(toml); // safely escapes quotes, backslashes, newlines

const banner = '// AUTO-GENERATED from matrix.toml by scripts/embed-matrix.mjs. Do not edit.\n';
writeFileSync(out, `${banner}export const MATRIX_TOML = ${json};\n`);
console.log(`embedded matrix.toml → ${out} (${toml.length} chars)`);
