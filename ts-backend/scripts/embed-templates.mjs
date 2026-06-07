// scripts/embed-templates.mjs — embed sidecar templates as a TS map (task 13.3).
//
// Reads the authoritative templates from the Rust crate
// (crates/stats-code/src/sidecar/templates/<software>/<id>.tmpl.txt) and emits
// packages/engine/src/sidecar/templates-data.ts with a frozen map keyed by
// "<id>\u0000<software>". Mirrors the Rust include_str! approach so templates
// survive esbuild/SEA bundling with no runtime file reads.

import { readdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const repoRoot = resolve(root, '..');
const templatesRoot = resolve(repoRoot, 'crates/stats-code/src/sidecar/templates');
const out = resolve(root, 'packages/engine/src/sidecar/templates-data.ts');

const SOFTWARE_DIR_TO_TOKEN = { r: 'R', sas: 'SAS', python: 'Python', spss: 'SPSS' };

if (!existsSync(templatesRoot)) {
  console.error(`error: templates dir not found at ${templatesRoot}`);
  process.exit(1);
}

const entries = {};
let count = 0;
for (const dir of readdirSync(templatesRoot)) {
  const software = SOFTWARE_DIR_TO_TOKEN[dir];
  if (!software) continue;
  const dirPath = join(templatesRoot, dir);
  for (const file of readdirSync(dirPath)) {
    const m = /^(.+)\.tmpl\.txt$/.exec(file);
    if (!m) continue;
    const id = m[1];
    // Normalize CRLF → LF to honor the LF-only determinism contract.
    const text = readFileSync(join(dirPath, file), 'utf8').replace(/\r\n/g, '\n');
    entries[`${id}\u0000${software}`] = text;
    count += 1;
  }
}

const banner = '// AUTO-GENERATED from crates/stats-code/src/sidecar/templates by scripts/embed-templates.mjs. Do not edit.\n';
const body = `export const SIDECAR_TEMPLATES: Readonly<Record<string, string>> = ${JSON.stringify(entries, null, 2)};\n`;
writeFileSync(out, banner + body);
console.log(`embedded ${count} sidecar template(s) → ${out}`);
