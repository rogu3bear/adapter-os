import React, { useState, useEffect, useMemo, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '../ui/card';
import { Badge } from '../ui/badge';
import { Button } from '../ui/button';
import { Textarea } from '../ui/textarea';
import { Label } from '../ui/label';
import { Switch } from '../ui/switch';
import { Alert, AlertDescription } from '../ui/alert';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../ui/select';
import {
  LineChart,
  Line,
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  Legend,
  ResponsiveContainer,
  Cell,
} from 'recharts';
import {
  Play,
  Loader2,
  CheckCircle,
  XCircle,
  BarChart3,
  Timer,
  Zap,
  Shield,
  Plus,
  Trash2,
} from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '../../api/client';
import { Adapter, InferRequest, InferResponse, BatchInferRequest, BatchInferResponse } from '../../api/types';
import { logger } from '../../utils/logger';

interface LatencyBucket {
  range: string;
  count: number;
  min: number;
  max: number;
}

interface TokenDistribution {
  range: string;
  count: number;
}

interface BatchResult {
  prompt: string;
  response: InferResponse | null;
  error?: string;
  determinismMatch?: boolean;
}

export default function MLEngineerInferenceTest() {
  // State for batch prompts
  const [prompts, setPrompts] = useState<string[]>(['']);
  const [model, setModel] = useState<string>('default');
  const [selectedAdapter, setSelectedAdapter] = useState<string>('none');
  const [adapters, setAdapters] = useState<Adapter[]>([]);

  // Inference parameters
  const [maxTokens, setMaxTokens] = useState<number>(100);
  const [temperature, setTemperature] = useState<number>(0.7);
  const [seed, setSeed] = useState<number | undefined>(42);

  // Determinism verification
  const [verifyDeterminism, setVerifyDeterminism] = useState<boolean>(false);
  const [determinismRuns, setDeterminismRuns] = useState<number>(3);

  // Results state
  const [isRunning, setIsRunning] = useState<boolean>(false);
  const [results, setResults] = useState<BatchResult[]>([]);
  const [batchResponse, setBatchResponse] = useState<BatchInferResponse | null>(null);

  // Computed metrics
  const [latencyHistogram, setLatencyHistogram] = useState<LatencyBucket[]>([]);
  const [tokenDistribution, setTokenDistribution] = useState<TokenDistribution[]>([]);

  // Load adapters on mount
  useEffect(() => {
    const loadAdapters = async () => {
      try {
        const adapterList = await apiClient.listAdapters();
        setAdapters(adapterList);
      } catch (error) {
        logger.error('Failed to load adapters', { error });
      }
    };
    loadAdapters();
  }, []);

  // Add a new prompt input
  const addPrompt = useCallback(() => {
    setPrompts(prev => [...prev, '']);
  }, []);

  // Remove a prompt
  const removePrompt = useCallback((index: number) => {
    setPrompts(prev => prev.filter((_, i) => i !== index));
  }, []);

  // Update a prompt
  const updatePrompt = useCallback((index: number, value: string) => {
    setPrompts(prev => prev.map((p, i) => i === index ? value : p));
  }, []);

  // Calculate latency histogram from results
  const calculateLatencyHistogram = useCallback((responses: InferResponse[]): LatencyBucket[] => {
    if (responses.length === 0) return [];

    const latencies = responses.map(r => r.latency_ms);
    const minLatency = Math.min(...latencies);
    const maxLatency = Math.max(...latencies);
    const bucketCount = 5;
    const bucketSize = (maxLatency - minLatency) / bucketCount || 1;

    const buckets: LatencyBucket[] = [];
    for (let i = 0; i < bucketCount; i++) {
      const min = minLatency + i * bucketSize;
      const max = min + bucketSize;
      const count = latencies.filter(l => l >= min && (i === bucketCount - 1 ? l <= max : l < max)).length;
      buckets.push({
        range: `${Math.round(min)}-${Math.round(max)}ms`,
        count,
        min: Math.round(min),
        max: Math.round(max),
      });
    }
    return buckets;
  }, []);

  // Calculate token distribution from results
  const calculateTokenDistribution = useCallback((responses: InferResponse[]): TokenDistribution[] => {
    if (responses.length === 0) return [];

    const tokens = responses.map(r => r.tokens_generated);
    const minTokens = Math.min(...tokens);
    const maxTokens = Math.max(...tokens);
    const bucketCount = 5;
    const bucketSize = Math.ceil((maxTokens - minTokens + 1) / bucketCount) || 1;

    const distribution: TokenDistribution[] = [];
    for (let i = 0; i < bucketCount; i++) {
      const min = minTokens + i * bucketSize;
      const max = Math.min(min + bucketSize - 1, maxTokens);
      const count = tokens.filter(t => t >= min && t <= max).length;
      distribution.push({
        range: min === max ? `${min}` : `${min}-${max}`,
        count,
      });
    }
    return distribution;
  }, []);

  // Run batch inference
  const runBatchInference = useCallback(async () => {
    const validPrompts = prompts.filter(p => p.trim().length > 0);
    if (validPrompts.length === 0) {
      toast.error('Please enter at least one prompt');
      return;
    }

    setIsRunning(true);
    setResults([]);
    setBatchResponse(null);

    try {
      const requests: InferRequest[] = validPrompts.map(prompt => ({
        prompt,
        model: model === 'default' ? undefined : model,
        max_tokens: maxTokens,
        temperature,
        seed: verifyDeterminism ? seed : undefined,
        adapter_stack: selectedAdapter !== 'none' ? [selectedAdapter] : undefined,
      }));

      const batchRequest: BatchInferRequest = {
        requests,
      };

      // If verifying determinism, run multiple times and compare
      if (verifyDeterminism) {
        const allRuns: InferResponse[][] = [];

        for (let run = 0; run < determinismRuns; run++) {
          const response = await apiClient.batchInfer(batchRequest);
          allRuns.push(response.results || response.responses || []);
        }

        // Compare all runs for determinism
        const batchResults: BatchResult[] = validPrompts.map((prompt, idx) => {
          const responses = allRuns.map(run => run[idx]);
          const firstResponse = responses[0];

          // Check if all responses match
          const allMatch = responses.every(r =>
            r?.text === firstResponse?.text &&
            r?.tokens_generated === firstResponse?.tokens_generated
          );

          return {
            prompt,
            response: firstResponse,
            determinismMatch: allMatch,
          };
        });

        setResults(batchResults);
        setBatchResponse({
          results: allRuns[0],
          responses: allRuns[0],
          total_tokens: allRuns[0].reduce((sum, r) => sum + (r?.tokens_generated || 0), 0),
          total_latency_ms: allRuns[0].reduce((sum, r) => sum + (r?.latency_ms || 0), 0),
        });

        const passedCount = batchResults.filter(r => r.determinismMatch).length;
        if (passedCount === batchResults.length) {
          toast.success(`Determinism verified: ${passedCount}/${batchResults.length} prompts consistent`);
        } else {
          toast.warning(`Determinism check: ${passedCount}/${batchResults.length} prompts consistent`);
        }
      } else {
        // Single run without determinism verification
        const response = await apiClient.batchInfer(batchRequest);
        const responseResults = response.results || response.responses || [];

        const batchResults: BatchResult[] = validPrompts.map((prompt, idx) => ({
          prompt,
          response: responseResults[idx] || null,
        }));

        setResults(batchResults);
        setBatchResponse(response);
        toast.success(`Batch inference completed: ${responseResults.length} responses`);
      }

      // Calculate metrics
      const responses = results.map(r => r.response).filter((r): r is InferResponse => r !== null);
      if (responses.length > 0) {
        setLatencyHistogram(calculateLatencyHistogram(responses));
        setTokenDistribution(calculateTokenDistribution(responses));
      }
    } catch (error) {
      logger.error('Batch inference failed', { error });
      toast.error('Batch inference failed');
    } finally {
      setIsRunning(false);
    }
  }, [prompts, model, maxTokens, temperature, seed, selectedAdapter, verifyDeterminism, determinismRuns, calculateLatencyHistogram, calculateTokenDistribution]);

  // Update metrics when results change
  useEffect(() => {
    const responses = results.map(r => r.response).filter((r): r is InferResponse => r !== null);
    if (responses.length > 0) {
      setLatencyHistogram(calculateLatencyHistogram(responses));
      setTokenDistribution(calculateTokenDistribution(responses));
    }
  }, [results, calculateLatencyHistogram, calculateTokenDistribution]);

  // Summary statistics
  const stats = useMemo(() => {
    const responses = results.map(r => r.response).filter((r): r is InferResponse => r !== null);
    if (responses.length === 0) return null;

    const latencies = responses.map(r => r.latency_ms);
    const tokens = responses.map(r => r.tokens_generated);

    return {
      totalResponses: responses.length,
      avgLatency: Math.round(latencies.reduce((a, b) => a + b, 0) / latencies.length),
      minLatency: Math.min(...latencies),
      maxLatency: Math.max(...latencies),
      totalTokens: tokens.reduce((a, b) => a + b, 0),
      avgTokens: Math.round(tokens.reduce((a, b) => a + b, 0) / tokens.length),
      determinismPassed: verifyDeterminism ? results.filter(r => r.determinismMatch).length : null,
    };
  }, [results, verifyDeterminism]);

  // Color palette for charts
  const COLORS = ['#4477AA', '#EE6677', '#228833', '#CCBB44', '#66CCEE'];

  return (
    <div className="space-y-6 p-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold">ML Engineer Inference Test</h2>
          <p className="text-sm text-muted-foreground">
            Batch inference testing with latency analysis and determinism verification
          </p>
        </div>
        <Badge variant="outline" className="gap-1">
          <Zap className="h-3 w-3" />
          {results.length} Results
        </Badge>
      </div>

      {/* Configuration */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
        {/* Model/Adapter Selection */}
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm">Model Configuration</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <Label>Model</Label>
              <Select value={model} onValueChange={setModel}>
                <SelectTrigger>
                  <SelectValue placeholder="Select model" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="default">Default</SelectItem>
                  <SelectItem value="llama-7b">Llama 7B</SelectItem>
                  <SelectItem value="llama-13b">Llama 13B</SelectItem>
                </SelectContent>
              </Select>
            </div>

            <div className="space-y-2">
              <Label>Adapter</Label>
              <Select value={selectedAdapter} onValueChange={setSelectedAdapter}>
                <SelectTrigger>
                  <SelectValue placeholder="Select adapter" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="none">None</SelectItem>
                  {adapters.map(adapter => (
                    <SelectItem key={adapter.id} value={adapter.id}>
                      {adapter.name || adapter.adapter_id}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </CardContent>
        </Card>

        {/* Inference Parameters */}
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm">Inference Parameters</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="grid grid-cols-2 gap-2">
              <div className="space-y-1">
                <Label className="text-xs">Max Tokens</Label>
                <input
                  type="number"
                  value={maxTokens}
                  onChange={e => setMaxTokens(parseInt(e.target.value) || 100)}
                  className="w-full px-2 py-1 text-sm border rounded"
                  min={1}
                  max={2048}
                />
              </div>
              <div className="space-y-1">
                <Label className="text-xs">Temperature</Label>
                <input
                  type="number"
                  value={temperature}
                  onChange={e => setTemperature(parseFloat(e.target.value) || 0.7)}
                  className="w-full px-2 py-1 text-sm border rounded"
                  min={0}
                  max={2}
                  step={0.1}
                />
              </div>
            </div>
            <div className="space-y-1">
              <Label className="text-xs">Seed (for determinism)</Label>
              <input
                type="number"
                value={seed || ''}
                onChange={e => setSeed(e.target.value ? parseInt(e.target.value) : undefined)}
                className="w-full px-2 py-1 text-sm border rounded"
                placeholder="Optional"
              />
            </div>
          </CardContent>
        </Card>

        {/* Determinism Verification */}
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm flex items-center gap-2">
              <Shield className="h-4 w-4" />
              Determinism Verification
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex items-center justify-between">
              <Label className="text-sm">Enable Verification</Label>
              <Switch
                checked={verifyDeterminism}
                onCheckedChange={setVerifyDeterminism}
              />
            </div>
            {verifyDeterminism && (
              <div className="space-y-1">
                <Label className="text-xs">Number of Runs</Label>
                <input
                  type="number"
                  value={determinismRuns}
                  onChange={e => setDeterminismRuns(Math.max(2, parseInt(e.target.value) || 3))}
                  className="w-full px-2 py-1 text-sm border rounded"
                  min={2}
                  max={10}
                />
              </div>
            )}
            {verifyDeterminism && (
              <Alert>
                <AlertDescription className="text-xs">
                  Each prompt will be run {determinismRuns} times to verify consistent outputs
                </AlertDescription>
              </Alert>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Batch Prompts Input */}
      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <CardTitle className="text-sm">Batch Prompts</CardTitle>
            <Button variant="outline" size="sm" onClick={addPrompt}>
              <Plus className="h-4 w-4 mr-1" />
              Add Prompt
            </Button>
          </div>
        </CardHeader>
        <CardContent className="space-y-3">
          {prompts.map((prompt, index) => (
            <div key={index} className="flex gap-2">
              <Textarea
                value={prompt}
                onChange={e => updatePrompt(index, e.target.value)}
                placeholder={`Prompt ${index + 1}...`}
                className="min-h-[60px] text-sm flex-1"
              />
              {prompts.length > 1 && (
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => removePrompt(index)}
                  className="shrink-0"
                >
                  <Trash2 className="h-4 w-4 text-destructive" />
                </Button>
              )}
            </div>
          ))}
          <Button
            onClick={runBatchInference}
            disabled={isRunning || prompts.every(p => !p.trim())}
            className="w-full"
          >
            {isRunning ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                Running...
              </>
            ) : (
              <>
                <Play className="h-4 w-4 mr-2" />
                Run Batch Inference
              </>
            )}
          </Button>
        </CardContent>
      </Card>

      {/* Results Summary */}
      {stats && (
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm">Summary Statistics</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-4">
              <div className="text-center">
                <div className="text-2xl font-bold">{stats.totalResponses}</div>
                <div className="text-xs text-muted-foreground">Responses</div>
              </div>
              <div className="text-center">
                <div className="text-2xl font-bold">{stats.avgLatency}ms</div>
                <div className="text-xs text-muted-foreground">Avg Latency</div>
              </div>
              <div className="text-center">
                <div className="text-2xl font-bold">{stats.minLatency}ms</div>
                <div className="text-xs text-muted-foreground">Min Latency</div>
              </div>
              <div className="text-center">
                <div className="text-2xl font-bold">{stats.maxLatency}ms</div>
                <div className="text-xs text-muted-foreground">Max Latency</div>
              </div>
              <div className="text-center">
                <div className="text-2xl font-bold">{stats.totalTokens}</div>
                <div className="text-xs text-muted-foreground">Total Tokens</div>
              </div>
              {stats.determinismPassed !== null && (
                <div className="text-center">
                  <div className="text-2xl font-bold flex items-center justify-center gap-1">
                    {stats.determinismPassed === stats.totalResponses ? (
                      <CheckCircle className="h-5 w-5 text-green-500" />
                    ) : (
                      <XCircle className="h-5 w-5 text-amber-500" />
                    )}
                    {stats.determinismPassed}/{stats.totalResponses}
                  </div>
                  <div className="text-xs text-muted-foreground">Determinism</div>
                </div>
              )}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Charts */}
      {results.length > 0 && (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
          {/* Latency Histogram */}
          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-sm flex items-center gap-2">
                <Timer className="h-4 w-4" />
                Latency Distribution
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="h-64">
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={latencyHistogram}>
                    <CartesianGrid strokeDasharray="3 3" />
                    <XAxis dataKey="range" fontSize={10} />
                    <YAxis fontSize={10} />
                    <Tooltip />
                    <Bar dataKey="count" fill="#4477AA">
                      {latencyHistogram.map((_, index) => (
                        <Cell key={index} fill={COLORS[index % COLORS.length]} />
                      ))}
                    </Bar>
                  </BarChart>
                </ResponsiveContainer>
              </div>
            </CardContent>
          </Card>

          {/* Token Distribution */}
          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-sm flex items-center gap-2">
                <BarChart3 className="h-4 w-4" />
                Token Distribution
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="h-64">
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={tokenDistribution}>
                    <CartesianGrid strokeDasharray="3 3" />
                    <XAxis dataKey="range" fontSize={10} />
                    <YAxis fontSize={10} />
                    <Tooltip />
                    <Bar dataKey="count" fill="#228833">
                      {tokenDistribution.map((_, index) => (
                        <Cell key={index} fill={COLORS[(index + 2) % COLORS.length]} />
                      ))}
                    </Bar>
                  </BarChart>
                </ResponsiveContainer>
              </div>
            </CardContent>
          </Card>
        </div>
      )}

      {/* Individual Results */}
      {results.length > 0 && (
        <Card>
          <CardHeader className="pb-3">
            <CardTitle className="text-sm">Individual Results</CardTitle>
            <CardDescription>Response details for each prompt</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              {results.map((result, index) => (
                <div key={index} className="border rounded-lg p-4 space-y-2">
                  <div className="flex items-center justify-between">
                    <Label className="text-xs font-medium">Prompt {index + 1}</Label>
                    <div className="flex items-center gap-2">
                      {result.response && (
                        <>
                          <Badge variant="outline" className="text-xs">
                            {result.response.latency_ms}ms
                          </Badge>
                          <Badge variant="outline" className="text-xs">
                            {result.response.tokens_generated} tokens
                          </Badge>
                        </>
                      )}
                      {verifyDeterminism && (
                        <Badge
                          variant={result.determinismMatch ? 'default' : 'destructive'}
                          className="text-xs"
                        >
                          {result.determinismMatch ? (
                            <><CheckCircle className="h-3 w-3 mr-1" /> Deterministic</>
                          ) : (
                            <><XCircle className="h-3 w-3 mr-1" /> Non-deterministic</>
                          )}
                        </Badge>
                      )}
                    </div>
                  </div>
                  <div className="text-sm text-muted-foreground bg-muted/50 p-2 rounded">
                    {result.prompt}
                  </div>
                  {result.response ? (
                    <div className="text-sm bg-card border p-2 rounded whitespace-pre-wrap">
                      {result.response.text}
                    </div>
                  ) : result.error ? (
                    <Alert variant="destructive">
                      <AlertDescription>{result.error}</AlertDescription>
                    </Alert>
                  ) : null}
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
