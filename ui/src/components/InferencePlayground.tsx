import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Textarea } from './ui/textarea';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Slider } from './ui/slider';
import { Checkbox } from './ui/checkbox';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Alert, AlertDescription } from './ui/alert';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './ui/collapsible';
import { 
  Play, 
  Copy, 
  Download, 
  History, 
  Settings2, 
  ChevronDown,
  Zap,
  Clock,
  BarChart3,
  Split,
  FileText,
  AlertTriangle,
  CheckCircle
} from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '../api/client';
import { InferRequest, InferResponse, InferenceSession } from '../api/types';
import { TraceVisualizer } from './TraceVisualizer';
import { logger, toError } from '../utils/logger';

interface InferencePlaygroundProps {
  selectedTenant: string;
}

interface InferenceConfig extends InferRequest {
  id: string;
}

export function InferencePlayground({ selectedTenant }: InferencePlaygroundProps) {
  const [mode, setMode] = useState<'single' | 'comparison'>('single');
  const [prompt, setPrompt] = useState('');
  const [showAdvanced, setShowAdvanced] = useState(false);
  
  // Inference configurations
  const [configA, setConfigA] = useState<InferenceConfig>({
    id: 'a',
    prompt: '',
    max_tokens: 100,
    temperature: 0.7,
    top_k: 50,
    top_p: 0.9,
    seed: undefined,
    require_evidence: false,
  });

  const [configB, setConfigB] = useState<InferenceConfig>({
    id: 'b',
    prompt: '',
    max_tokens: 100,
    temperature: 0.9,
    top_k: 50,
    top_p: 0.9,
    seed: undefined,
    require_evidence: false,
  });

  const [responseA, setResponseA] = useState<InferResponse | null>(null);
  const [responseB, setResponseB] = useState<InferResponse | null>(null);
  const [isLoadingA, setIsLoadingA] = useState(false);
  const [isLoadingB, setIsLoadingB] = useState(false);
  
  const [recentSessions, setRecentSessions] = useState<InferenceSession[]>([]);

  useEffect(() => {
    // Load recent sessions from localStorage
    const stored = localStorage.getItem('inference_sessions');
    if (stored) {
      try {
        setRecentSessions(JSON.parse(stored));
      } catch (err) {
        logger.error('Failed to parse stored inference sessions', {
          component: 'InferencePlayground',
          operation: 'loadSessions',
        }, toError(err));
      }
    }
  }, []);

  const saveSession = (config: InferenceConfig, response: InferResponse) => {
    // Convert InferResponse to EnhancedInferResponse for session storage
    const enhancedResponse = {
      ...response,
      token_count: response.token_count || 0,
      finish_reason: response.finish_reason || 'stop',
      latency_ms: response.latency_ms || 0,
      trace: response.trace,
    };
    
    const session: InferenceSession = {
      id: Date.now().toString(),
      created_at: new Date().toISOString(),
      prompt: config.prompt,
      request: config,
      response: enhancedResponse as any, // Type compatibility
      status: 'completed',
    };

    const updated = [session, ...recentSessions].slice(0, 10); // Keep last 10
    setRecentSessions(updated);
    localStorage.setItem('inference_sessions', JSON.stringify(updated));
  };

  const handleInfer = async (config: InferenceConfig, setResponse: (r: InferResponse | null) => void, setLoading: (l: boolean) => void) => {
    if (!config.prompt.trim()) {
      toast.error('Please enter a prompt');
      return;
    }

    setLoading(true);
    setResponse(null);

    try {
      const response = await apiClient.infer(config);
      setResponse(response);
      saveSession(config, response);
      toast.success('Inference completed');
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Inference failed';
      toast.error(errorMessage);
      logger.error('Inference request failed', {
        component: 'InferencePlayground',
        operation: 'infer',
        configId: config.id,
        tenantId: selectedTenant,
      }, toError(err));
    } finally {
      setLoading(false);
    }
  };

  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text);
    toast.success('Copied to clipboard');
  };

  const handleExport = (config: InferenceConfig, response: InferResponse | null) => {
    if (!response) return;

    const data = {
      prompt: config.prompt,
      config,
      response,
      timestamp: new Date().toISOString(),
    };

    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `inference-${Date.now()}.json`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
    toast.success('Exported successfully');
  };

  const loadSession = (session: InferenceSession) => {
    setPrompt(session.prompt);
    setConfigA({ ...configA, prompt: session.prompt, ...session.request });
    if (session.response) {
      setResponseA(session.response);
    }
    toast.success('Session loaded');
  };

  const renderAdvancedOptions = (config: InferenceConfig, setConfig: (c: InferenceConfig) => void) => (
    <Collapsible open={showAdvanced} onOpenChange={setShowAdvanced}>
      <CollapsibleTrigger asChild>
        <Button variant="ghost" className="w-full justify-between">
          <span className="flex items-center gap-2">
            <Settings2 className="h-4 w-4" />
            Advanced Options
          </span>
          <ChevronDown className={`h-4 w-4 transition-transform ${showAdvanced ? 'rotate-180' : ''}`} />
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent className="space-y-4 pt-4">
        <div className="space-y-2">
          <div className="flex justify-between">
            <Label>Max Tokens</Label>
            <span className="text-sm text-muted-foreground">{config.max_tokens}</span>
          </div>
          <Slider
            value={[config.max_tokens || 100]}
            onValueChange={(v) => setConfig({ ...config, max_tokens: v[0] })}
            min={10}
            max={2000}
            step={10}
          />
        </div>

        <div className="space-y-2">
          <div className="flex justify-between">
            <Label>Temperature</Label>
            <span className="text-sm text-muted-foreground">{config.temperature?.toFixed(2)}</span>
          </div>
          <Slider
            value={[config.temperature || 0.7]}
            onValueChange={(v) => setConfig({ ...config, temperature: v[0] })}
            min={0}
            max={2}
            step={0.1}
          />
        </div>

        <div className="space-y-2">
          <div className="flex justify-between">
            <Label>Top K</Label>
            <span className="text-sm text-muted-foreground">{config.top_k}</span>
          </div>
          <Slider
            value={[config.top_k || 50]}
            onValueChange={(v) => setConfig({ ...config, top_k: v[0] })}
            min={1}
            max={100}
            step={1}
          />
        </div>

        <div className="space-y-2">
          <div className="flex justify-between">
            <Label>Top P</Label>
            <span className="text-sm text-muted-foreground">{config.top_p?.toFixed(2)}</span>
          </div>
          <Slider
            value={[config.top_p || 0.9]}
            onValueChange={(v) => setConfig({ ...config, top_p: v[0] })}
            min={0}
            max={1}
            step={0.05}
          />
        </div>

        <div className="space-y-2">
          <Label htmlFor="seed">Seed (Optional)</Label>
          <Input
            id="seed"
            type="number"
            placeholder="Random seed"
            value={config.seed || ''}
            onChange={(e) => setConfig({ ...config, seed: parseInt(e.target.value) || undefined })}
          />
        </div>

        <div className="flex items-center space-x-2">
          <Checkbox
            id="evidence"
            checked={config.require_evidence || false}
            onCheckedChange={(checked) => setConfig({ ...config, require_evidence: !!checked })}
          />
          <Label htmlFor="evidence">Require Evidence (RAG)</Label>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );

  const renderResponse = (response: InferResponse | null, isLoading: boolean) => {
    if (isLoading) {
      return (
        <div className="flex items-center justify-center p-8">
          <div className="text-center space-y-2">
            <Zap className="h-8 w-8 animate-pulse mx-auto text-primary" />
            <p className="text-sm text-muted-foreground">Generating response...</p>
          </div>
        </div>
      );
    }

    if (!response) {
      return (
        <div className="flex items-center justify-center p-8 text-muted-foreground">
          <FileText className="h-8 w-8 mr-2" />
          <p>No response yet. Click "Generate" to run inference.</p>
        </div>
      );
    }

    return (
      <div className="space-y-4">
        {/* Response Text */}
        <Card>
          <CardHeader className="pb-3">
            <div className="flex items-center justify-between">
              <CardTitle className="text-base">Response</CardTitle>
              <div className="flex gap-2">
                <Badge variant="outline" className="gap-1">
                  <Clock className="h-3 w-3" />
                  {response.latency_ms || ('trace' in response && response.trace && 'latency_ms' in response.trace ? (response.trace as any).latency_ms : 0)}ms
                </Badge>
                <Badge variant="outline" className="gap-1">
                  <FileText className="h-3 w-3" />
                  {response.token_count || 0} tokens
                </Badge>
              </div>
            </div>
          </CardHeader>
          <CardContent>
            <div className="relative">
              <pre className="whitespace-pre-wrap text-sm p-4 bg-muted rounded-lg">
                {response.text}
              </pre>
              <Button
                variant="ghost"
                size="sm"
                className="absolute top-2 right-2"
                onClick={() => handleCopy(response.text)}
              >
                <Copy className="h-4 w-4" />
              </Button>
            </div>
          </CardContent>
        </Card>

        {/* Trace Information */}
        {response.trace && 'latency_ms' in response.trace && (
          <TraceVisualizer trace={response.trace as any} />
        )}

        {/* Finish Reason */}
        <div className="flex items-center gap-2">
          <span className="text-sm text-muted-foreground">Finish Reason:</span>
          <Badge>{response.finish_reason}</Badge>
        </div>
      </div>
    );
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex-between">
        <div>
          <h1 className="text-2xl font-bold">Inference Playground</h1>
          <p className="text-sm text-muted-foreground">
            Test model inference with advanced configuration options
          </p>
        </div>
        <div className="flex gap-2">
          <Button
            variant={mode === 'single' ? 'default' : 'outline'}
            onClick={() => setMode('single')}
          >
            <FileText className="h-4 w-4 mr-2" />
            Single
          </Button>
          <Button
            variant={mode === 'comparison' ? 'default' : 'outline'}
            onClick={() => setMode('comparison')}
          >
            <Split className="h-4 w-4 mr-2" />
            Comparison
          </Button>
        </div>
      </div>

      {mode === 'single' ? (
        /* Single Mode */
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* Configuration Panel */}
          <div className="space-y-4">
            <Card>
              <CardHeader>
                <CardTitle className="text-base">Configuration</CardTitle>
              </CardHeader>
              <CardContent className="space-y-4">
                <div className="space-y-2">
                  <Label htmlFor="prompt">Prompt</Label>
                  <Textarea
                    id="prompt"
                    placeholder="Enter your prompt here..."
                    value={configA.prompt}
                    onChange={(e) => setConfigA({ ...configA, prompt: e.target.value })}
                    rows={6}
                  />
                </div>

                {renderAdvancedOptions(configA, setConfigA)}

                <Button
                  className="w-full"
                  onClick={() => handleInfer(configA, setResponseA, setIsLoadingA)}
                  disabled={isLoadingA}
                >
                  <Play className="h-4 w-4 mr-2" />
                  {isLoadingA ? 'Generating...' : 'Generate'}
                </Button>

                {responseA && (
                  <Button
                    variant="outline"
                    className="w-full"
                    onClick={() => handleExport(configA, responseA)}
                  >
                    <Download className="h-4 w-4 mr-2" />
                    Export
                  </Button>
                )}
              </CardContent>
            </Card>

            {/* Recent Sessions */}
            {recentSessions.length > 0 && (
              <Card>
                <CardHeader>
                  <CardTitle className="text-base flex items-center gap-2">
                    <History className="h-4 w-4" />
                    Recent Sessions
                  </CardTitle>
                </CardHeader>
                <CardContent className="space-y-2">
                  {recentSessions.slice(0, 5).map((session) => (
                    <Button
                      key={session.id}
                      variant="ghost"
                      className="w-full justify-start text-left h-auto py-2"
                      onClick={() => loadSession(session)}
                    >
                      <div className="truncate">
                        <p className="text-sm truncate">{session.prompt}</p>
                        <p className="text-xs text-muted-foreground">
                          {new Date(session.created_at).toLocaleString()}
                        </p>
                      </div>
                    </Button>
                  ))}
                </CardContent>
              </Card>
            )}
          </div>

          {/* Response Panel */}
          <div className="lg:col-span-2">
            <Card className="min-h-[600px]">
              <CardHeader>
                <CardTitle className="text-base">Output</CardTitle>
              </CardHeader>
              <CardContent>
                {renderResponse(responseA, isLoadingA)}
              </CardContent>
            </Card>
          </div>
        </div>
      ) : (
        /* Comparison Mode */
        <div className="space-y-4">
          {/* Shared Prompt */}
          <Card>
            <CardHeader>
              <CardTitle className="text-base">Shared Prompt</CardTitle>
            </CardHeader>
            <CardContent>
              <Textarea
                placeholder="Enter prompt to compare..."
                value={prompt}
                onChange={(e) => {
                  setPrompt(e.target.value);
                  setConfigA({ ...configA, prompt: e.target.value });
                  setConfigB({ ...configB, prompt: e.target.value });
                }}
                rows={4}
              />
            </CardContent>
          </Card>

          {/* Side-by-Side Configurations */}
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {/* Config A */}
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle className="text-base">Configuration A</CardTitle>
                  <Badge>Temperature: {configA.temperature}</Badge>
                </div>
              </CardHeader>
              <CardContent className="space-y-4">
                {renderAdvancedOptions(configA, setConfigA)}
                <Button
                  className="w-full"
                  onClick={() => handleInfer(configA, setResponseA, setIsLoadingA)}
                  disabled={isLoadingA || !prompt.trim()}
                >
                  <Play className="h-4 w-4 mr-2" />
                  Generate A
                </Button>
                {renderResponse(responseA, isLoadingA)}
              </CardContent>
            </Card>

            {/* Config B */}
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle className="text-base">Configuration B</CardTitle>
                  <Badge>Temperature: {configB.temperature}</Badge>
                </div>
              </CardHeader>
              <CardContent className="space-y-4">
                {renderAdvancedOptions(configB, setConfigB)}
                <Button
                  className="w-full"
                  onClick={() => handleInfer(configB, setResponseB, setIsLoadingB)}
                  disabled={isLoadingB || !prompt.trim()}
                >
                  <Play className="h-4 w-4 mr-2" />
                  Generate B
                </Button>
                {renderResponse(responseB, isLoadingB)}
              </CardContent>
            </Card>
          </div>

          {/* Comparison Summary */}
          {responseA && responseB && (
            <Card>
              <CardHeader>
                <CardTitle className="text-base flex items-center gap-2">
                  <BarChart3 className="h-4 w-4" />
                  Comparison Summary
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                  <div>
                    <p className="text-sm font-medium">Latency</p>
                    <div className="flex items-center gap-2 mt-1">
                      <Badge variant="outline">A: {responseA.latency_ms || 0}ms</Badge>
                      <Badge variant="outline">B: {responseB.latency_ms || 0}ms</Badge>
                    </div>
                  </div>
                  <div>
                    <p className="text-sm font-medium">Tokens</p>
                    <div className="flex items-center gap-2 mt-1">
                      <Badge variant="outline">A: {responseA.token_count || 0}</Badge>
                      <Badge variant="outline">B: {responseB.token_count || 0}</Badge>
                    </div>
                  </div>
                  <div>
                    <p className="text-sm font-medium">Finish Reason</p>
                    <div className="flex items-center gap-2 mt-1">
                      <Badge variant="outline">{responseA.finish_reason || 'unknown'}</Badge>
                      <Badge variant="outline">{responseB.finish_reason || 'unknown'}</Badge>
                    </div>
                  </div>
                  <div>
                    <p className="text-sm font-medium">Winner</p>
                    <Badge className="mt-1">
                      {(responseA.latency_ms || 0) < (responseB.latency_ms || 0) ? 'A (Faster)' : 'B (Faster)'}
                    </Badge>
                  </div>
                </div>
              </CardContent>
            </Card>
          )}
        </div>
      )}
    </div>
  );
}

