/**
 * Example usage of InferenceStreaming component
 *
 * This file demonstrates various ways to use the InferenceStreaming component
 * for real-time token-by-token inference from the /v1/infer/stream endpoint.
 */

import React, { useState } from 'react';
import { InferenceStreaming } from './InferenceStreaming';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';

// ============================================================================
// Example 1: Basic Usage
// ============================================================================

function BasicExample() {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Basic Usage</CardTitle>
        <CardDescription>
          Simple streaming inference with default settings
        </CardDescription>
      </CardHeader>
      <CardContent>
        <InferenceStreaming
          prompt="Explain what LoRA adapters are in 3 sentences."
          maxTokens={150}
          temperature={0.7}
          autoStart={false}
        />
      </CardContent>
    </Card>
  );
}

// ============================================================================
// Example 2: With Adapters
// ============================================================================

function AdapterExample() {
  const [selectedAdapter, setSelectedAdapter] = useState('code-assistant');

  return (
    <Card>
      <CardHeader>
        <CardTitle>With Adapter Selection</CardTitle>
        <CardDescription>
          Use specific adapters for domain-specific inference
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-2">
          <Label>Select Adapter</Label>
          <Input
            value={selectedAdapter}
            onChange={(e) => setSelectedAdapter(e.target.value)}
            placeholder="Enter adapter ID"
          />
        </div>
        <InferenceStreaming
          prompt="Write a Python function to calculate Fibonacci numbers."
          adapters={selectedAdapter ? [selectedAdapter] : undefined}
          maxTokens={200}
          temperature={0.5}
        />
      </CardContent>
    </Card>
  );
}

// ============================================================================
// Example 3: Interactive Prompt
// ============================================================================

function InteractiveExample() {
  const [prompt, setPrompt] = useState('');
  const [submittedPrompt, setSubmittedPrompt] = useState('');

  const handleSubmit = () => {
    setSubmittedPrompt(prompt);
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Interactive Prompt</CardTitle>
        <CardDescription>
          Enter your own prompt and watch the streaming response
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-2">
          <Label>Your Prompt</Label>
          <Textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            placeholder="Enter your prompt here..."
            rows={4}
          />
          <Button onClick={handleSubmit} disabled={!prompt}>
            Set Prompt
          </Button>
        </div>
        {submittedPrompt && (
          <InferenceStreaming
            key={submittedPrompt} // Re-mount on new prompt
            prompt={submittedPrompt}
            maxTokens={500}
            temperature={0.7}
            onComplete={(text) => {
              console.log('Generation complete:', text);
            }}
            onError={(error) => {
              console.error('Generation error:', error);
            }}
          />
        )}
      </CardContent>
    </Card>
  );
}

// ============================================================================
// Example 4: With Callbacks
// ============================================================================

function CallbackExample() {
  const [completedText, setCompletedText] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  return (
    <Card>
      <CardHeader>
        <CardTitle>With Callbacks</CardTitle>
        <CardDescription>
          Handle completion and error events
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <InferenceStreaming
          prompt="What are the benefits of using streaming inference?"
          maxTokens={200}
          temperature={0.8}
          onComplete={(text) => {
            setCompletedText(text);
            setErrorMessage(null);
          }}
          onError={(error) => {
            setErrorMessage(error.message);
            setCompletedText(null);
          }}
        />

        {completedText && (
          <div className="rounded-md border border-green-200 bg-green-50 p-4">
            <p className="text-sm font-medium text-green-900">Completed!</p>
            <p className="mt-1 text-sm text-green-700">
              Generated {completedText.length} characters
            </p>
          </div>
        )}

        {errorMessage && (
          <div className="rounded-md border border-red-200 bg-red-50 p-4">
            <p className="text-sm font-medium text-red-900">Error occurred</p>
            <p className="mt-1 text-sm text-red-700">{errorMessage}</p>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

// ============================================================================
// Example 5: Auto-start
// ============================================================================

function AutoStartExample() {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Auto-start Streaming</CardTitle>
        <CardDescription>
          Automatically begin streaming on component mount
        </CardDescription>
      </CardHeader>
      <CardContent>
        <InferenceStreaming
          prompt="Describe the main advantages of server-sent events (SSE) over polling."
          maxTokens={250}
          temperature={0.6}
          autoStart={true}
        />
      </CardContent>
    </Card>
  );
}

// ============================================================================
// Example 6: Custom Styling
// ============================================================================

function CustomStylingExample() {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Custom Styling</CardTitle>
        <CardDescription>
          Override default styles with custom classes
        </CardDescription>
      </CardHeader>
      <CardContent>
        <InferenceStreaming
          prompt="List the top 5 programming languages for AI development."
          maxTokens={200}
          temperature={0.5}
          showTiming={true}
          showStatus={true}
          className="border-2 border-primary/20"
        />
      </CardContent>
    </Card>
  );
}

// ============================================================================
// Main Demo Component
// ============================================================================

export function InferenceStreamingExamples() {
  return (
    <div className="container mx-auto space-y-8 py-8">
      <div className="space-y-2">
        <h1 className="text-3xl font-bold">InferenceStreaming Examples</h1>
        <p className="text-muted-foreground">
          Explore different ways to use the InferenceStreaming component for
          real-time token-by-token inference.
        </p>
      </div>

      <Tabs defaultValue="basic" className="space-y-4">
        <TabsList className="grid w-full grid-cols-3 lg:grid-cols-6">
          <TabsTrigger value="basic">Basic</TabsTrigger>
          <TabsTrigger value="adapter">Adapters</TabsTrigger>
          <TabsTrigger value="interactive">Interactive</TabsTrigger>
          <TabsTrigger value="callbacks">Callbacks</TabsTrigger>
          <TabsTrigger value="autostart">Auto-start</TabsTrigger>
          <TabsTrigger value="custom">Custom</TabsTrigger>
        </TabsList>

        <TabsContent value="basic">
          <BasicExample />
        </TabsContent>

        <TabsContent value="adapter">
          <AdapterExample />
        </TabsContent>

        <TabsContent value="interactive">
          <InteractiveExample />
        </TabsContent>

        <TabsContent value="callbacks">
          <CallbackExample />
        </TabsContent>

        <TabsContent value="autostart">
          <AutoStartExample />
        </TabsContent>

        <TabsContent value="custom">
          <CustomStylingExample />
        </TabsContent>
      </Tabs>
    </div>
  );
}

export default InferenceStreamingExamples;
