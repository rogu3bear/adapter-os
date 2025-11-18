import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from './ui/card';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Slider } from './ui/slider';
import { Badge } from './ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import {
  Settings,
  Sliders,
  TrendingUp,
  Target,
  Save,
  RotateCcw,
  AlertCircle,
  CheckCircle,
  BarChart3,
  Zap
} from 'lucide-react';
import apiClient from '../api/client';
import { RouterConfig, FeatureVector, AdapterScore } from '../api/types';
import { toast } from 'sonner';
import { logger } from '../utils/logger';

interface RouterConfigPageProps {
  selectedTenant: string;
}

interface FeatureWeights {
  language: number;
  framework: number;
  symbol_hits: number;
  path_tokens: number;
  prompt_verb: number;
}

export function RouterConfigPage({ selectedTenant }: RouterConfigPageProps) {
  const [config, setConfig] = useState<RouterConfig>({
    k_sparse: 8,
    gate_quant: 'q15',
    entropy_floor: 0.1,
    sample_tokens_full: 128
  });

  const [featureWeights, setFeatureWeights] = useState<FeatureWeights>({
    language: 0.30,
    framework: 0.25,
    symbol_hits: 0.20,
    path_tokens: 0.15,
    prompt_verb: 0.10
  });

  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [testPrompt, setTestPrompt] = useState('');
  const [testResults, setTestResults] = useState<AdapterScore[] | null>(null);
  const [hasUnsavedChanges, setHasUnsavedChanges] = useState(false);

  useEffect(() => {
    loadRouterConfig();
  }, [selectedTenant]);

  const loadRouterConfig = async () => {
    setIsLoading(true);
    try {
      // Load current policy to get router config
      const policies = await apiClient.listPolicies();
      if (policies.length > 0) {
        const policyData = JSON.parse(policies[0].policy_json);
        if (policyData.packs?.router) {
          const routerConfig = policyData.packs.router;
          setConfig({
            k_sparse: routerConfig.k_max || 8,
            gate_quant: routerConfig.gate_quantization || 'q15',
            entropy_floor: routerConfig.entropy_floor || 0.1,
            sample_tokens_full: routerConfig.sample_tokens_full || 128
          });

          if (routerConfig.feature_weights) {
            setFeatureWeights(routerConfig.feature_weights);
          }
        }
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to load router config';
      logger.error('Failed to load router config', {
        component: 'RouterConfigPage',
        operation: 'loadRouterConfig',
        tenant: selectedTenant,
        error: errorMessage
      });
      toast.error('Failed to load router configuration');
    } finally {
      setIsLoading(false);
    }
  };

  const handleConfigChange = (field: keyof RouterConfig, value: any) => {
    setConfig(prev => ({ ...prev, [field]: value }));
    setHasUnsavedChanges(true);
  };

  const handleWeightChange = (feature: keyof FeatureWeights, value: number) => {
    setFeatureWeights(prev => ({ ...prev, [feature]: value }));
    setHasUnsavedChanges(true);
  };

  const normalizeWeights = () => {
    const total = Object.values(featureWeights).reduce((sum, w) => sum + w, 0);
    const normalized = Object.entries(featureWeights).reduce((acc, [key, value]) => ({
      ...acc,
      [key]: value / total
    }), {} as FeatureWeights);
    setFeatureWeights(normalized);
    toast.success('Weights normalized to sum to 1.0');
  };

  const resetToDefaults = () => {
    setConfig({
      k_sparse: 8,
      gate_quant: 'q15',
      entropy_floor: 0.1,
      sample_tokens_full: 128
    });
    setFeatureWeights({
      language: 0.30,
      framework: 0.25,
      symbol_hits: 0.20,
      path_tokens: 0.15,
      prompt_verb: 0.10
    });
    setHasUnsavedChanges(false);
    toast.success('Reset to default configuration');
  };

  const saveConfiguration = async () => {
    setIsSaving(true);
    try {
      // Get current policy
      const policies = await apiClient.listPolicies();
      if (policies.length === 0) {
        throw new Error('No policy found to update');
      }

      const currentPolicy = JSON.parse(policies[0].policy_json);

      // Update router configuration
      const updatedPolicy = {
        ...currentPolicy,
        packs: {
          ...currentPolicy.packs,
          router: {
            k_min: Math.floor(config.k_sparse / 2),
            k_max: config.k_sparse,
            entropy_floor: config.entropy_floor,
            gate_quantization: config.gate_quant,
            sample_tokens_full: config.sample_tokens_full,
            feature_weights: featureWeights
          }
        }
      };

      // Apply updated policy
      await apiClient.applyPolicy({
        cpid: policies[0].cpid,
        policy_json: JSON.stringify(updatedPolicy)
      });

      setHasUnsavedChanges(false);
      logger.info('Router configuration saved', {
        component: 'RouterConfigPage',
        operation: 'saveConfiguration',
        tenant: selectedTenant,
        config
      });
      toast.success('Router configuration saved successfully');
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to save router config';
      logger.error('Failed to save router config', {
        component: 'RouterConfigPage',
        operation: 'saveConfiguration',
        tenant: selectedTenant,
        error: errorMessage
      });
      toast.error('Failed to save configuration');
    } finally {
      setIsSaving(false);
    }
  };

  const testRouterConfig = async () => {
    if (!testPrompt.trim()) {
      toast.error('Please enter a test prompt');
      return;
    }

    setIsLoading(true);
    try {
      const result = await apiClient.debugRouting({
        prompt: testPrompt
      });
      setTestResults(result.selected_adapters);
      logger.info('Router test completed', {
        component: 'RouterConfigPage',
        operation: 'testRouterConfig',
        resultCount: result.selected_adapters.length
      });
      toast.success('Router test completed');
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to test router';
      logger.error('Failed to test router', {
        component: 'RouterConfigPage',
        operation: 'testRouterConfig',
        testPrompt,
        error: errorMessage
      });
      toast.error('Router test failed');
    } finally {
      setIsLoading(false);
    }
  };

  const weightTotal = Object.values(featureWeights).reduce((sum, w) => sum + w, 0);
  const isWeightBalanced = Math.abs(weightTotal - 1.0) < 0.001;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-3xl font-bold tracking-tight">Router Configuration</h2>
          <p className="text-muted-foreground">
            Configure K-sparse routing, feature weights, and gate quantization
          </p>
        </div>
        <div className="flex gap-2">
          {hasUnsavedChanges && (
            <Badge variant="outline" className="text-amber-600">
              <AlertCircle className="w-3 h-3 mr-1" />
              Unsaved Changes
            </Badge>
          )}
          <Button variant="outline" onClick={resetToDefaults}>
            <RotateCcw className="w-4 h-4 mr-2" />
            Reset
          </Button>
          <Button onClick={saveConfiguration} disabled={isSaving}>
            <Save className="w-4 h-4 mr-2" />
            {isSaving ? 'Saving...' : 'Save Configuration'}
          </Button>
        </div>
      </div>

      <Tabs defaultValue="basic" className="space-y-4">
        <TabsList>
          <TabsTrigger value="basic">
            <Settings className="w-4 h-4 mr-2" />
            Basic Settings
          </TabsTrigger>
          <TabsTrigger value="weights">
            <Sliders className="w-4 h-4 mr-2" />
            Feature Weights
          </TabsTrigger>
          <TabsTrigger value="calibration">
            <TrendingUp className="w-4 h-4 mr-2" />
            Calibration
          </TabsTrigger>
          <TabsTrigger value="test">
            <Zap className="w-4 h-4 mr-2" />
            Test Router
          </TabsTrigger>
        </TabsList>

        {/* Basic Settings */}
        <TabsContent value="basic" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>K-Sparse Configuration</CardTitle>
              <CardDescription>
                Configure the number of adapters selected per token (K-sparse routing)
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-6">
              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <Label htmlFor="k-sparse">K Value (Adapters per Token)</Label>
                  <span className="text-2xl font-bold text-primary">{config.k_sparse}</span>
                </div>
                <Slider
                  id="k-sparse"
                  min={1}
                  max={32}
                  step={1}
                  value={[config.k_sparse]}
                  onValueChange={([value]) => handleConfigChange('k_sparse', value)}
                  className="w-full"
                />
                <p className="text-sm text-muted-foreground">
                  Recommended: 8-16 for balanced performance. Higher K increases compute cost.
                </p>
              </div>

              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <Label htmlFor="entropy-floor">Entropy Floor</Label>
                  <span className="text-lg font-mono">{config.entropy_floor.toFixed(3)}</span>
                </div>
                <Slider
                  id="entropy-floor"
                  min={0}
                  max={1}
                  step={0.01}
                  value={[config.entropy_floor]}
                  onValueChange={([value]) => handleConfigChange('entropy_floor', value)}
                  className="w-full"
                />
                <p className="text-sm text-muted-foreground">
                  Minimum entropy threshold for routing decisions (0.0 - 1.0)
                </p>
              </div>

              <div className="space-y-3">
                <Label htmlFor="sample-tokens">Full Sampling Tokens</Label>
                <Input
                  id="sample-tokens"
                  type="number"
                  min={32}
                  max={512}
                  value={config.sample_tokens_full}
                  onChange={(e) => handleConfigChange('sample_tokens_full', parseInt(e.target.value))}
                />
                <p className="text-sm text-muted-foreground">
                  Number of initial tokens to log with full router decisions (default: 128)
                </p>
              </div>

              <div className="space-y-3">
                <Label htmlFor="gate-quant">Gate Quantization</Label>
                <select
                  id="gate-quant"
                  value={config.gate_quant}
                  onChange={(e) => handleConfigChange('gate_quant', e.target.value)}
                  className="w-full rounded-md border border-input bg-background px-3 py-2"
                >
                  <option value="q15">Q15 (15-bit, recommended)</option>
                  <option value="q8">Q8 (8-bit, faster)</option>
                  <option value="f16">FP16 (16-bit float, precise)</option>
                </select>
                <p className="text-sm text-muted-foreground">
                  Gate value quantization format for memory efficiency
                </p>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        {/* Feature Weights */}
        <TabsContent value="weights" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center justify-between">
                <span>Feature Vector Weights</span>
                <div className="flex items-center gap-2">
                  <span className="text-sm font-normal text-muted-foreground">
                    Total: {weightTotal.toFixed(3)}
                  </span>
                  {isWeightBalanced ? (
                    <CheckCircle className="w-5 h-5 text-green-500" />
                  ) : (
                    <AlertCircle className="w-5 h-5 text-amber-500" />
                  )}
                </div>
              </CardTitle>
              <CardDescription>
                Configure feature importance for adapter selection (should sum to 1.0)
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-6">
              {(Object.keys(featureWeights) as Array<keyof FeatureWeights>).map((feature) => (
                <div key={feature} className="space-y-3">
                  <div className="flex items-center justify-between">
                    <Label htmlFor={`weight-${feature}`} className="capitalize">
                      {feature.replace('_', ' ')}
                    </Label>
                    <span className="text-lg font-mono">{featureWeights[feature].toFixed(2)}</span>
                  </div>
                  <Slider
                    id={`weight-${feature}`}
                    min={0}
                    max={1}
                    step={0.01}
                    value={[featureWeights[feature]]}
                    onValueChange={([value]) => handleWeightChange(feature, value)}
                    className="w-full"
                  />
                  <div className="h-2 bg-secondary rounded-full overflow-hidden">
                    <div
                      className="h-full bg-primary transition-all"
                      style={{ width: `${featureWeights[feature] * 100}%` }}
                    />
                  </div>
                </div>
              ))}

              <div className="pt-4 border-t">
                <Button onClick={normalizeWeights} variant="outline" className="w-full">
                  <Target className="w-4 h-4 mr-2" />
                  Normalize Weights to 1.0
                </Button>
              </div>

              <div className="p-4 bg-muted rounded-lg space-y-2">
                <h4 className="font-semibold text-sm">Weight Recommendations:</h4>
                <ul className="text-sm text-muted-foreground space-y-1">
                  <li>Language: 0.30 (strong signal for language-specific adapters)</li>
                  <li>Framework: 0.25 (strong signal for framework adapters)</li>
                  <li>Symbol Hits: 0.20 (moderate signal from code index)</li>
                  <li>Path Tokens: 0.15 (moderate signal from file paths)</li>
                  <li>Prompt Verb: 0.10 (weak signal from action verbs)</li>
                </ul>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        {/* Calibration */}
        <TabsContent value="calibration" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Router Calibration</CardTitle>
              <CardDescription>
                Automatic calibration using historical routing data
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="p-4 bg-muted rounded-lg">
                <p className="text-sm text-muted-foreground">
                  Calibration analyzes routing performance and automatically adjusts feature weights
                  to optimize adapter selection quality. This process uses telemetry data from
                  previous routing decisions.
                </p>
              </div>

              <Button className="w-full" variant="outline" disabled>
                <BarChart3 className="w-4 h-4 mr-2" />
                Run Automatic Calibration (Coming Soon)
              </Button>

              <div className="pt-4 space-y-2">
                <h4 className="font-semibold text-sm">Calibration Metrics:</h4>
                <div className="grid grid-cols-2 gap-4">
                  <div className="p-3 bg-muted rounded-lg">
                    <div className="text-xs text-muted-foreground">Hit Rate</div>
                    <div className="text-2xl font-bold">--</div>
                  </div>
                  <div className="p-3 bg-muted rounded-lg">
                    <div className="text-xs text-muted-foreground">Avg Confidence</div>
                    <div className="text-2xl font-bold">--</div>
                  </div>
                  <div className="p-3 bg-muted rounded-lg">
                    <div className="text-xs text-muted-foreground">Latency Overhead</div>
                    <div className="text-2xl font-bold">--</div>
                  </div>
                  <div className="p-3 bg-muted rounded-lg">
                    <div className="text-xs text-muted-foreground">Quality Score</div>
                    <div className="text-2xl font-bold">--</div>
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        {/* Test Router */}
        <TabsContent value="test" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Test Router Configuration</CardTitle>
              <CardDescription>
                Test routing decisions with sample prompts
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="test-prompt">Test Prompt</Label>
                <Input
                  id="test-prompt"
                  placeholder="Enter a prompt to test routing..."
                  value={testPrompt}
                  onChange={(e) => setTestPrompt(e.target.value)}
                />
              </div>

              <Button onClick={testRouterConfig} disabled={isLoading} className="w-full">
                <Zap className="w-4 h-4 mr-2" />
                {isLoading ? 'Testing...' : 'Test Router'}
              </Button>

              {testResults && testResults.length > 0 && (
                <div className="mt-4 space-y-3">
                  <h4 className="font-semibold">Selected Adapters:</h4>
                  <div className="space-y-2">
                    {testResults.map((adapter, idx) => (
                      <div
                        key={idx}
                        className="flex items-center justify-between p-3 bg-muted rounded-lg"
                      >
                        <div className="flex items-center gap-3">
                          <Badge variant="outline">{idx + 1}</Badge>
                          <span className="font-mono text-sm">{adapter.adapter_id}</span>
                        </div>
                        <div className="flex items-center gap-4">
                          <div className="text-right">
                            <div className="text-xs text-muted-foreground">Score</div>
                            <div className="font-mono text-sm">{adapter.score.toFixed(4)}</div>
                          </div>
                          <div className="text-right">
                            <div className="text-xs text-muted-foreground">Gate</div>
                            <div className="font-mono text-sm">{adapter.gate_value}</div>
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
