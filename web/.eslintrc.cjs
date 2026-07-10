/* Minimal ESLint config for the deciduous web viewer (ESLint 8 + @typescript-eslint 6) */
module.exports = {
  root: true,
  env: { browser: true, es2021: true },
  parser: '@typescript-eslint/parser',
  parserOptions: {
    ecmaVersion: 'latest',
    sourceType: 'module',
    ecmaFeatures: { jsx: true },
  },
  plugins: ['@typescript-eslint'],
  extends: [
    'eslint:recommended',
    'plugin:@typescript-eslint/recommended',
  ],
  ignorePatterns: ['dist', 'dist-embed', 'node_modules', 'src/types/generated'],
  rules: {
    // The codebase relies on non-null assertions for Map access patterns
    '@typescript-eslint/no-non-null-assertion': 'off',
  },
};
