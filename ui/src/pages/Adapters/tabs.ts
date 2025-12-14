/**
 * @deprecated Use `useAdapterTabRouter()` from '@/hooks/navigation/useTabRouter' instead.
 * This file is retained for backward compatibility only and will be removed in a future version.
 * The new hook provides path-based routing (deep-linkable URLs) instead of hash-based routing.
 *
 * Migration example:
 * ```tsx
 * // OLD:
 * import { resolveAdaptersTab, adapterTabToPath } from '@/pages/Adapters/tabs';
 * const activeTab = resolveAdaptersTab(location.pathname, location.hash, adapterId);
 * const path = adapterTabToPath(activeTab, adapterId);
 *
 * // NEW:
 * import { useAdapterTabRouter } from '@/hooks/navigation/useTabRouter';
 * const { activeTab, setActiveTab, availableTabs, getTabPath } = useAdapterTabRouter();
 * ```
 */
export type AdaptersTab =
  | 'overview'
  | 'activations'
  | 'usage'
  | 'lineage'
  | 'manifest'
  | 'register'
  | 'policies';

export const adaptersTabOrder: AdaptersTab[] = [
  'overview',
  'activations',
  'usage',
  'lineage',
  'manifest',
  'register',
  'policies',
];

export function resolveAdaptersTab(pathname: string, hash: string, adapterId?: string): AdaptersTab {
  const normalizedHash = hash.toLowerCase();
  const basePath = adapterId ? `/adapters/${adapterId}` : '/adapters';

  if (pathname.startsWith('/adapters/new')) return 'register';
  if (pathname === `${basePath}/activations`) return 'activations';
  if (pathname === `${basePath}/usage`) return 'usage';
  if (pathname === `${basePath}/lineage`) return 'lineage';
  if (pathname === `${basePath}/manifest`) return 'manifest';
  if (normalizedHash.includes('policies')) return 'policies';
  return 'overview';
}

export function adapterTabToPath(tab: AdaptersTab, adapterId?: string): string {
  const basePath = adapterId ? `/adapters/${adapterId}` : '/adapters';
  switch (tab) {
    case 'overview':
      return basePath;
    case 'activations':
      return `${basePath}/activations`;
    case 'usage':
      return `${basePath}/usage`;
    case 'lineage':
      return `${basePath}/lineage`;
    case 'manifest':
      return `${basePath}/manifest`;
    case 'register':
      return '/adapters/new';
    case 'policies':
      return `${basePath}#policies`;
    default:
      return basePath;
  }
}

