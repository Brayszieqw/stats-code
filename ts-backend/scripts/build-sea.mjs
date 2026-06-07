// scripts/build-sea.mjs — Node 22 SEA blob injection (Phase 0, task 1.3).
//
// Pipeline:
//   1. embed-assets  → build/assets/* + asset-manifest.json (frontend in-memory)
//   2. bundle        → build/bundle.cjs (single CommonJS file)
//   3. generate a SEA config embedding the bundle + every frontend asset
//   4. node --experimental-sea-config → build/sea-prep.blob
//   5. copy the node binary → build/stats-code.exe
//   6. postject the blob into the exe (NODE_SEA_BLOB / NODE_SEA_FUSE)
//
// Produces build/stats-code.exe: a zero-external-runtime Windows x64 artifact.

import { execFileSync } from 'node:child_process';
import {
  readdirSync,
  copyFileSync,
  writeFileSync,
  mkdirSync,
  existsSync,
  chmodSync,
} from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve, join } from 'node:path';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const build = resolve(root, 'build');
const assetsDir = join(build, 'assets');

function run(cmd, args, opts = {}) {
  console.log(`$ ${cmd} ${args.join(' ')}`);
  execFileSync(cmd, args, { stdio: 'inherit', cwd: root, ...opts });
}

mkdirSync(build, { recursive: true });

// 1. embed frontend assets
run(process.execPath, [resolve(here, 'embed-assets.mjs')]);

// 2. bundle the backend
run(process.execPath, [resolve(here, 'bundle.mjs')]);

// 3. generate a SEA config embedding the bundle + every frontend asset.
const assetFiles = readdirSync(assetsDir);
const assetMap = {};
for (const f of assetFiles) {
  // asset key = file name as stored; the runtime resolves via asset-manifest.json
  assetMap[f] = join('build', 'assets', f).split('\\').join('/');
}

const seaConfig = {
  main: 'build/bundle.cjs',
  output: 'build/sea-prep.blob',
  disableExperimentalSEAWarning: true,
  useSnapshot: false,
  useCodeCache: true,
  assets: assetMap,
};
const seaConfigPath = join(build, 'sea-config.generated.json');
writeFileSync(seaConfigPath, JSON.stringify(seaConfig, null, 2));
console.log(`generated SEA config with ${assetFiles.length} embedded asset(s)`);

// 4. generate the SEA blob
run(process.execPath, ['--experimental-sea-config', seaConfigPath]);

// 5. copy the node binary as the target exe
const isWindows = process.platform === 'win32';
const exeName = isWindows ? 'stats-code.exe' : 'stats-code';
const exePath = join(build, exeName);
copyFileSync(process.execPath, exePath);
if (!isWindows) {
  chmodSync(exePath, 0o755);
}

// 6. inject the blob with postject
const blobPath = join(build, 'sea-prep.blob');
if (!existsSync(blobPath)) {
  throw new Error(`SEA blob not produced at ${blobPath}`);
}

const postjectBin = require.resolve('postject/dist/cli.js');
const postjectArgs = [
  postjectBin,
  exePath,
  'NODE_SEA_BLOB',
  blobPath,
  '--sentinel-fuse',
  'NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2',
];
if (process.platform === 'darwin') {
  postjectArgs.push('--macho-segment-name', 'NODE_SEA');
}
run(process.execPath, postjectArgs);

console.log(`\nSEA artifact ready → ${exePath}`);
