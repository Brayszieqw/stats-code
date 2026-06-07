// scripts/release-meta.mjs — release metadata builder (Phase 0, task 1.4).
//
// Produces two records, preserving the existing scripts/release.ps1 contract:
//
//   1. version record  → build/release/version.json
//        { name, version, target, archive }
//   2. SHA256 checksum record → build/release/SHA256SUMS.txt
//        each line: "<64-hex-lowercase>  <filename>"   (GNU coreutils format)
//
// Requirement 10.4: metadata generation MUST be possible independently of
// producing the .exe. So this script hashes whichever of the contract files
// already exist and never builds anything. Missing files are reported but do
// not abort version-record emission unless --require-artifacts is passed.
//
// Usage:
//   node scripts/release-meta.mjs                 # emit metadata for present files
//   node scripts/release-meta.mjs --require-artifacts   # fail if exe is missing

import {
  createHash,
} from 'node:crypto';
import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  existsSync,
} from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve, join } from 'node:path';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');

const requireArtifacts = process.argv.includes('--require-artifacts');

const enginePkg = require(resolve(root, 'packages/engine/package.json'));
const version = enginePkg.version ?? '0.0.0';
const target = 'windows-x64';
const archive = `stats-code-${version}-${target}.zip`;

const outDir = resolve(root, 'build/release');
mkdirSync(outDir, { recursive: true });

function sha256Lower(path) {
  const hash = createHash('sha256');
  hash.update(readFileSync(path));
  return hash.digest('hex'); // already lowercase
}

// The Distribution_Artifact contract files (mirrors release.ps1 staging set).
const candidates = [
  { name: 'stats-code.exe', path: resolve(root, 'build/stats-code.exe') },
  { name: 'install.ps1', path: resolve(root, '../install.ps1') },
];

const sums = [];
const missing = [];
for (const { name, path } of candidates) {
  if (existsSync(path)) {
    sums.push(`${sha256Lower(path)}  ${name}`);
  } else {
    missing.push(name);
  }
}

if (requireArtifacts && missing.includes('stats-code.exe')) {
  console.error('error: build/stats-code.exe is missing but --require-artifacts was set.');
  process.exit(1);
}

// 1. version record (always emittable, independent of the .exe — R10.4).
const versionRecord = {
  name: 'stats-code',
  version,
  target,
  archive,
};
const versionPath = join(outDir, 'version.json');
writeFileSync(versionPath, JSON.stringify(versionRecord, null, 2) + '\n');

// 2. checksum record (GNU coreutils format: LF endings, trailing newline).
const sumsPath = join(outDir, 'SHA256SUMS.txt');
writeFileSync(sumsPath, sums.length > 0 ? sums.join('\n') + '\n' : '', {
  encoding: 'utf8',
});

console.log('release metadata written:');
console.log(`  version  : ${version}`);
console.log(`  ${versionPath}`);
console.log(`  ${sumsPath} (${sums.length} entr${sums.length === 1 ? 'y' : 'ies'})`);
if (missing.length > 0) {
  console.log(`  note: skipped missing file(s): ${missing.join(', ')}`);
}
