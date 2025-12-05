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

