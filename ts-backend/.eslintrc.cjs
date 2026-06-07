/**
 * ESLint config enforcing the package dependency direction:
 *   api → core → server → engine
 *
 * A package may only import from packages strictly downstream of it.
 * `import/no-restricted-paths` zones reject upstream imports.
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
          'packages/core/tsconfig.json',
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
            from: ['./packages/server', './packages/core', './packages/api'],
            message: 'engine must not depend on server/core/api (boundary: api → core → server → engine).',
          },
          // server may only depend on engine.
          {
            target: './packages/server',
            from: ['./packages/core', './packages/api'],
            message: 'server must not depend on core/api (boundary: api → core → server → engine).',
          },
          // core may only depend on server (and transitively engine).
          {
            target: './packages/core',
            from: ['./packages/api'],
            message: 'core must not depend on api (boundary: api → core → server → engine).',
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
