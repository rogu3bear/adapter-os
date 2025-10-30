module.exports = {
  root: true,
  parser: '@typescript-eslint/parser',
  parserOptions: {
    ecmaVersion: 2021,
    sourceType: 'module',
    ecmaFeatures: { jsx: true },
  },
  env: { browser: true, es2021: true, node: true },
  plugins: ['@typescript-eslint', 'react-hooks'],
  overrides: [
    {
      files: ['src/**/*.{ts,tsx}'],
      rules: {
        'no-console': ['error', { allow: ['error'] }],
        // allow repository annotations that reference this rule
        'react-hooks/exhaustive-deps': 'warn',
      },
    },
    {
      files: ['src/utils/logger.ts'],
      rules: {
        'no-console': 'off',
      },
    },
    {
      files: ['src/__tests__/**/*.{ts,tsx}'],
      rules: {
        'no-console': 'off',
      },
    },
  ],
};
