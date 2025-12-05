/**
 * Routes Debug Page - Dev-only route inventory and analysis
 *
 * Accessible at: /_dev/routes
 *
 * Shows:
 * - All registered routes with metadata
 * - Status (active/deprecated/draft)
 * - Reachability (primary/nested/hidden/orphan)
 * - Duplicates and orphans highlighted
 * - Section/type breakdown
 * - Hub tab validation (missing/extra tabs)
 * - Product flow validation
 * - Component file paths with copy button
 */

import { useState, useMemo } from 'react';
import {
  getRouteManifest,
  getManifestStats,
  validateFlow,
  PRODUCT_FLOWS,
  PRIMARY_SPINE,
  type RouteManifestEntry,
  type RouteSection,
  type RouteType,
  type RouteStatus,
  type Reachability,
} from '@/config/routes_manifest';
import { cn } from '@/components/ui/utils';
import {
  AlertTriangle,
  CheckCircle,
  ChevronDown,
  ChevronRight,
  Copy,
  Eye,
  EyeOff,
  Filter,
  LayoutGrid,
  List,
  Lock,
  XCircle,
} from 'lucide-react';
import PageWrapper from '@/layout/PageWrapper';

type ViewMode = 'table' | 'grouped';
type FilterMode = 'all' | 'issues' | 'orphans' | 'duplicates' | 'hubs' | 'deprecated' | 'draft' | 'hidden';

const SECTION_COLORS: Record<RouteSection, string> = {
  Core: 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200',
  Adapters: 'bg-purple-100 text-purple-800 dark:bg-purple-900 dark:text-purple-200',
  Training: 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200',
  Inference: 'bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-200',
  System: 'bg-slate-100 text-slate-800 dark:bg-slate-900 dark:text-slate-200',
  Monitor: 'bg-cyan-100 text-cyan-800 dark:bg-cyan-900 dark:text-cyan-200',
  Security: 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200',
  Admin: 'bg-orange-100 text-orange-800 dark:bg-orange-900 dark:text-orange-200',
  Labs: 'bg-pink-100 text-pink-800 dark:bg-pink-900 dark:text-pink-200',
};

const TYPE_BADGES: Record<RouteType, { label: string; className: string }> = {
  hub: { label: 'HUB', className: 'bg-indigo-500 text-white' },
  detail: { label: 'detail', className: 'bg-gray-200 text-gray-700 dark:bg-gray-700 dark:text-gray-200' },
  tool: { label: 'tool', className: 'bg-teal-200 text-teal-800 dark:bg-teal-800 dark:text-teal-200' },
  landing: { label: 'landing', className: 'bg-yellow-200 text-yellow-800 dark:bg-yellow-800 dark:text-yellow-200' },
  future: { label: 'future', className: 'bg-gray-400 text-white' },
};

const STATUS_BADGES: Record<RouteStatus, { label: string; className: string }> = {
  active: { label: 'active', className: 'bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-300' },
  deprecated: { label: 'deprecated', className: 'bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300' },
  draft: { label: 'draft', className: 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900 dark:text-yellow-300' },
};

const REACHABILITY_BADGES: Record<Reachability, { label: string; className: string; icon: typeof Eye }> = {
  primary: { label: 'primary', className: 'bg-green-50 text-green-600 dark:bg-green-950 dark:text-green-400', icon: Eye },
  nested: { label: 'nested', className: 'bg-blue-50 text-blue-600 dark:bg-blue-950 dark:text-blue-400', icon: ChevronRight },
  hidden: { label: 'hidden', className: 'bg-gray-100 text-gray-500 dark:bg-gray-800 dark:text-gray-400', icon: EyeOff },
  orphan: { label: 'orphan', className: 'bg-red-50 text-red-600 dark:bg-red-950 dark:text-red-400', icon: XCircle },
};

function Badge({ children, className }: { children: React.ReactNode; className?: string }) {
  return (
    <span className={cn('px-2 py-0.5 rounded text-xs font-medium', className)}>
      {children}
    </span>
  );
}

function StatCard({ label, value, highlight, onClick }: {
  label: string;
  value: number;
  highlight?: boolean;
  onClick?: () => void;
}) {
  return (
    <button
      onClick={onClick}
      disabled={!onClick}
      className={cn(
        'p-3 rounded-lg border text-left transition-colors',
        highlight ? 'border-amber-500 bg-amber-50 dark:bg-amber-950' : 'border-border bg-card',
        onClick && 'hover:border-primary cursor-pointer'
      )}
    >
      <div className="text-2xl font-bold">{value}</div>
      <div className="text-xs text-muted-foreground">{label}</div>
    </button>
  );
}

function CopyButton({ text, label }: { text: string; label?: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <button
      onClick={handleCopy}
      className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
      title={`Copy ${label || text}`}
    >
      {copied ? (
        <CheckCircle className="h-3 w-3 text-green-500" />
      ) : (
        <Copy className="h-3 w-3" />
      )}
    </button>
  );
}

function FlowValidator() {
  const [expandedFlow, setExpandedFlow] = useState<string | null>(null);

  const flowResults = useMemo(() => {
    return Object.entries(PRODUCT_FLOWS).map(([key, flow]) => ({
      key,
      name: flow.name,
      steps: flow.steps,
      result: validateFlow(key as keyof typeof PRODUCT_FLOWS),
    }));
  }, []);

  return (
    <div className="space-y-2">
      <h3 className="text-sm font-medium text-muted-foreground">Product Flow Validation</h3>
      <div className="space-y-1">
        {flowResults.map(({ key, name, steps, result }) => (
          <div key={key} className="border rounded-lg overflow-hidden">
            <button
              onClick={() => setExpandedFlow(expandedFlow === key ? null : key)}
              className="w-full flex items-center gap-2 p-2 hover:bg-muted/50 text-left"
            >
              {result.valid ? (
                <CheckCircle className="h-4 w-4 text-green-500 flex-shrink-0" />
              ) : (
                <XCircle className="h-4 w-4 text-red-500 flex-shrink-0" />
              )}
              <span className="flex-1 text-sm truncate">{name}</span>
              <span className="text-xs text-muted-foreground">{steps.length} steps</span>
              {result.issues.length > 0 && (
                <Badge className="bg-red-100 text-red-700">{result.issues.length}</Badge>
              )}
              {expandedFlow === key ? (
                <ChevronDown className="h-4 w-4" />
              ) : (
                <ChevronRight className="h-4 w-4" />
              )}
            </button>
            {expandedFlow === key && (
              <div className="border-t bg-muted/30 p-2 space-y-1">
                {steps.map((step, i) => {
                  const issue = result.issues.find(is => is.step === i + 1);
                  return (
                    <div
                      key={i}
                      className={cn(
                        'flex items-center gap-2 text-xs p-1 rounded',
                        issue && 'bg-red-100 dark:bg-red-900/30'
                      )}
                    >
                      <span className="w-5 text-muted-foreground">{i + 1}.</span>
                      <code className="font-mono flex-1">{step.page}</code>
                      <span className="text-muted-foreground">{step.action}</span>
                      {issue && (
                        <span className="text-red-600 dark:text-red-400">{issue.issue}</span>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

function HubValidation({ route }: { route: RouteManifestEntry }) {
  if (!route.isHub) return null;

  const hasMissing = route.missingTabs.length > 0;
  const hasExtra = route.extraTabs.length > 0;

  if (!hasMissing && !hasExtra && route.tabs.length === 0) {
    return <span className="text-xs text-muted-foreground">no tabs</span>;
  }

  return (
    <div className="space-y-1">
      {route.tabs.length > 0 && (
        <div className="text-xs">
          <span className="text-muted-foreground">tabs:</span>{' '}
          {route.tabs.map((t, i) => (
            <span key={t}>
              <code className="text-xs">{t.replace(route.path, '.')}</code>
              {i < route.tabs.length - 1 && ', '}
            </span>
          ))}
        </div>
      )}
      {hasMissing && (
        <div className="text-xs text-amber-600">
          missing: {route.missingTabs.join(', ')}
        </div>
      )}
      {hasExtra && (
        <div className="text-xs text-blue-600">
          extra: {route.extraTabs.join(', ')}
        </div>
      )}
    </div>
  );
}

function RouteTable({ routes, showExtended }: { routes: RouteManifestEntry[]; showExtended?: boolean }) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b bg-muted/50">
            <th className="text-left p-2 font-medium">Path</th>
            <th className="text-left p-2 font-medium">Nav Title</th>
            <th className="text-left p-2 font-medium">Section</th>
            <th className="text-left p-2 font-medium">Type</th>
            <th className="text-left p-2 font-medium">Status</th>
            <th className="text-left p-2 font-medium">Reach</th>
            {showExtended && <th className="text-left p-2 font-medium">Component</th>}
            {showExtended && <th className="text-left p-2 font-medium">Role</th>}
            <th className="text-left p-2 font-medium">Issues</th>
          </tr>
        </thead>
        <tbody>
          {routes.map((route) => {
            const ReachIcon = REACHABILITY_BADGES[route.reachability].icon;
            const isSpine = PRIMARY_SPINE.includes(route.path as typeof PRIMARY_SPINE[number]);

            return (
              <tr
                key={route.path}
                className={cn(
                  'border-b hover:bg-muted/30',
                  route.issues.length > 0 && 'bg-amber-50/50 dark:bg-amber-950/20',
                  route.status === 'deprecated' && 'opacity-60',
                  isSpine && 'border-l-2 border-l-primary'
                )}
              >
                <td className="p-2">
                  <div className="flex items-center gap-1">
                    <code className="font-mono text-xs">{route.path}</code>
                    <CopyButton text={route.path} />
                  </div>
                </td>
                <td className="p-2">
                  {route.navTitle ? (
                    <span>{route.navTitle}</span>
                  ) : (
                    <span className="text-muted-foreground italic">—</span>
                  )}
                </td>
                <td className="p-2">
                  <Badge className={SECTION_COLORS[route.section]}>{route.section}</Badge>
                </td>
                <td className="p-2">
                  <div className="flex flex-col gap-1">
                    <Badge className={TYPE_BADGES[route.type].className}>
                      {TYPE_BADGES[route.type].label}
                    </Badge>
                    {route.isHub && <HubValidation route={route} />}
                  </div>
                </td>
                <td className="p-2">
                  {route.status !== 'active' && (
                    <Badge className={STATUS_BADGES[route.status].className}>
                      {STATUS_BADGES[route.status].label}
                    </Badge>
                  )}
                </td>
                <td className="p-2">
                  <div className="flex items-center gap-1">
                    <ReachIcon className="h-3 w-3" />
                    <Badge className={REACHABILITY_BADGES[route.reachability].className}>
                      {REACHABILITY_BADGES[route.reachability].label}
                    </Badge>
                  </div>
                </td>
                {showExtended && (
                  <td className="p-2">
                    {route.componentFile !== 'unknown' ? (
                      <div className="flex items-center gap-1">
                        <code className="font-mono text-xs text-muted-foreground truncate max-w-[200px]">
                          {route.componentFile}
                        </code>
                        <CopyButton text={`src/${route.componentFile}`} label="path" />
                      </div>
                    ) : (
                      <span className="text-muted-foreground">—</span>
                    )}
                  </td>
                )}
                {showExtended && (
                  <td className="p-2">
                    {route.minRole ? (
                      <div className="flex items-center gap-1">
                        <Lock className="h-3 w-3" />
                        <span className="text-xs">{route.minRole}</span>
                      </div>
                    ) : (
                      <span className="text-muted-foreground">—</span>
                    )}
                  </td>
                )}
                <td className="p-2">
                  {route.issues.length > 0 ? (
                    <div className="flex items-start gap-1">
                      <AlertTriangle className="h-3 w-3 text-amber-500 mt-0.5 flex-shrink-0" />
                      <div className="text-xs text-amber-600 dark:text-amber-400 space-y-0.5">
                        {route.issues.map((issue, i) => (
                          <div key={i}>{issue}</div>
                        ))}
                      </div>
                    </div>
                  ) : (
                    <CheckCircle className="h-3 w-3 text-green-500" />
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function GroupedView({ routes }: { routes: RouteManifestEntry[] }) {
  const [collapsedSections, setCollapsedSections] = useState<Set<string>>(new Set());

  const grouped = useMemo(() => {
    const sections = new Map<RouteSection, RouteManifestEntry[]>();
    for (const route of routes) {
      const list = sections.get(route.section) || [];
      list.push(route);
      sections.set(route.section, list);
    }
    return Array.from(sections.entries()).sort(([a], [b]) => a.localeCompare(b));
  }, [routes]);

  const toggleSection = (section: string) => {
    setCollapsedSections(prev => {
      const next = new Set(prev);
      if (next.has(section)) {
        next.delete(section);
      } else {
        next.add(section);
      }
      return next;
    });
  };

  return (
    <div className="space-y-4">
      {grouped.map(([section, sectionRoutes]) => {
        const isCollapsed = collapsedSections.has(section);
        const issueCount = sectionRoutes.filter(r => r.issues.length > 0).length;
        const deprecatedCount = sectionRoutes.filter(r => r.status === 'deprecated').length;
        const draftCount = sectionRoutes.filter(r => r.status === 'draft').length;

        return (
          <div key={section} className="border rounded-lg overflow-hidden">
            <button
              onClick={() => toggleSection(section)}
              className={cn(
                'w-full flex items-center gap-2 p-3 hover:bg-muted/50 text-left',
                SECTION_COLORS[section]
              )}
            >
              {isCollapsed ? (
                <ChevronRight className="h-4 w-4" />
              ) : (
                <ChevronDown className="h-4 w-4" />
              )}
              <span className="font-medium flex-1">{section}</span>
              <span className="text-sm opacity-75">{sectionRoutes.length} routes</span>
              {issueCount > 0 && (
                <Badge className="bg-amber-500 text-white">{issueCount} issues</Badge>
              )}
              {deprecatedCount > 0 && (
                <Badge className="bg-red-500 text-white">{deprecatedCount} deprecated</Badge>
              )}
              {draftCount > 0 && (
                <Badge className="bg-yellow-500 text-white">{draftCount} draft</Badge>
              )}
            </button>
            {!isCollapsed && (
              <div className="bg-background">
                <RouteTable routes={sectionRoutes} showExtended />
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

function SpineOverview() {
  const manifest = useMemo(() => getRouteManifest(), []);

  return (
    <div className="border rounded-lg p-4 space-y-3">
      <h3 className="text-sm font-medium text-muted-foreground">Primary Spine</h3>
      <div className="flex flex-wrap gap-2">
        {PRIMARY_SPINE.map(path => {
          const route = manifest.find(r => r.path === path);
          if (!route) return null;
          return (
            <div
              key={path}
              className={cn(
                'flex items-center gap-2 px-3 py-1.5 rounded-lg border',
                route.issues.length > 0 ? 'border-amber-500' : 'border-green-500'
              )}
            >
              <Badge className={SECTION_COLORS[route.section]}>{route.section}</Badge>
              <span className="text-sm font-medium">{route.navTitle || path}</span>
              {route.issues.length > 0 && (
                <AlertTriangle className="h-3 w-3 text-amber-500" />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

export default function RoutesDebugPage() {
  const [viewMode, setViewMode] = useState<ViewMode>('table');
  const [filterMode, setFilterMode] = useState<FilterMode>('all');
  const [searchTerm, setSearchTerm] = useState('');
  const [showExtended, setShowExtended] = useState(true);

  const manifest = useMemo(() => getRouteManifest(), []);
  const stats = useMemo(() => getManifestStats(), []);

  const filteredRoutes = useMemo(() => {
    let routes = manifest;

    // Apply filter mode
    switch (filterMode) {
      case 'issues':
        routes = routes.filter(r => r.issues.length > 0);
        break;
      case 'orphans':
        routes = routes.filter(r => r.reachability === 'orphan');
        break;
      case 'hidden':
        routes = routes.filter(r => r.reachability === 'hidden');
        break;
      case 'duplicates':
        routes = routes.filter(r => r.issues.some(i => i.includes('duplicate') || i.includes('similar')));
        break;
      case 'hubs':
        routes = routes.filter(r => r.isHub);
        break;
      case 'deprecated':
        routes = routes.filter(r => r.status === 'deprecated');
        break;
      case 'draft':
        routes = routes.filter(r => r.status === 'draft');
        break;
    }

    // Apply search
    if (searchTerm) {
      const term = searchTerm.toLowerCase();
      routes = routes.filter(r =>
        r.path.toLowerCase().includes(term) ||
        r.navTitle?.toLowerCase().includes(term) ||
        r.section.toLowerCase().includes(term) ||
        r.componentFile.toLowerCase().includes(term)
      );
    }

    return routes;
  }, [manifest, filterMode, searchTerm]);

  return (
    <PageWrapper
      pageKey="routes-manifest"
      title="Routes Manifest"
      description={`Dev-only route inventory. ${stats.total} total routes registered.`}
      maxWidth="xl"
    >
      <div className="space-y-6">
        {/* Header */}
        <div>
          <h1 className="text-2xl font-bold">Routes Manifest</h1>
          <p className="text-muted-foreground">
            Dev-only route inventory. {stats.total} total routes registered.
          </p>
        </div>

        {/* Stats Grid */}
        <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-8 gap-3">
          <StatCard label="Total Routes" value={stats.total} onClick={() => setFilterMode('all')} />
          <StatCard label="In Sidebar" value={stats.inSidebar} />
          <StatCard label="Hub Pages" value={stats.hubs} onClick={() => setFilterMode('hubs')} />
          <StatCard
            label="With Issues"
            value={stats.withIssues}
            highlight={stats.withIssues > 0}
            onClick={() => setFilterMode('issues')}
          />
          <StatCard
            label="Orphans"
            value={stats.orphans}
            highlight={stats.orphans > 0}
            onClick={() => setFilterMode('orphans')}
          />
          <StatCard
            label="Hidden"
            value={stats.hidden}
            onClick={() => setFilterMode('hidden')}
          />
          <StatCard
            label="Draft"
            value={stats.byStatus.draft}
            highlight={stats.byStatus.draft > 0}
            onClick={() => setFilterMode('draft')}
          />
          <StatCard
            label="Deprecated"
            value={stats.byStatus.deprecated}
            highlight={stats.byStatus.deprecated > 0}
            onClick={() => setFilterMode('deprecated')}
          />
        </div>

        {/* Primary Spine */}
        <SpineOverview />

        {/* Section breakdown */}
        <div className="flex flex-wrap gap-2">
          {Object.entries(stats.bySection)
            .filter(([, count]) => count > 0)
            .sort(([, a], [, b]) => b - a)
            .map(([section, count]) => (
              <Badge key={section} className={SECTION_COLORS[section as RouteSection]}>
                {section}: {count}
              </Badge>
            ))}
        </div>

        {/* Reachability breakdown */}
        <div className="flex flex-wrap gap-2">
          {Object.entries(stats.byReachability)
            .filter(([, count]) => count > 0)
            .map(([reach, count]) => {
              const r = reach as Reachability;
              const Icon = REACHABILITY_BADGES[r].icon;
              return (
                <div key={reach} className="flex items-center gap-1">
                  <Icon className="h-3 w-3" />
                  <Badge className={REACHABILITY_BADGES[r].className}>
                    {reach}: {count}
                  </Badge>
                </div>
              );
            })}
        </div>

        {/* Controls */}
        <div className="flex flex-wrap items-center gap-4">
          {/* View mode toggle */}
          <div className="flex items-center border rounded-lg overflow-hidden">
            <button
              onClick={() => setViewMode('table')}
              className={cn(
                'p-2',
                viewMode === 'table' ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'
              )}
              title="Table view"
            >
              <List className="h-4 w-4" />
            </button>
            <button
              onClick={() => setViewMode('grouped')}
              className={cn(
                'p-2',
                viewMode === 'grouped' ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'
              )}
              title="Grouped view"
            >
              <LayoutGrid className="h-4 w-4" />
            </button>
          </div>

          {/* Extended columns toggle */}
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={showExtended}
              onChange={(e) => setShowExtended(e.target.checked)}
              className="rounded"
            />
            Show component files
          </label>

          {/* Filter mode */}
          <div className="flex items-center gap-2">
            <Filter className="h-4 w-4 text-muted-foreground" />
            <select
              value={filterMode}
              onChange={(e) => setFilterMode(e.target.value as FilterMode)}
              className="border rounded px-2 py-1 text-sm bg-background"
            >
              <option value="all">All routes</option>
              <option value="issues">With issues</option>
              <option value="orphans">Orphans only</option>
              <option value="hidden">Hidden only</option>
              <option value="duplicates">Duplicates only</option>
              <option value="hubs">Hubs only</option>
              <option value="deprecated">Deprecated only</option>
              <option value="draft">Draft only</option>
            </select>
          </div>

          {/* Search */}
          <input
            type="text"
            placeholder="Search routes..."
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            className="border rounded px-3 py-1 text-sm bg-background flex-1 max-w-xs"
          />

          <span className="text-sm text-muted-foreground">
            Showing {filteredRoutes.length} of {manifest.length}
          </span>
        </div>

        {/* Main content */}
        <div className="border rounded-lg overflow-hidden">
          {viewMode === 'table' ? (
            <RouteTable routes={filteredRoutes} showExtended={showExtended} />
          ) : (
            <GroupedView routes={filteredRoutes} />
          )}
        </div>

        {/* Flow validation */}
        <div className="border rounded-lg p-4">
          <FlowValidator />
        </div>

        {/* Legend */}
        <div className="border rounded-lg p-4 space-y-4">
          <h3 className="text-sm font-medium text-muted-foreground">Legend</h3>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-6 text-xs">
            <div className="space-y-2">
              <div className="font-medium">Route Types:</div>
              {Object.entries(TYPE_BADGES).map(([type, { label, className }]) => (
                <div key={type} className="flex items-center gap-2">
                  <Badge className={className}>{label}</Badge>
                  <span className="text-muted-foreground capitalize">{type}</span>
                </div>
              ))}
            </div>
            <div className="space-y-2">
              <div className="font-medium">Status:</div>
              {Object.entries(STATUS_BADGES).map(([status, { label, className }]) => (
                <div key={status} className="flex items-center gap-2">
                  <Badge className={className}>{label}</Badge>
                  <span className="text-muted-foreground capitalize">{status}</span>
                </div>
              ))}
            </div>
            <div className="space-y-2">
              <div className="font-medium">Reachability:</div>
              {Object.entries(REACHABILITY_BADGES).map(([reach, { label, className, icon: Icon }]) => (
                <div key={reach} className="flex items-center gap-2">
                  <Icon className="h-3 w-3" />
                  <Badge className={className}>{label}</Badge>
                  <span className="text-muted-foreground">
                    {reach === 'primary' && '= in sidebar'}
                    {reach === 'nested' && '= via parent/tabs'}
                    {reach === 'hidden' && '= direct URL only'}
                    {reach === 'orphan' && '= unreachable'}
                  </span>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </PageWrapper>
  );
}
