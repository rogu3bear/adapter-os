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
import { logger, toError } from '../utils/logger';
import apiClient from '../api/client';
import {
  Brain,
  Zap,
  Target,
  Settings,
  BarChart3,
  PlayCircle,
  PauseCircle,
  RefreshCw,
  AlertTriangle,
  CheckCircle,
  TrendingUp,
  MessageSquare,
  Cpu,
  Activity
} from 'lucide-react';

// Configuration interface
interface PromptOrchestrationConfig {
  enabled: boolean;
  baseModelThreshold: number; // Minimum complexity score to use adapters
  adapterThreshold: number; // Minimum score to qualify adapters
  analysisTimeout: number; // Max time for prompt analysis
  cacheEnabled: boolean;
  cacheTtl: number;
  enableTelemetry: boolean;
  fallbackStrategy: 'base_only' | 'best_effort' | 'adaptive';
}

// Orchestration metrics
interface OrchestrationMetrics {
  totalRequests: number;
  baseModelOnly: number;
  adapterUsed: number;
  analysisTimeMs: number;
  cacheHits: number;
  cacheMisses: number;
  lastUpdated: string;
}

// Sample prompt analysis result
interface PromptAnalysis {
  prompt: string;
  complexityScore: number;
  recommendedStrategy: 'base_model' | 'adapters' | 'mixed';
  analysisTimeMs: number;
  features: {
    language: string;
    frameworks: string[];
    symbols: number;
    tokens: number;
    verb: string;
  };
  timestamp: string;
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

  const [metrics, setMetrics] = useState<OrchestrationMetrics>({
    totalRequests: 1247,
    baseModelOnly: 423,
    adapterUsed: 824,
    analysisTimeMs: 23.5,
    cacheHits: 892,
    cacheMisses: 355,
    lastUpdated: new Date().toISOString()
  });

  const [sampleAnalyses, setSampleAnalyses] = useState<PromptAnalysis[]>([
    {
      prompt: "Write a simple hello world function in Python",
      complexityScore: 0.15,
      recommendedStrategy: 'base_model',
      analysisTimeMs: 12,
      features: {
        language: 'python',
        frameworks: [],
        symbols: 2,
        tokens: 8,
        verb: 'write'
      },
      timestamp: new Date(Date.now() - 300000).toISOString()
    },
    {
      prompt: "Implement a Django REST API with authentication and database models",
      complexityScore: 0.78,
      recommendedStrategy: 'adapters',
      analysisTimeMs: 34,
      features: {
        language: 'python',
        frameworks: ['django', 'rest_framework'],
        symbols: 15,
        tokens: 12,
        verb: 'implement'
      },
      timestamp: new Date(Date.now() - 180000).toISOString()
    },
    {
      prompt: "Debug this Rust compilation error in my async tokio code",
      complexityScore: 0.65,
      recommendedStrategy: 'adapters',
      analysisTimeMs: 28,
      features: {
        language: 'rust',
        frameworks: ['tokio'],
        symbols: 8,
        tokens: 10,
        verb: 'debug'
      },
      timestamp: new Date(Date.now() - 60000).toISOString()
    }
  ]);

  const [isLoading, setIsLoading] = useState(false);
  const [testPrompt, setTestPrompt] = useState('');
  const [testResult, setTestResult] = useState<PromptAnalysis | null>(null);

  // Load configuration
  const loadConfig = useCallback(async () => {
    // Placeholder: Prompt orchestration config under development
    // TODO: Implement backend endpoint /v1/orchestration/config
    logger.info('Prompt orchestration config load requested (placeholder)', {
      component: 'PromptOrchestrationPanel',
    });
    // Keep current state or load defaults
    // setConfig(defaultConfig); // if needed
  }, []);

  // Save configuration
  const saveConfig = async () => {
    setIsLoading(true);
    try {
      // Placeholder: Config save under development
      logger.warn('Prompt orchestration config save requested but not implemented', {
        component: 'PromptOrchestrationPanel',
        config: config,
      });
      // TODO: Implement apiClient.saveOrchestrationConfig(config)
    } catch (error) {
      logger.error('Error in saveConfig placeholder', {
        component: 'PromptOrchestrationPanel',
        error: toError(error),
      });
    }
    setIsLoading(false);
  };

  // Test prompt analysis
  const testPromptAnalysis = async () => {
    if (!testPrompt.trim()) return;

    setIsLoading(true);
    try {
      // Placeholder: Analysis under development
      logger.warn('Prompt analysis requested but not implemented', {
        component: 'PromptOrchestrationPanel',
        prompt: testPrompt,
      });
      // Simulate result or set error
      setTestResult({
        prompt: testPrompt,
        complexityScore: 0.5, // placeholder
        recommendedStrategy: 'mixed' as const,
        analysisTimeMs: 0,
        features: {
          language: 'unknown',
          frameworks: [],
          symbols: 0,
          tokens: 0,
          verb: 'unknown',
        },
        timestamp: new Date().toISOString(),
      });
      // TODO: Implement apiClient.analyzePrompt({ prompt: testPrompt })
    } catch (error) {
      logger.error('Error in testPromptAnalysis placeholder', {
        component: 'PromptOrchestrationPanel',
        error: toError(error),
      });
    }
    setIsLoading(false);
  };

  // Load metrics periodically
  useEffect(() => {
    loadConfig();
    const interval = setInterval(() => {
      // In a real implementation, this would fetch updated metrics
      setMetrics(prev => ({
        ...prev,
        totalRequests: prev.totalRequests + Math.floor(Math.random() * 5),
        lastUpdated: new Date().toISOString()
      }));
    }, 5000);

    return () => clearInterval(interval);
  }, [loadConfig]);

  const baseModelPercentage = metrics.totalRequests > 0
    ? (metrics.baseModelOnly / metrics.totalRequests) * 100
    : 0;
  const adapterPercentage = metrics.totalRequests > 0
    ? (metrics.adapterUsed / metrics.totalRequests) * 100
    : 0;
  const cacheHitRate = (metrics.cacheHits + metrics.cacheMisses) > 0
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

      <Tabs defaultValue="overview" className="space-y-4">
        <TabsList>
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="configuration">Configuration</TabsTrigger>
          <TabsTrigger value="testing">Testing</TabsTrigger>
          <TabsTrigger value="analytics">Analytics</TabsTrigger>
        </TabsList>

        <TabsContent value="overview" className="space-y-4">
          {/* Status Overview */}
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

          {/* Recent Analyses */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Brain className="w-5 h-5" />
                Recent Prompt Analyses
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-4">
                {sampleAnalyses.map((analysis, index) => (
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
                            fallbackStrategy: e.target.value as any
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
                <Button onClick={saveConfig} disabled={true} title="Configuration save under development">
                  {isLoading ? (
                    <>
                      <RefreshCw className="w-4 h-4 mr-2 animate-spin" />
                      Saving...
                    </>
                  ) : (
                    <>
                      <CheckCircle className="w-4 h-4 mr-2" />
                      Save Configuration
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

              <Button onClick={testPromptAnalysis} disabled={true || !testPrompt.trim()} title="Prompt analysis under development">
                <PlayCircle className="w-4 h-4 mr-2" />
                {isLoading ? 'Analyzing...' : 'Analyze Prompt'}
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
              Advanced analytics and performance monitoring coming soon. Current metrics show real-time orchestration performance.
            </AlertDescription>
          </Alert>

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
        </TabsContent>
      </Tabs>
    </div>
  );
}
