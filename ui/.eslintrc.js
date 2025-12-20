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
	        // Prevent legacy query-param tab routing (tabs are path-based).
	        'no-restricted-syntax': [
	          'error',
	          {
	            selector: "Literal[value=/[?&]tab=/]",
	            message: 'Do not use legacy "?tab=" routing. Use path-based tab routes and navLinks builders instead.',
	          },
	          {
	            selector: "TemplateElement[value.raw=/[?&]tab=/]",
	            message: 'Do not use legacy "?tab=" routing. Use path-based tab routes and navLinks builders instead.',
	          },
	        ],
	        // Block deprecated flat hook paths - use domain-organized canonical paths
	        // These are hard errors to prevent future usage of deprecated import patterns
	        'no-restricted-imports': ['error', {
	          paths: [
	            {
	              name: '@/hooks/useAdmin',
	              message: 'Import from "@/hooks/admin/useAdmin" instead of deprecated flat path.',
	            },
	            {
	              name: '@/hooks/useCollectionsApi',
	              message: 'Import from "@/hooks/api/useCollectionsApi" instead of deprecated flat path.',
	            },
	            {
	              name: '@/hooks/useChatSessionsApi',
	              message: 'Import from "@/hooks/chat/useChatSessionsApi" instead of deprecated flat path.',
	            },
	            {
	              name: '@/hooks/useChatTags',
	              message: 'Import from "@/hooks/chat/useChatTags" instead of deprecated flat path.',
	            },
	          ],
	          // Relative import patterns - kept at warn level until existing violations are fixed
	          // NOTE: These produce warnings, not errors. The 'error' level above only applies to paths.
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
	        'no-restricted-syntax': 'off',
	        // Be more permissive with 'any' in test files
	        '@typescript-eslint/no-explicit-any': 'off',
	        'max-lines-per-function': 'off',
	      },
	    },
    {
      // Strict enforcement for route configuration to prevent routing modal components
      // or components with required props that RouteGuard cannot provide.
      //
      // Why these rules exist:
      // RouteGuard renders components as `<Component />` with NO PROPS.
      // If a component has required props, it will crash at runtime.
      // These rules prevent engineers from using `as any` or other casts to bypass
      // TypeScript's type checking when adding route components.
      //
      // If you have a component with required props:
      // 1. Create a *RoutePage wrapper (e.g., TenantDetailRoutePage)
      // 2. The wrapper reads params from URL via useParams()
      // 3. The wrapper fetches data via hooks
      // 4. The wrapper renders the original component with proper props
      //
      // See TenantDetailRoutePage for an example.
	      files: ['src/config/routes.ts'],
	      rules: {
        // Block explicit 'any' types - modal components should use *RoutePage wrappers
        '@typescript-eslint/no-explicit-any': 'error',
        // Block type assertions that bypass routeability checks
	        'no-restricted-syntax': [
	          'error',
	          {
	            selector: "Literal[value=/[?&]tab=/]",
	            message: 'Do not use legacy "?tab=" routing. Use path-based tab routes and navLinks builders instead.',
	          },
	          {
	            selector: "TemplateElement[value.raw=/[?&]tab=/]",
	            message: 'Do not use legacy "?tab=" routing. Use path-based tab routes and navLinks builders instead.',
	          },
	          {
	            // Block: as any
	            selector: 'TSAsExpression > TSAnyKeyword',
	            message: 'Type assertion to "any" is not allowed in routes.ts. If a component has required props, create a *RoutePage wrapper instead.',
          },
          {
            // Block: as unknown (often used as intermediate step: as unknown as X)
            selector: 'TSAsExpression > TSUnknownKeyword',
            message: 'Type assertion to "unknown" is not allowed in routes.ts (often used to bypass type safety). If a component has required props, create a *RoutePage wrapper instead.',
          },
          {
            // Block: as React.ComponentType<any> or ComponentType<any>
            selector: 'TSAsExpression > TSTypeReference[typeName.name="ComponentType"] > TSTypeParameterInstantiation > TSAnyKeyword',
            message: 'Type assertion to ComponentType<any> is not allowed in routes.ts. If a component has required props, create a *RoutePage wrapper instead.',
          },
          {
            // Block: as React.ComponentType<unknown>
            selector: 'TSAsExpression > TSTypeReference[typeName.name="ComponentType"] > TSTypeParameterInstantiation > TSUnknownKeyword',
            message: 'Type assertion to ComponentType<unknown> is not allowed in routes.ts. If a component has required props, create a *RoutePage wrapper instead.',
          },
          {
            // Block: <any> type assertion (legacy syntax)
            selector: 'TSTypeAssertion > TSAnyKeyword',
            message: 'Type assertion to "any" is not allowed in routes.ts. If a component has required props, create a *RoutePage wrapper instead.',
          },
          {
            // Block: <unknown> type assertion (legacy syntax)
            selector: 'TSTypeAssertion > TSUnknownKeyword',
            message: 'Type assertion to "unknown" is not allowed in routes.ts. If a component has required props, create a *RoutePage wrapper instead.',
          },
          {
            // Block: // @ts-ignore comments
            selector: 'Line[value=/^\\s*@ts-ignore/]',
            message: '@ts-ignore is not allowed in routes.ts. If a component has required props, create a *RoutePage wrapper instead.',
          },
          {
            // Block: // @ts-expect-error comments
            selector: 'Line[value=/^\\s*@ts-expect-error/]',
            message: '@ts-expect-error is not allowed in routes.ts. If a component has required props, create a *RoutePage wrapper instead.',
          },
        ],
        // Disallow @ts-ignore and @ts-expect-error comments
        '@typescript-eslint/ban-ts-comment': ['error', {
          'ts-ignore': true,
          'ts-expect-error': true,
          'ts-nocheck': true,
        }],
      },
    },
  ],
};
