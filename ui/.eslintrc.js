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
        // Enforce @/ alias instead of deep relative imports
        // Using 'warn' to avoid blocking development during migration
        'no-restricted-imports': ['warn', {
          patterns: [
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
