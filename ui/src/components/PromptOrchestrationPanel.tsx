import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Switch } from './ui/switch';
import { Slider } from './ui/slider';
import { Badge } from './ui/badge';
import { Separator } from './ui/separator';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Alert, AlertDescription } from './ui/alert';
import { toast } from 'sonner';
import { logger, toError } from '@/utils/logger';
import { apiClient } from '@/api/services';
import {
  LegacyOrchestrationMetrics,
  PromptAnalysisResult
} from '@/api/types';

// Local interface for prompt orchestration config
// [source: ui/src/components/PromptOrchestrationPanel.tsx L15-L30]
// Note: api/api-types.ts has conflicting OrchestrationConfig definitions
// This local interface matches the prompt orchestration use case
interface PromptOrchestrationConfig {
  enabled: boolean;
  baseModelThreshold: number; // Minimum complexity score to use adapters
  adapterThreshold: number; // Minimum score to qualify adapters
  analysisTimeout: number; // Max time for prompt analysis in ms
  cacheEnabled: boolean;
  cacheTtl: number; // Cache TTL in seconds
  enableTelemetry: boolean;
  fallbackStrategy: 'base_only' | 'best_effort' | 'adaptive';
}
import {
  Brain,
  Zap,
  Target,
  Settings,
  BarChart3,
  PlayCircle,
  RefreshCw,
  AlertTriangle,
  CheckCircle,
  TrendingUp,
  MessageSquare,
  Activity
} from 'lucide-react';

// [source: ui/src/components/PromptOrchestrationPanel.tsx L12-L20]
// LocalStorage keys for graceful degradation while backend endpoints are pending
// Backend endpoint /v1/orchestration/config is planned but not yet implemented
const ORCHESTRATION_CONFIG_KEY = 'aos:orchestration:config';
const ORCHESTRATION_ANALYSES_KEY = 'aos:orchestration:analyses';

// Client-side heuristic analysis patterns
// [source: CLAUDE.md - Policy Pack #9 (Telemetry)]
const LANGUAGE_PATTERNS: Record<string, RegExp> = {
  rust: /\b(fn|impl|struct|enum|trait|pub|mod|use|let\s+mut|async|await|match|Option|Result)\b/i,
  typescript: /\b(interface|type|enum|namespace|readonly|as\s+const|import\s+type)\b/i,
  javascript: /\b(const|let|var|function|class|extends|import|export|async|await|=>)\b/i,
  python: /\b(def|class|import|from|self|async|await|lambda|yield|with|except|raise)\b/i,
  go: /\b(func|package|import|type|struct|interface|go|chan|defer|select)\b/i,
  java: /\b(public|private|protected|class|interface|extends|implements|throws)\b/i,
  sql: /\b(SELECT|INSERT|UPDATE|DELETE|FROM|WHERE|JOIN|CREATE|ALTER|DROP)\b/i,
};

const FRAMEWORK_PATTERNS: Record<string, RegExp> = {
  react: /\b(useState|useEffect|useCallback|useMemo|useRef|useContext|React|JSX|tsx)\b/i,
  vue: /\b(ref|reactive|computed|watch|v-if|v-for|v-model|defineComponent)\b/i,
  angular: /\b(@Component|@Injectable|@NgModule|Observable|BehaviorSubject)\b/i,
  express: /\b(app\.get|app\.post|req|res|middleware|router)\b/i,
  django: /\b(models\.Model|views\.|urls\.py|migrations|queryset)\b/i,
  fastapi: /\b(FastAPI|@app\.(get|post)|Depends|HTTPException|BaseModel)\b/i,
  tokio: /\b(tokio::|#\[tokio::main\]|spawn|async_trait)\b/i,
  axum: /\b(axum::|Router|Extension|Json|extract)\b/i,
};

const VERB_PATTERNS: Record<string, RegExp> = {
  implement: /\b(implement|create|build|add|make|write|develop)\b/i,
  fix: /\b(fix|repair|resolve|debug|solve|correct|patch)\b/i,
  refactor: /\b(refactor|improve|optimize|enhance|clean|restructure)\b/i,
  explain: /\b(explain|describe|what|why|how|understand|clarify)\b/i,
  review: /\b(review|check|analyze|audit|inspect|validate)\b/i,
  test: /\b(test|verify|assert|spec|unit|integration|e2e)\b/i,
};

// Default configuration for graceful degradation
// [source: CLAUDE.md - Policy Pack #1 (Egress) - use local state when backend unavailable]
const DEFAULT_CONFIG: PromptOrchestrationConfig = {
  enabled: true,
  baseModelThreshold: 0.2,
  adapterThreshold: 0.1,
  analysisTimeout: 50,
  cacheEnabled: true,
  cacheTtl: 300,
  enableTelemetry: true,
  fallbackStrategy: 'adaptive'
};

// Client-side heuristic prompt analysis
// [source: ui/src/components/PromptOrchestrationPanel.tsx L90-L150]
// This provides basic analysis while backend /v1/orchestration/analyze is pending
function analyzePromptLocally(prompt: string): PromptAnalysisResult {
  const startTime = performance.now();

  // Estimate token count (rough approximation: ~4 chars per token)
  const tokens = Math.ceil(prompt.length / 4);

  // Count code symbols (brackets, semicolons, etc.)
  const symbolMatches = prompt.match(/[{}\[\]();:=<>]/g);
  const symbols = symbolMatches ? symbolMatches.length : 0;

  // Detect programming language
  let detectedLanguage = 'natural';
  for (const [lang, pattern] of Object.entries(LANGUAGE_PATTERNS)) {
    if (pattern.test(prompt)) {
      detectedLanguage = lang;
      break;
    }
  }

  // Detect frameworks
  const detectedFrameworks: string[] = [];
  for (const [framework, pattern] of Object.entries(FRAMEWORK_PATTERNS)) {
    if (pattern.test(prompt)) {
      detectedFrameworks.push(framework);
    }
  }

  // Detect primary verb/action
  let detectedVerb = 'general';
  for (const [verb, pattern] of Object.entries(VERB_PATTERNS)) {
    if (pattern.test(prompt)) {
      detectedVerb = verb;
      break;
    }
  }

  // Calculate complexity score based on heuristics
  // [source: CLAUDE.md - K-Sparse Routing concept]
  let complexityScore = 0;

  // Token count factor (0.0-0.3)
  complexityScore += Math.min(tokens / 500, 0.3);

  // Code detection factor (0.0-0.3)
  if (detectedLanguage !== 'natural') {
    complexityScore += 0.2;
  }
  complexityScore += Math.min(symbols / 50, 0.1);

  // Framework detection factor (0.0-0.2)
  complexityScore += Math.min(detectedFrameworks.length * 0.1, 0.2);

  // Action type factor (0.0-0.2)
  if (['implement', 'refactor', 'fix'].includes(detectedVerb)) {
    complexityScore += 0.15;
  } else if (['review', 'test'].includes(detectedVerb)) {
    complexityScore += 0.1;
  }

  // Clamp to 0.0-1.0
  complexityScore = Math.min(Math.max(complexityScore, 0), 1);

  // Determine recommended strategy based on complexity
  let recommendedStrategy: 'base_model' | 'adapters' | 'mixed';
  if (complexityScore < 0.2) {
    recommendedStrategy = 'base_model';
  } else if (complexityScore > 0.6) {
    recommendedStrategy = 'adapters';
  } else {
    recommendedStrategy = 'mixed';
  }

  const analysisTimeMs = Math.round(performance.now() - startTime);

  return {
    prompt,
    complexityScore,
    recommendedStrategy,
    analysisTimeMs,
    features: {
      language: detectedLanguage,
      frameworks: detectedFrameworks,
      symbols,
      tokens,
      verb: detectedVerb,
    },
    timestamp: new Date().toISOString(),
  };
}

export default function PromptOrchestrationPanel() {
  const [config, setConfig] = useState<PromptOrchestrationConfig>({
    enabled: true,
    baseModelThreshold: 0.2,
    adapterThreshold: 0.1,
    analysisTimeout: 50,
    cacheEnabled: true,
    cacheTtl: 300,
    enableTelemetry: true,
    fallbackStrategy: 'adaptive'
  });

  const [metrics, setMetrics] = useState<LegacyOrchestrationMetrics | null>(null);
  const [orchestrationUnavailable, setOrchestrationUnavailable] = useState(false);
  const [unavailableReason, setUnavailableReason] = useState<string | null>(null);

  const [recentAnalyses, setRecentAnalyses] = useState<PromptAnalysisResult[]>([]);
  const [isLoadingMetrics, setIsLoadingMetrics] = useState(true);

  const [isLoading, setIsLoading] = useState(false);
  const [testPrompt, setTestPrompt] = useState('');
  const [testResult, setTestResult] = useState<PromptAnalysisResult | null>(null);

  const markOrchestrationUnavailable = useCallback((reason: string) => {
    setOrchestrationUnavailable(true);
    setUnavailableReason(reason);
  }, []);

  const clearOrchestrationUnavailable = useCallback(() => {
    setOrchestrationUnavailable(false);
    setUnavailableReason(null);
  }, []);

  // Load configuration with graceful degradation
  // [source: ui/src/components/PromptOrchestrationPanel.tsx L200-L240]
  // Backend endpoint /v1/orchestration/config is planned but not yet implemented
  const loadConfig = useCallback(async () => {
    logger.info('Loading orchestration config', {
      component: 'PromptOrchestrationPanel',
      operation: 'loadConfig',
    });

    // Try localStorage first (graceful degradation)
    try {
      const stored = localStorage.getItem(ORCHESTRATION_CONFIG_KEY);
      if (stored) {
        const parsed = JSON.parse(stored) as PromptOrchestrationConfig;
        setConfig(parsed);
        logger.debug('Loaded config from localStorage', {
          component: 'PromptOrchestrationPanel',
          operation: 'loadConfig',
        });
        return;
      }
    } catch (e) {
      logger.warn('Failed to parse stored config, using defaults', {
        component: 'PromptOrchestrationPanel',
        operation: 'loadConfig',
        error: toError(e),
      });
    }

    // Try backend endpoint (may not exist yet)
    try {
      const response = await apiClient.request<PromptOrchestrationConfig>(
        '/v1/orchestration/config',
        { method: 'GET' },
        true // skipRetry - endpoint may not exist
      );
      setConfig(response);
      clearOrchestrationUnavailable();
      // Cache to localStorage
      localStorage.setItem(ORCHESTRATION_CONFIG_KEY, JSON.stringify(response));
      logger.info('Loaded config from backend', {
        component: 'PromptOrchestrationPanel',
        operation: 'loadConfig',
      });
    } catch (e) {
      // Backend not available - use defaults
      logger.info('Backend /v1/orchestration/config not available, using defaults (endpoint pending)', {
        component: 'PromptOrchestrationPanel',
        operation: 'loadConfig',
      });
      markOrchestrationUnavailable('Orchestration APIs are not available on this backend (v0.9)');
      setConfig(DEFAULT_CONFIG);
    }

    // Load recent analyses from localStorage
    try {
      const storedAnalyses = localStorage.getItem(ORCHESTRATION_ANALYSES_KEY);
      if (storedAnalyses) {
        const parsed = JSON.parse(storedAnalyses) as PromptAnalysisResult[];
        setRecentAnalyses(parsed.slice(0, 10)); // Keep last 10
      }
    } catch (e) {
      logger.debug('No stored analyses found', {
        component: 'PromptOrchestrationPanel',
      });
    }
  }, []);

  // Save configuration with localStorage and optional backend sync
  // [source: ui/src/components/PromptOrchestrationPanel.tsx L268-L310]
  const saveConfig = async () => {
    setIsLoading(true);
    try {
      // Always save to localStorage for persistence
      localStorage.setItem(ORCHESTRATION_CONFIG_KEY, JSON.stringify(config));
      logger.info('Saved config to localStorage', {
        component: 'PromptOrchestrationPanel',
        operation: 'saveConfig',
        config,
      });

      // Try to sync with backend (may not exist yet)
      try {
        await apiClient.request<void>(
          '/v1/orchestration/config',
          {
            method: 'PUT',
            body: JSON.stringify(config),
          },
          true // skipRetry - endpoint may not exist
        );
        logger.info('Synced config to backend', {
          component: 'PromptOrchestrationPanel',
          operation: 'saveConfig',
        });
        toast.success('Configuration saved and synced');
      } catch (backendError) {
        // Backend not available - still success since localStorage worked
        logger.info('Backend sync pending - /v1/orchestration/config endpoint not available', {
          component: 'PromptOrchestrationPanel',
          operation: 'saveConfig',
        });
        markOrchestrationUnavailable('Cannot sync orchestration config: API not available on this backend (v0.9)');
        toast.success('Configuration saved locally (backend sync pending)');
      }
    } catch (error) {
      const err = toError(error);
      logger.error('Failed to save orchestration config', {
        component: 'PromptOrchestrationPanel',
        operation: 'saveConfig',
      }, err);
      toast.error('Failed to save configuration');
    }
    setIsLoading(false);
  };

  // Test prompt analysis with client-side heuristics and optional backend
  // [source: ui/src/components/PromptOrchestrationPanel.tsx L315-L380]
  const testPromptAnalysis = async () => {
    if (!testPrompt.trim()) return;

    setIsLoading(true);
    try {
      let result: PromptAnalysisResult;

      // Try backend analysis first (may not exist yet)
      if (!orchestrationUnavailable) {
        try {
          result = await apiClient.request<PromptAnalysisResult>(
            '/v1/orchestration/analyze',
            {
              method: 'POST',
              body: JSON.stringify({ prompt: testPrompt }),
            },
            true // skipRetry - endpoint may not exist
          );
          logger.info('Received analysis from backend', {
            component: 'PromptOrchestrationPanel',
            operation: 'testPromptAnalysis',
            complexityScore: result.complexityScore,
          });
          clearOrchestrationUnavailable();
        } catch (backendError) {
          // Backend not available - use client-side heuristic analysis
          logger.info('Using client-side heuristic analysis (backend /v1/orchestration/analyze pending)', {
            component: 'PromptOrchestrationPanel',
            operation: 'testPromptAnalysis',
          });
          markOrchestrationUnavailable('Prompt analysis API is not available on this backend (v0.9)');
          result = analyzePromptLocally(testPrompt);
        }
      } else {
        // Already know backend is unavailable – skip network
        result = analyzePromptLocally(testPrompt);
      }

      setTestResult(result);

      // Add to recent analyses and persist
      const updatedAnalyses = [result, ...recentAnalyses].slice(0, 10);
      setRecentAnalyses(updatedAnalyses);
      localStorage.setItem(ORCHESTRATION_ANALYSES_KEY, JSON.stringify(updatedAnalyses));

      toast.success(`Analysis complete: ${result.recommendedStrategy.replace('_', ' ')} strategy recommended`);
    } catch (error) {
      const err = toError(error);
      logger.error('Failed to analyze prompt', {
        component: 'PromptOrchestrationPanel',
        operation: 'testPromptAnalysis',
      }, err);
      toast.error('Failed to analyze prompt');
    }
    setIsLoading(false);
  };

  // Load orchestration metrics with graceful degradation
  // [source: ui/src/components/PromptOrchestrationPanel.tsx L367-L420]
  const loadMetrics = useCallback(async () => {
    try {
      // Try dedicated orchestration metrics endpoint first
      try {
        const orchestrationMetrics = await apiClient.request<LegacyOrchestrationMetrics>(
          '/v1/orchestration/metrics',
          { method: 'GET' },
          true // skipRetry - endpoint may not exist
        );
        setMetrics(orchestrationMetrics);
        clearOrchestrationUnavailable();
        logger.debug('Loaded orchestration metrics from dedicated endpoint', {
          component: 'PromptOrchestrationPanel',
          operation: 'loadMetrics',
        });
        return;
      } catch {
        // Dedicated endpoint not available, try general metrics
      }

      // Try to extract from general /v1/metrics endpoint
      try {
        const response = await apiClient.request<Record<string, unknown>>(
          '/v1/metrics',
          { method: 'GET' },
          true
        );

        // Extract orchestration-related metrics if available
        if (response && typeof response === 'object') {
          const orchestrationData = response['orchestration'] as LegacyOrchestrationMetrics | undefined;
          if (orchestrationData) {
            setMetrics(orchestrationData);
            clearOrchestrationUnavailable();
            logger.debug('Extracted orchestration metrics from /v1/metrics', {
              component: 'PromptOrchestrationPanel',
              operation: 'loadMetrics',
            });
            return;
          }
        }
      } catch {
        // General metrics endpoint also not available
      }

      // No backend metrics available - metrics remain null (shows pending state)
      logger.debug('Orchestration metrics endpoints not available (pending implementation)', {
        component: 'PromptOrchestrationPanel',
        operation: 'loadMetrics',
      });
      markOrchestrationUnavailable('Orchestration metrics API is not available on this backend (v0.9)');
    } catch (error) {
      logger.debug('Error loading orchestration metrics', {
        component: 'PromptOrchestrationPanel',
        operation: 'loadMetrics',
        error: toError(error),
      });
      markOrchestrationUnavailable('Orchestration metrics API is not available on this backend (v0.9)');
    }
    setIsLoadingMetrics(false);
  }, []);

  // Load config and metrics on mount, poll metrics periodically
  useEffect(() => {
    loadConfig();
    loadMetrics();

    // Poll metrics every 30 seconds when component is mounted
    const metricsInterval = setInterval(() => {
      loadMetrics();
    }, 30000);

    return () => {
      clearInterval(metricsInterval);
    };
  }, [loadConfig, loadMetrics]);

  const baseModelPercentage = metrics && metrics.totalRequests > 0
    ? (metrics.baseModelOnly / metrics.totalRequests) * 100
    : 0;
  const adapterPercentage = metrics && metrics.totalRequests > 0
    ? (metrics.adapterUsed / metrics.totalRequests) * 100
    : 0;
  const cacheHitRate = metrics && (metrics.cacheHits + metrics.cacheMisses) > 0
    ? (metrics.cacheHits / (metrics.cacheHits + metrics.cacheMisses)) * 100
    : 0;

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-gray-900">Prompt Orchestration</h2>
          <p className="text-gray-600 mt-1">
            Intelligent routing between base model and LoRA adapters based on prompt analysis
          </p>
        </div>
        <div className="flex items-center gap-4">
          <Badge variant={config.enabled ? "default" : "secondary"}>
            {config.enabled ? "Enabled" : "Disabled"}
          </Badge>
          <Button onClick={loadConfig} variant="outline" size="sm">
            <RefreshCw className="w-4 h-4 mr-2" />
            Refresh
          </Button>
        </div>
      </div>

      {orchestrationUnavailable && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertDescription>
            {unavailableReason || 'Orchestration APIs are not available on this backend (v0.9). The UI is running in local-only mode until the server supports /v1/orchestration endpoints.'}
          </AlertDescription>
        </Alert>
      )}

      <Tabs defaultValue="overview" className="space-y-4">
        <TabsList>
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="configuration">Configuration</TabsTrigger>
          <TabsTrigger value="testing">Testing</TabsTrigger>
          <TabsTrigger value="analytics">Analytics</TabsTrigger>
        </TabsList>

        <TabsContent value="overview" className="space-y-4">
          {/* Status Overview */}
          {isLoadingMetrics ? (
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
              {[...Array(4)].map((_, i) => (
                <Card key={i}>
                  <CardContent className="p-6">
                    <div className="animate-pulse">
                      <div className="h-4 bg-gray-200 rounded w-3/4 mb-2"></div>
                      <div className="h-8 bg-gray-200 rounded w-1/2"></div>
                    </div>
                  </CardContent>
                </Card>
              ))}
            </div>
          ) : !metrics ? (
            <Alert>
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>
                Orchestration metrics not available. The backend endpoint is under development.
              </AlertDescription>
            </Alert>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
              <Card>
                <CardContent className="p-6">
                  <div className="flex items-center">
                    <Activity className="h-4 w-4 text-blue-600" />
                    <div className="ml-4 space-y-1">
                      <p className="text-sm font-medium text-gray-600">Total Requests</p>
                      <p className="text-2xl font-bold text-gray-900">{metrics.totalRequests.toLocaleString()}</p>
                    </div>
                  </div>
                </CardContent>
              </Card>

              <Card>
                <CardContent className="p-6">
                  <div className="flex items-center">
                    <MessageSquare className="h-4 w-4 text-green-600" />
                    <div className="ml-4 space-y-1">
                      <p className="text-sm font-medium text-gray-600">Base Model Only</p>
                      <p className="text-2xl font-bold text-gray-900">{metrics.baseModelOnly.toLocaleString()}</p>
                      <p className="text-xs text-gray-500">{baseModelPercentage.toFixed(1)}%</p>
                    </div>
                  </div>
                </CardContent>
              </Card>

              <Card>
                <CardContent className="p-6">
                  <div className="flex items-center">
                    <Zap className="h-4 w-4 text-purple-600" />
                    <div className="ml-4 space-y-1">
                      <p className="text-sm font-medium text-gray-600">Adapters Used</p>
                      <p className="text-2xl font-bold text-gray-900">{metrics.adapterUsed.toLocaleString()}</p>
                      <p className="text-xs text-gray-500">{adapterPercentage.toFixed(1)}%</p>
                    </div>
                  </div>
                </CardContent>
              </Card>

              <Card>
                <CardContent className="p-6">
                  <div className="flex items-center">
                    <TrendingUp className="h-4 w-4 text-orange-600" />
                    <div className="ml-4 space-y-1">
                      <p className="text-sm font-medium text-gray-600">Avg Analysis Time</p>
                      <p className="text-2xl font-bold text-gray-900">{metrics.analysisTimeMs.toFixed(1)}ms</p>
                      <p className="text-xs text-gray-500">Cache hit rate: {cacheHitRate.toFixed(1)}%</p>
                    </div>
                  </div>
                </CardContent>
              </Card>
            </div>
          )}

          {/* Recent Analyses */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Brain className="w-5 h-5" />
                Recent Prompt Analyses
              </CardTitle>
            </CardHeader>
            <CardContent>
              {recentAnalyses.length === 0 ? (
                <div className="text-center py-8 text-gray-500">
                  <Brain className="w-12 h-12 mx-auto mb-3 opacity-20" />
                  <p className="font-medium">No recent analyses</p>
                  <p className="text-sm mt-1">
                    Use the Testing tab to analyze prompts, or wait for backend API integration.
                  </p>
                </div>
              ) : (
                <div className="space-y-4">
                  {recentAnalyses.map((analysis, index) => (
                    <div key={index} className="border rounded-lg p-4 space-y-3">
                      <div className="flex items-start justify-between">
                        <div className="flex-1">
                          <p className="text-sm font-medium text-gray-900 line-clamp-2">
                            {analysis.prompt}
                          </p>
                          <div className="flex items-center gap-4 mt-2 text-xs text-gray-500">
                            <span>Score: {analysis.complexityScore.toFixed(2)}</span>
                            <span>Time: {analysis.analysisTimeMs}ms</span>
                            <Badge variant={
                              analysis.recommendedStrategy === 'base_model' ? 'secondary' :
                              analysis.recommendedStrategy === 'adapters' ? 'default' : 'outline'
                            }>
                              {analysis.recommendedStrategy.replace('_', ' ')}
                            </Badge>
                          </div>
                        </div>
                        <div className="text-xs text-gray-400">
                          {new Date(analysis.timestamp).toLocaleTimeString()}
                        </div>
                      </div>

                      <div className="flex flex-wrap gap-2">
                        <Badge variant="outline">{analysis.features.language}</Badge>
                        {analysis.features.frameworks.map(fw => (
                          <Badge key={fw} variant="outline">{fw}</Badge>
                        ))}
                        <Badge variant="outline">{analysis.features.symbols} symbols</Badge>
                        <Badge variant="outline">{analysis.features.verb}</Badge>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="configuration" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Settings className="w-5 h-5" />
                Orchestration Configuration
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-6">
              <div className="flex items-center justify-between">
                <div className="space-y-0.5">
                  <Label className="text-base">Enable Prompt Orchestration</Label>
                  <p className="text-sm text-gray-600">
                    When enabled, prompts are analyzed to determine optimal base model vs adapter usage
                  </p>
                </div>
                <Switch
                  checked={config.enabled}
                  onCheckedChange={(enabled) => setConfig(prev => ({ ...prev, enabled }))}
                />
              </div>

              <Separator />

              <div className="space-y-4">
                <div>
                  <Label className="text-base">Base Model Threshold</Label>
                  <p className="text-sm text-gray-600 mb-2">
                    Minimum complexity score to consider using adapters (0.0-1.0)
                  </p>
                  <div className="space-y-2">
                    <Slider
                      value={[config.baseModelThreshold]}
                      onValueChange={([value]) => setConfig(prev => ({ ...prev, baseModelThreshold: value }))}
                      max={1}
                      min={0}
                      step={0.05}
                      className="w-full"
                    />
                    <div className="flex justify-between text-xs text-gray-500">
                      <span>0.0 (Simple prompts)</span>
                      <span className="font-medium">{config.baseModelThreshold}</span>
                      <span>1.0 (Complex prompts)</span>
                    </div>
                  </div>
                </div>

                <div>
                  <Label className="text-base">Adapter Qualification Threshold</Label>
                  <p className="text-sm text-gray-600 mb-2">
                    Minimum score for individual adapters to be considered (0.0-1.0)
                  </p>
                  <div className="space-y-2">
                    <Slider
                      value={[config.adapterThreshold]}
                      onValueChange={([value]) => setConfig(prev => ({ ...prev, adapterThreshold: value }))}
                      max={1}
                      min={0}
                      step={0.05}
                      className="w-full"
                    />
                    <div className="flex justify-between text-xs text-gray-500">
                      <span>0.0 (All adapters)</span>
                      <span className="font-medium">{config.adapterThreshold}</span>
                      <span>1.0 (Only high-confidence)</span>
                    </div>
                  </div>
                </div>

                <div>
                  <Label htmlFor="analysis-timeout">Analysis Timeout (ms)</Label>
                  <Input
                    id="analysis-timeout"
                    type="number"
                    value={config.analysisTimeout}
                    onChange={(e) => setConfig(prev => ({ ...prev, analysisTimeout: parseInt(e.target.value) }))}
                    className="mt-1"
                  />
                </div>

                <div className="flex items-center justify-between">
                  <div className="space-y-0.5">
                    <Label>Enable Caching</Label>
                    <p className="text-sm text-gray-600">Cache prompt analysis results to improve performance</p>
                  </div>
                  <Switch
                    checked={config.cacheEnabled}
                    onCheckedChange={(cacheEnabled) => setConfig(prev => ({ ...prev, cacheEnabled }))}
                  />
                </div>

                {config.cacheEnabled && (
                  <div>
                    <Label htmlFor="cache-ttl">Cache TTL (seconds)</Label>
                    <Input
                      id="cache-ttl"
                      type="number"
                      value={config.cacheTtl}
                      onChange={(e) => setConfig(prev => ({ ...prev, cacheTtl: parseInt(e.target.value) }))}
                      className="mt-1"
                    />
                  </div>
                )}

                <div>
                  <Label>Fallback Strategy</Label>
                  <div className="mt-2 space-y-2">
                    {[
                      { value: 'base_only', label: 'Base Model Only', desc: 'Always fall back to base model' },
                      { value: 'best_effort', label: 'Best Effort', desc: 'Use best available adapter' },
                      { value: 'adaptive', label: 'Adaptive', desc: 'Learn from past decisions' }
                    ].map(strategy => (
                      <div key={strategy.value} className="flex items-center space-x-2">
                        <input
                          type="radio"
                          id={strategy.value}
                          name="fallbackStrategy"
                          checked={config.fallbackStrategy === strategy.value}
                          onChange={(e) => setConfig(prev => ({
                            ...prev,
                            fallbackStrategy: e.target.value as PromptOrchestrationConfig['fallbackStrategy']
                          }))}
                          className="text-blue-600"
                        />
                        <div>
                          <Label htmlFor={strategy.value} className="font-medium">
                            {strategy.label}
                          </Label>
                          <p className="text-sm text-gray-600">{strategy.desc}</p>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              </div>

              <Separator />

              <div className="flex justify-end">
                <Button onClick={saveConfig} disabled={isLoading}>
                  {isLoading ? (
                    <>
                      <RefreshCw className="w-4 h-4 mr-2 animate-spin" />
                      Saving...
                    </>
                  ) : (
                    <>
                      <CheckCircle className="w-4 h-4 mr-2" />
                      {orchestrationUnavailable ? 'Save Locally (backend pending)' : 'Save Configuration'}
                    </>
                  )}
                </Button>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="testing" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Target className="w-5 h-5" />
                Test Prompt Analysis
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <div>
                <Label htmlFor="test-prompt">Test Prompt</Label>
                <textarea
                  id="test-prompt"
                  value={testPrompt}
                  onChange={(e) => setTestPrompt(e.target.value)}
                  placeholder="Enter a prompt to analyze..."
                  className="w-full mt-1 p-3 border border-gray-300 rounded-md resize-none"
                  rows={4}
                />
              </div>

              <Button onClick={testPromptAnalysis} disabled={isLoading || !testPrompt.trim()}>
                <PlayCircle className="w-4 h-4 mr-2" />
                {isLoading
                  ? 'Analyzing...'
                  : orchestrationUnavailable
                    ? 'Analyze Prompt (local only)'
                    : 'Analyze Prompt'}
              </Button>

              {testResult && (
                <div className="border rounded-lg p-4 space-y-3">
                  <div className="flex items-center gap-2">
                    <Brain className="w-4 h-4 text-blue-600" />
                    <h3 className="font-medium">Analysis Result</h3>
                  </div>

                  <div className="grid grid-cols-2 gap-4 text-sm">
                    <div>
                      <span className="font-medium">Complexity Score:</span>
                      <span className="ml-2">{testResult.complexityScore.toFixed(3)}</span>
                    </div>
                    <div>
                      <span className="font-medium">Analysis Time:</span>
                      <span className="ml-2">{testResult.analysisTimeMs}ms</span>
                    </div>
                    <div className="col-span-2">
                      <span className="font-medium">Recommended Strategy:</span>
                      <Badge variant={
                        testResult.recommendedStrategy === 'base_model' ? 'secondary' :
                        testResult.recommendedStrategy === 'adapters' ? 'default' : 'outline'
                      } className="ml-2">
                        {testResult.recommendedStrategy.replace('_', ' ')}
                      </Badge>
                    </div>
                  </div>

                  <div>
                    <span className="font-medium text-sm">Detected Features:</span>
                    <div className="flex flex-wrap gap-2 mt-2">
                      <Badge variant="outline">{testResult.features.language}</Badge>
                      {testResult.features.frameworks.map(fw => (
                        <Badge key={fw} variant="outline">{fw}</Badge>
                      ))}
                      <Badge variant="outline">{testResult.features.symbols} symbols</Badge>
                      <Badge variant="outline">{testResult.features.tokens} tokens</Badge>
                      <Badge variant="outline">{testResult.features.verb}</Badge>
                    </div>
                  </div>
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="analytics" className="space-y-4">
          <Alert>
            <BarChart3 className="h-4 w-4" />
            <AlertDescription>
              Analytics and performance monitoring coming soon. Backend API endpoints are under development.
            </AlertDescription>
          </Alert>

          {metrics ? (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <Card>
                <CardHeader>
                  <CardTitle>Strategy Distribution</CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="space-y-3">
                    <div className="flex justify-between items-center">
                      <span className="text-sm">Base Model Only</span>
                      <div className="flex items-center gap-2">
                        <div className="w-24 bg-gray-200 rounded-full h-2">
                          <div
                            className="bg-green-600 h-2 rounded-full"
                            style={{ width: `${baseModelPercentage}%` }}
                          />
                        </div>
                        <span className="text-sm font-medium">{baseModelPercentage.toFixed(1)}%</span>
                      </div>
                    </div>

                    <div className="flex justify-between items-center">
                      <span className="text-sm">Adapters Used</span>
                      <div className="flex items-center gap-2">
                        <div className="w-24 bg-gray-200 rounded-full h-2">
                          <div
                            className="bg-purple-600 h-2 rounded-full"
                            style={{ width: `${adapterPercentage}%` }}
                          />
                        </div>
                        <span className="text-sm font-medium">{adapterPercentage.toFixed(1)}%</span>
                      </div>
                    </div>
                  </div>
                </CardContent>
              </Card>

              <Card>
                <CardHeader>
                  <CardTitle>Cache Performance</CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="space-y-3">
                    <div className="flex justify-between items-center">
                      <span className="text-sm">Cache Hit Rate</span>
                      <span className="text-sm font-medium">{cacheHitRate.toFixed(1)}%</span>
                    </div>
                    <div className="flex justify-between items-center">
                      <span className="text-sm">Cache Hits</span>
                      <span className="text-sm font-medium">{metrics.cacheHits.toLocaleString()}</span>
                    </div>
                    <div className="flex justify-between items-center">
                      <span className="text-sm">Cache Misses</span>
                      <span className="text-sm font-medium">{metrics.cacheMisses.toLocaleString()}</span>
                    </div>
                  </div>
                </CardContent>
              </Card>
            </div>
          ) : (
            <div className="text-center py-12 text-gray-500">
              <BarChart3 className="w-12 h-12 mx-auto mb-3 opacity-20" />
              <p className="font-medium">No analytics data available</p>
              <p className="text-sm mt-1">
                Analytics will be displayed once the backend API is implemented.
              </p>
            </div>
          )}
        </TabsContent>
      </Tabs>
    </div>
  );
}
