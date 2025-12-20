import { useState, useMemo } from 'react';
import FeatureLayout from '@/layout/FeatureLayout';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import {
  useErrorStore,
  type ErrorCategory,
  type CapturedError,
} from '@/stores/errorStore';
import {
  WifiOff,
  Shield,
  HardDrive,
  Box,
  GraduationCap,
  Database,
  Upload,
  Zap,
  Server,
  Monitor,
  HelpCircle,
  Trash2,
  X,
  ChevronDown,
  ChevronRight,
  AlertTriangle,
  Bug,
  Clock,
  Copy,
  Check,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { toast } from 'sonner';
import { formatRelativeTime } from '@/lib/formatters';

// Category configuration
const CATEGORY_CONFIG: Record<
  ErrorCategory,
  { label: string; icon: typeof WifiOff; color: string; description: string }
> = {
  network: {
    label: 'Network',
    icon: WifiOff,
    color: 'text-orange-500',
    description: 'Connection timeouts, fetch failures, network errors',
  },
  auth: {
    label: 'Authentication',
    icon: Shield,
    color: 'text-yellow-500',
    description: 'Login failures, session expiry, permission denied',
  },
  resource: {
    label: 'Resources',
    icon: HardDrive,
    color: 'text-purple-500',
    description: 'Memory limits, disk space, resource contention',
  },
  adapter: {
    label: 'Adapters',
    icon: Box,
    color: 'text-blue-500',
    description: 'Load failures, corruption, not found errors',
  },
  training: {
    label: 'Training',
    icon: GraduationCap,
    color: 'text-green-500',
    description: 'Job failures, invalid data, training timeouts',
  },
  model: {
    label: 'Models',
    icon: Database,
    color: 'text-cyan-500',
    description: 'Model loading, busy states, not found errors',
  },
  upload: {
    label: 'Uploads',
    icon: Upload,
    color: 'text-pink-500',
    description: 'File size limits, format errors, upload failures',
  },
  inference: {
    label: 'Inference',
    icon: Zap,
    color: 'text-amber-500',
    description: 'Generation failures, invalid prompts',
  },
  server: {
    label: 'Server',
    icon: Server,
    color: 'text-red-500',
    description: '5xx errors, service unavailable, maintenance',
  },
  ui: {
    label: 'UI/React',
    icon: Monitor,
    color: 'text-indigo-500',
    description: 'Render errors, hook failures, component crashes',
  },
  unknown: {
    label: 'Unknown',
    icon: HelpCircle,
    color: 'text-gray-500',
    description: 'Uncategorized errors',
  },
};

// All categories in display order
const CATEGORIES: ErrorCategory[] = [
  'network',
  'auth',
  'server',
  'adapter',
  'model',
  'training',
  'inference',
  'upload',
  'resource',
  'ui',
  'unknown',
];

// Local timestamp formatter for this specific format
function formatTimestamp(date: Date): string {
  return date.toLocaleTimeString('en-US', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

interface ErrorItemProps {
  error: CapturedError;
  onDismiss: (id: string) => void;
}

function ErrorItem({ error, onDismiss }: ErrorItemProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const config = CATEGORY_CONFIG[error.category];
  const Icon = config.icon;

  const handleCopy = async () => {
    const text = JSON.stringify(
      {
        message: error.message,
        code: error.code,
        category: error.category,
        timestamp: error.timestamp,
        stack: error.stack,
        context: error.context,
      },
      null,
      2
    );
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
    toast.success('Error details copied to clipboard');
  };

  return (
    <Collapsible open={isOpen} onOpenChange={setIsOpen}>
      <div
        className={cn(
          'border rounded-lg transition-colors',
          error.dismissed ? 'opacity-50 bg-muted/30' : 'bg-card',
          isOpen && 'ring-1 ring-primary/20'
        )}
      >
        <CollapsibleTrigger asChild>
          <div className="flex items-start gap-3 p-3 cursor-pointer hover:bg-muted/50 transition-colors">
            <div className={cn('mt-0.5', config.color)}>
              <Icon className="h-4 w-4" />
            </div>

            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2 mb-1">
                {error.code && (
                  <Badge variant="outline" className="text-xs font-mono">
                    {error.code}
                  </Badge>
                )}
                {error.httpStatus && (
                  <Badge variant="secondary" className="text-xs">
                    HTTP {error.httpStatus}
                  </Badge>
                )}
                {error.component && (
                  <Badge variant="outline" className="text-xs text-muted-foreground">
                    {error.component}
                  </Badge>
                )}
              </div>

              <p className="text-sm font-medium truncate pr-4">{error.message}</p>

              <div className="flex items-center gap-2 mt-1 text-xs text-muted-foreground">
                <Clock className="h-3 w-3" />
                <span>{formatRelativeTime(error.timestamp)}</span>
                <span className="text-muted-foreground/50">
                  ({formatTimestamp(error.timestamp)})
                </span>
              </div>
            </div>

            <div className="flex items-center gap-1">
              {isOpen ? (
                <ChevronDown className="h-4 w-4 text-muted-foreground" />
              ) : (
                <ChevronRight className="h-4 w-4 text-muted-foreground" />
              )}
            </div>
          </div>
        </CollapsibleTrigger>

        <CollapsibleContent>
          <div className="border-t px-3 py-3 space-y-3 bg-muted/20">
            {/* Stack trace */}
            {error.stack && (
              <div>
                <div className="text-xs font-medium text-muted-foreground mb-1">
                  Stack Trace
                </div>
                <ScrollArea className="h-32">
                  <pre className="text-xs font-mono bg-background p-2 rounded border overflow-x-auto whitespace-pre-wrap break-all">
                    {error.stack}
                  </pre>
                </ScrollArea>
              </div>
            )}

            {/* Context */}
            {error.context && Object.keys(error.context).length > 0 && (
              <div>
                <div className="text-xs font-medium text-muted-foreground mb-1">
                  Context
                </div>
                <pre className="text-xs font-mono bg-background p-2 rounded border overflow-x-auto">
                  {JSON.stringify(error.context, null, 2)}
                </pre>
              </div>
            )}

            {/* Actions */}
            <div className="flex items-center gap-2 pt-2">
              <Button variant="ghost" size="sm" onClick={handleCopy}>
                {copied ? (
                  <Check className="h-3 w-3 mr-1" />
                ) : (
                  <Copy className="h-3 w-3 mr-1" />
                )}
                Copy
              </Button>
              {!error.dismissed && (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => onDismiss(error.id)}
                >
                  <X className="h-3 w-3 mr-1" />
                  Dismiss
                </Button>
              )}
            </div>
          </div>
        </CollapsibleContent>
      </div>
    </Collapsible>
  );
}

interface CategorySectionProps {
  category: ErrorCategory;
  errors: CapturedError[];
  onDismiss: (id: string) => void;
  onDismissCategory: (category: ErrorCategory) => void;
}

function CategorySection({
  category,
  errors,
  onDismiss,
  onDismissCategory,
}: CategorySectionProps) {
  const config = CATEGORY_CONFIG[category];
  const Icon = config.icon;
  const activeErrors = errors.filter((e) => !e.dismissed);
  const dismissedCount = errors.length - activeErrors.length;

  if (errors.length === 0) {
    return null;
  }

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Icon className={cn('h-5 w-5', config.color)} />
            <CardTitle className="text-base">{config.label}</CardTitle>
            <Badge variant={activeErrors.length > 0 ? 'destructive' : 'secondary'}>
              {activeErrors.length}
            </Badge>
            {dismissedCount > 0 && (
              <Badge variant="outline" className="text-muted-foreground">
                +{dismissedCount} dismissed
              </Badge>
            )}
          </div>
          {activeErrors.length > 0 && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => onDismissCategory(category)}
            >
              Dismiss All
            </Button>
          )}
        </div>
        <CardDescription>{config.description}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-2">
        {errors.map((error) => (
          <ErrorItem key={error.id} error={error} onDismiss={onDismiss} />
        ))}
      </CardContent>
    </Card>
  );
}

export default function DevErrorsPage() {
  const {
    errors,
    dismissError,
    dismissCategory,
    clearAll,
    clearDismissed,
    getCategoryCounts,
    getActiveCount,
  } = useErrorStore();

  const [activeTab, setActiveTab] = useState<'all' | ErrorCategory>('all');
  const [showDismissed, setShowDismissed] = useState(true);

  const counts = useMemo(() => getCategoryCounts(), [getCategoryCounts]);
  const activeCount = useMemo(() => getActiveCount(), [getActiveCount]);
  const totalCount = errors.length;

  // Group errors by category
  const errorsByCategory = useMemo(() => {
    const grouped: Record<ErrorCategory, CapturedError[]> = {
      network: [],
      auth: [],
      resource: [],
      adapter: [],
      training: [],
      model: [],
      upload: [],
      inference: [],
      server: [],
      ui: [],
      unknown: [],
    };

    for (const error of errors) {
      if (showDismissed || !error.dismissed) {
        grouped[error.category].push(error);
      }
    }

    return grouped;
  }, [errors, showDismissed]);

  // Categories with errors
  const categoriesWithErrors = CATEGORIES.filter(
    (cat) => errorsByCategory[cat].length > 0
  );

  // Filtered errors for current tab
  const filteredErrors =
    activeTab === 'all'
      ? errors.filter((e) => showDismissed || !e.dismissed)
      : errorsByCategory[activeTab];

  return (
    <FeatureLayout
      title="Error Inspector"
      description="Development-only error tracking and debugging"
      headerActions={
        <div className="flex items-center gap-2">
          <Badge variant="outline" className="gap-1">
            <Bug className="h-3 w-3" />
            DEV MODE
          </Badge>
        </div>
      }
    >
      <div className="space-y-6">
        {/* Summary Cards */}
        <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-4">
          {CATEGORIES.slice(0, 6).map((category) => {
            const config = CATEGORY_CONFIG[category];
            const Icon = config.icon;
            const count = counts[category];
            return (
              <Card
                key={category}
                className={cn(
                  'cursor-pointer transition-all hover:ring-1 hover:ring-primary/50',
                  activeTab === category && 'ring-2 ring-primary'
                )}
                onClick={() => setActiveTab(category)}
              >
                <CardContent className="p-4">
                  <div className="flex items-center justify-between">
                    <Icon className={cn('h-5 w-5', config.color)} />
                    <span className="text-2xl font-bold">{count}</span>
                  </div>
                  <div className="text-xs text-muted-foreground mt-1">
                    {config.label}
                  </div>
                </CardContent>
              </Card>
            );
          })}
        </div>

        {/* Controls */}
        <Card>
          <CardContent className="p-4">
            <div className="flex flex-wrap items-center justify-between gap-4">
              <div className="flex items-center gap-4">
                <div className="flex items-center gap-2">
                  <AlertTriangle className="h-4 w-4 text-destructive" />
                  <span className="font-medium">{activeCount} active</span>
                  <span className="text-muted-foreground">
                    / {totalCount} total
                  </span>
                </div>
                <label className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={showDismissed}
                    onChange={(e) => setShowDismissed(e.target.checked)}
                    className="rounded"
                  />
                  Show dismissed
                </label>
              </div>

              <div className="flex items-center gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={clearDismissed}
                  disabled={errors.filter((e) => e.dismissed).length === 0}
                >
                  Clear Dismissed
                </Button>
                <Button
                  variant="destructive"
                  size="sm"
                  onClick={clearAll}
                  disabled={errors.length === 0}
                >
                  <Trash2 className="h-4 w-4 mr-1" />
                  Clear All
                </Button>
              </div>
            </div>
          </CardContent>
        </Card>

        {/* Tabs for categories */}
        <Tabs value={activeTab} onValueChange={(v) => setActiveTab(v as typeof activeTab)}>
          <TabsList className="flex-wrap h-auto gap-1">
            <TabsTrigger value="all" className="gap-1">
              All
              <Badge variant="secondary" className="ml-1">
                {filteredErrors.length}
              </Badge>
            </TabsTrigger>
            {categoriesWithErrors.map((category) => {
              const config = CATEGORY_CONFIG[category];
              const Icon = config.icon;
              return (
                <TabsTrigger key={category} value={category} className="gap-1">
                  <Icon className={cn('h-3 w-3', config.color)} />
                  {config.label}
                  <Badge variant="secondary" className="ml-1">
                    {errorsByCategory[category].length}
                  </Badge>
                </TabsTrigger>
              );
            })}
          </TabsList>

          <TabsContent value="all" className="mt-4 space-y-4">
            {categoriesWithErrors.length === 0 ? (
              <Card>
                <CardContent className="py-12 text-center">
                  <Check className="h-12 w-12 mx-auto text-green-500 mb-4" />
                  <h3 className="text-lg font-medium">No errors captured</h3>
                  <p className="text-muted-foreground mt-1">
                    API errors will appear here automatically as they occur.
                  </p>
                </CardContent>
              </Card>
            ) : (
              categoriesWithErrors.map((category) => (
                <CategorySection
                  key={category}
                  category={category}
                  errors={errorsByCategory[category]}
                  onDismiss={dismissError}
                  onDismissCategory={dismissCategory}
                />
              ))
            )}
          </TabsContent>

          {CATEGORIES.map((category) => (
            <TabsContent key={category} value={category} className="mt-4">
              {errorsByCategory[category].length === 0 ? (
                <Card>
                  <CardContent className="py-12 text-center">
                    <Check className="h-12 w-12 mx-auto text-green-500 mb-4" />
                    <h3 className="text-lg font-medium">
                      No {CATEGORY_CONFIG[category].label.toLowerCase()} errors
                    </h3>
                  </CardContent>
                </Card>
              ) : (
                <CategorySection
                  category={category}
                  errors={errorsByCategory[category]}
                  onDismiss={dismissError}
                  onDismissCategory={dismissCategory}
                />
              )}
            </TabsContent>
          ))}
        </Tabs>
      </div>
    </FeatureLayout>
  );
}
