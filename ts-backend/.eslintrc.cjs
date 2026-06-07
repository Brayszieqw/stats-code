/**
 * ESLint config enforcing the package dependency direction:
 *   api → server → engine
 *
 * A package may only import from packages strictly downstream of it.
 * `import/no-restricted-paths` zones reject upstream imports.
 *
 * NOTE: the `core` package (Rust agent-core analogue) was removed — its
 * orchestration responsibilities live in `server` (see ADR-0004). The chain is
 * now three layers, matching the actual module boundaries.
 */
module.exports = {
  root: true,
  parser: '@typescript-eslint/parser',
  parserOptions: {
    ecmaVersion: 2022,
    sourceType: 'module',
  },
  plugins: ['@typescript-eslint', 'import'],
  extends: [
    'eslint:recommended',
    'plugin:@typescript-eslint/recommended',
    'plugin:import/recommended',
    'plugin:import/typescript',
  ],
  settings: {
    'import/resolver': {
      typescript: {
        project: [
          'packages/api/tsconfig.json',
          'packages/server/tsconfig.json',
          'packages/engine/tsconfig.json',
        ],
      },
    },
  },
  rules: {
    'import/no-restricted-paths': [
      'error',
      {
        zones: [
          // engine is the base: it must not import any sibling package.
          {
            target: './packages/engine',
            from: ['./packages/server', './packages/api'],
            message: 'engine must not depend on server/api (boundary: api → server → engine).',
          },
          // server may only depend on engine.
          {
            target: './packages/server',
            from: ['./packages/api'],
            message: 'server must not depend on api (boundary: api → server → engine).',
          },
        ],
      },
    ],
    '@typescript-eslint/no-unused-vars': [
      'error',
      { argsIgnorePattern: '^_', varsIgnorePattern: '^_' },
    ],
  },
  ignorePatterns: ['**/dist/**', 'node_modules/**', 'scripts/**', '*.cjs'],
};
