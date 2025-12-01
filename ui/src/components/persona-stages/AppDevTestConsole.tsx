import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';
import { apiClient } from '@/api/client';
import type { Adapter } from '@/api/adapter-types';
import type { InferResponse } from '@/api/api-types';
import { Play, Loader2, Clock, Zap } from 'lucide-react';

interface TestResult {
  id: string;
  text: string;
  tokens_generated: number;
  latency_ms: number;
  adapters_used: string[];
  finish_reason: string;
  timestamp: Date;
}

export default function AppDevTestConsole() {
  const [prompt, setPrompt] = useState('');
  const [selectedAdapter, setSelectedAdapter] = useState<string>('');
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isLoadingAdapters, setIsLoadingAdapters] = useState(true);
  const [result, setResult] = useState<TestResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const loadAdapters = async () => {
      try {
        const adapterList = await apiClient.listAdapters();
        setAdapters(adapterList);
      } catch (err) {
        setError('Failed to load adapters');
      } finally {
        setIsLoadingAdapters(false);
      }
    };
    loadAdapters();
  }, []);

  const handleTest = async () => {
    if (!prompt.trim()) {
      setError('Please enter a prompt');
      return;
    }

    setIsLoading(true);
    setError(null);
    setResult(null);

    try {
      const response: InferResponse = await apiClient.infer({
        prompt: prompt.trim(),
        adapters: selectedAdapter ? [selectedAdapter] : undefined,
        max_tokens: 256,
        temperature: 0.7,
      });

      setResult({
        id: response.id,
        text: response.text,
        tokens_generated: response.tokens_generated,
        latency_ms: response.latency_ms,
        adapters_used: response.adapters_used,
        finish_reason: response.finish_reason,
        timestamp: new Date(),
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Inference request failed');
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="space-y-4 p-4">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Play className="h-5 w-5" />
            Test Console
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <label className="text-sm font-medium">Adapter</label>
            <Select value={selectedAdapter || "__none__"} onValueChange={(v) => setSelectedAdapter(v === "__none__" ? "" : v)}>
              <SelectTrigger>
                <SelectValue placeholder={isLoadingAdapters ? 'Loading...' : 'Select adapter (optional)'} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__none__">No adapter (base model)</SelectItem>
                {adapters.map((adapter) => (
                  <SelectItem key={adapter.id} value={adapter.id}>
                    {adapter.id}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-2">
            <label className="text-sm font-medium">Prompt</label>
            <Textarea
              placeholder="Enter your test prompt..."
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              rows={4}
              className="resize-y"
            />
          </div>

          <Button
            onClick={handleTest}
            disabled={isLoading || !prompt.trim()}
            className="w-full"
          >
            {isLoading ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                Running...
              </>
            ) : (
              <>
                <Play className="h-4 w-4" />
                Run Test
              </>
            )}
          </Button>

          {error && (
            <div className="rounded-md bg-destructive/10 p-3 text-sm text-destructive">
              {error}
            </div>
          )}
        </CardContent>
      </Card>

      {result && (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Results</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex flex-wrap gap-2">
              <Badge variant="outline" className="flex items-center gap-1">
                <Zap className="h-3 w-3" />
                {result.tokens_generated} tokens
              </Badge>
              <Badge variant="outline" className="flex items-center gap-1">
                <Clock className="h-3 w-3" />
                {result.latency_ms.toFixed(0)}ms
              </Badge>
              <Badge variant={result.finish_reason === 'stop' ? 'default' : 'secondary'}>
                {result.finish_reason}
              </Badge>
            </div>

            {result.adapters_used.length > 0 && (
              <div className="space-y-1">
                <span className="text-xs text-muted-foreground">Adapters used:</span>
                <div className="flex flex-wrap gap-1">
                  {result.adapters_used.map((adapter) => (
                    <Badge key={adapter} variant="secondary" className="text-xs">
                      {adapter}
                    </Badge>
                  ))}
                </div>
              </div>
            )}

            <div className="space-y-1">
              <span className="text-xs text-muted-foreground">Response:</span>
              <div className="rounded-md bg-muted p-3 text-sm whitespace-pre-wrap">
                {result.text}
              </div>
            </div>

            <div className="text-xs text-muted-foreground">
              Request ID: {result.id}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
