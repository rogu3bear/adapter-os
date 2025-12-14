import { useParams } from 'react-router-dom';

/**
 * Hook to extract route parameters for breadcrumb resolution.
 * Returns a Record of param name to param value from the current route.
 *
 * Example:
 *   Route: /adapters/:adapterId/lineage
 *   URL: /adapters/abc-123/lineage
 *   Returns: { adapterId: 'abc-123' }
 */
export function useBreadcrumbParams(): Record<string, string> {
  const params = useParams();
  return params as Record<string, string>;
}

/**
 * Resolves a parameterized path template with actual parameter values.
 *
 * @param parameterizedPath - Path with :param placeholders (e.g., '/adapters/:adapterId')
 * @param params - Record of param names to values (e.g., { adapterId: 'abc-123' })
 * @returns Resolved path (e.g., '/adapters/abc-123')
 *
 * Example:
 *   resolvePathWithParams('/adapters/:adapterId/lineage', { adapterId: 'abc-123' })
 *   // Returns: '/adapters/abc-123/lineage'
 */
export function resolvePathWithParams(
  parameterizedPath: string,
  params: Record<string, string>
): string {
  let resolvedPath = parameterizedPath;

  // Replace each :paramName with its value from params
  Object.entries(params).forEach(([paramName, paramValue]) => {
    const paramPattern = `:${paramName}`;
    resolvedPath = resolvedPath.replace(paramPattern, paramValue);
  });

  return resolvedPath;
}

/**
 * Extracts parameter values from an actual pathname based on a route pattern.
 *
 * @param pathname - Actual URL path (e.g., '/adapters/abc-123/lineage')
 * @param routePattern - Route pattern (e.g., '/adapters/:adapterId/lineage')
 * @returns Record of param names to values (e.g., { adapterId: 'abc-123' })
 *
 * Example:
 *   extractParamsFromPath('/adapters/abc-123/lineage', '/adapters/:adapterId/lineage')
 *   // Returns: { adapterId: 'abc-123' }
 */
export function extractParamsFromPath(
  pathname: string,
  routePattern: string
): Record<string, string> {
  const params: Record<string, string> = {};

  const patternParts = routePattern.split('/');
  const pathParts = pathname.split('/');

  if (patternParts.length !== pathParts.length) {
    return params;
  }

  patternParts.forEach((part, index) => {
    if (part.startsWith(':')) {
      const paramName = part.slice(1); // Remove the ':' prefix
      params[paramName] = pathParts[index];
    }
  });

  return params;
}
