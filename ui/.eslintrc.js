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
        // Enforce @/ alias instead of relative imports
        // NOTE: Keep at 'warn' until existing violations are fixed, then upgrade to 'error'
        'no-restricted-imports': ['warn', {
          patterns: [
            {
              group: ['../*'],
              message: 'Use @/ alias for imports outside current directory.',
            },
            {
              group: ['../../**/components/*', '../../../**/components/*'],
              message: 'Use @/components instead of deep relative imports. Example: import { Button } from "@/components/ui/button"',
            },
            {
              group: ['../../**/hooks/*', '../../../**/hooks/*'],
              message: 'Use @/hooks instead of deep relative imports. Example: import { useAuth } from "@/hooks"',
            },
            {
              group: ['../../**/utils/*', '../../../**/utils/*'],
              message: 'Use @/utils instead of deep relative imports. Example: import { logger } from "@/utils"',
            },
            {
              group: ['../../**/contexts/*', '../../../**/contexts/*'],
              message: 'Use @/contexts instead of deep relative imports. Example: import { useDensity } from "@/contexts"',
            },
          ],
        }],
        // TypeScript type safety rules
        '@typescript-eslint/no-explicit-any': 'warn',
        '@typescript-eslint/explicit-module-boundary-types': 'off',
        'max-lines-per-function': ['warn', { max: 500, skipBlankLines: true, skipComments: true }],
      },
    },
    {
      files: ['src/utils/logger.ts'],
      rules: {
        'no-console': 'off',
      },
    },
    {
      files: ['src/__tests__/**/*.{ts,tsx}', '**/*.test.{ts,tsx}', '**/*.spec.{ts,tsx}'],
      rules: {
        'no-console': 'off',
        // Be more permissive with 'any' in test files
        '@typescript-eslint/no-explicit-any': 'off',
        'max-lines-per-function': 'off',
      },
    },
  ],
};
