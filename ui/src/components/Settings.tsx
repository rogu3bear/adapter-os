import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from './ui/card';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Textarea } from './ui/textarea';
import { Switch } from './ui/switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { Alert, AlertDescription } from './ui/alert';
import { Settings as SettingsIcon, Save, RefreshCw, MessageSquare, Zap, Shield } from 'lucide-react';
import { toast } from 'sonner';
import { User } from '../api/types';

interface SettingsProps {
  user: User;
  selectedTenant: string;
}

export function Settings({ user, selectedTenant }: SettingsProps) {
  // System Prompt Settings
  const [systemPrompt, setSystemPrompt] = useState('');
  const [autoSave, setAutoSave] = useState(false);
  const [saving, setSaving] = useState(false);
  
  // AI Assistant Settings
  const [enableAITools, setEnableAITools] = useState(true);
  const [maxTokens, setMaxTokens] = useState(2000);
  const [temperature, setTemperature] = useState(0.7);
  
  // Load settings
  useEffect(() => {
    loadSettings();
  }, [selectedTenant]);
  
  const loadSettings = async () => {
    try {
      // Load system prompt from local storage or API
      const stored = localStorage.getItem(`system_prompt_${selectedTenant}`);
      if (stored) {
        setSystemPrompt(stored);
      } else {
        // Default prompt
        setSystemPrompt(`You are an AI assistant for AdapterOS, a system for managing LoRA adapters and ML infrastructure.

You have access to tools for:
- Managing LoRA adapters (register, delete, promote, monitor health)
- Viewing system metrics and performance data
- Analyzing adapter activations and routing decisions
- Monitoring training jobs and repository scans

Be concise, technical, and action-oriented. Always cite metrics and data when available.`);
      }
      
      // Load other settings
      const aiSettings = localStorage.getItem(`ai_settings_${selectedTenant}`);
      if (aiSettings) {
        const parsed = JSON.parse(aiSettings);
        setEnableAITools(parsed.enableAITools ?? true);
        setMaxTokens(parsed.maxTokens ?? 2000);
        setTemperature(parsed.temperature ?? 0.7);
      }
    } catch (error) {
      console.error('Failed to load settings:', error);
      toast.error('Failed to load settings');
    }
  };
  
  const saveSystemPrompt = async () => {
    setSaving(true);
    try {
      localStorage.setItem(`system_prompt_${selectedTenant}`, systemPrompt);
      toast.success('System prompt saved successfully');
    } catch (error) {
      console.error('Failed to save system prompt:', error);
      toast.error('Failed to save system prompt');
    } finally {
      setSaving(false);
    }
  };
  
  const saveAISettings = async () => {
    try {
      const settings = {
        enableAITools,
        maxTokens,
        temperature,
      };
      localStorage.setItem(`ai_settings_${selectedTenant}`, JSON.stringify(settings));
      toast.success('AI settings saved successfully');
    } catch (error) {
      console.error('Failed to save AI settings:', error);
      toast.error('Failed to save AI settings');
    }
  };
  
  const resetToDefault = () => {
    setSystemPrompt(`You are an AI assistant for AdapterOS, a system for managing LoRA adapters and ML infrastructure.

You have access to tools for:
- Managing LoRA adapters (register, delete, promote, monitor health)
- Viewing system metrics and performance data
- Analyzing adapter activations and routing decisions
- Monitoring training jobs and repository scans

Be concise, technical, and action-oriented. Always cite metrics and data when available.`);
    toast.info('Reset to default system prompt');
  };
  
  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex-between section-header">
        <div>
          <h1 className="section-title flex items-center gap-2">
            <SettingsIcon className="icon-standard" />
            Settings
          </h1>
          <p className="section-description">
            Configure system prompt and AI assistant behavior
          </p>
        </div>
      </div>
      
      <Tabs defaultValue="system-prompt" className="w-full">
        <TabsList className="grid w-full grid-cols-3">
          <TabsTrigger value="system-prompt">
            <MessageSquare className="icon-small mr-2" />
            System Prompt
          </TabsTrigger>
          <TabsTrigger value="ai-tools">
            <Zap className="icon-small mr-2" />
            AI Tools
          </TabsTrigger>
          <TabsTrigger value="security">
            <Shield className="icon-small mr-2" />
            Security
          </TabsTrigger>
        </TabsList>
        
        {/* System Prompt Tab */}
        <TabsContent value="system-prompt" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>System Prompt Configuration</CardTitle>
              <CardDescription>
                Define the AI assistant's behavior, personality, and available capabilities
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <Alert>
                <AlertDescription>
                  The system prompt is sent with every AI request. It should describe the assistant's role,
                  available tools, and guidelines for responses.
                </AlertDescription>
              </Alert>
              
              <div className="space-y-2">
                <Label htmlFor="system-prompt">System Prompt</Label>
                <Textarea
                  id="system-prompt"
                  value={systemPrompt}
                  onChange={(e) => setSystemPrompt(e.target.value)}
                  rows={15}
                  className="font-mono text-sm"
                  placeholder="Enter system prompt..."
                />
                <p className="text-sm text-muted-foreground">
                  {systemPrompt.length} characters · {systemPrompt.split(/\s+/).length} words
                </p>
              </div>
              
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  <Switch
                    id="auto-save"
                    checked={autoSave}
                    onCheckedChange={setAutoSave}
                  />
                  <Label htmlFor="auto-save">Auto-save changes</Label>
                </div>
              </div>
              
              <div className="flex gap-2">
                <Button onClick={saveSystemPrompt} disabled={saving}>
                  <Save className="icon-small mr-2" />
                  {saving ? 'Saving...' : 'Save Prompt'}
                </Button>
                <Button variant="outline" onClick={resetToDefault}>
                  <RefreshCw className="icon-small mr-2" />
                  Reset to Default
                </Button>
              </div>
            </CardContent>
          </Card>
          
          {/* Prompt Variables */}
          <Card>
            <CardHeader>
              <CardTitle>Available Variables</CardTitle>
              <CardDescription>
                These variables are automatically replaced when the prompt is sent
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-2 text-sm font-mono">
                <div className="flex justify-between">
                  <span className="text-muted-foreground">{'{{user_name}}'}</span>
                  <span>{user.display_name}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">{'{{user_role}}'}</span>
                  <span>{user.role}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">{'{{tenant_id}}'}</span>
                  <span>{selectedTenant}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">{'{{timestamp}}'}</span>
                  <span>{new Date().toISOString()}</span>
                </div>
              </div>
            </CardContent>
          </Card>
        </TabsContent>
        
        {/* AI Tools Tab */}
        <TabsContent value="ai-tools" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>AI Tool Configuration</CardTitle>
              <CardDescription>
                Configure which tools the AI assistant can use
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-6">
              <div className="flex items-center justify-between">
                <div>
                  <Label htmlFor="enable-tools">Enable AI Tools</Label>
                  <p className="text-sm text-muted-foreground">
                    Allow AI to manage adapters and system operations
                  </p>
                </div>
                <Switch
                  id="enable-tools"
                  checked={enableAITools}
                  onCheckedChange={setEnableAITools}
                />
              </div>
              
              <div className="space-y-2">
                <Label htmlFor="max-tokens">Max Tokens</Label>
                <Input
                  id="max-tokens"
                  type="number"
                  value={maxTokens}
                  onChange={(e) => setMaxTokens(parseInt(e.target.value))}
                  min={100}
                  max={8000}
                />
                <p className="text-sm text-muted-foreground">
                  Maximum response length (100-8000)
                </p>
              </div>
              
              <div className="space-y-2">
                <Label htmlFor="temperature">Temperature: {temperature}</Label>
                <Input
                  id="temperature"
                  type="range"
                  min="0"
                  max="1"
                  step="0.1"
                  value={temperature}
                  onChange={(e) => setTemperature(parseFloat(e.target.value))}
                />
                <p className="text-sm text-muted-foreground">
                  Controls randomness: 0 = focused, 1 = creative
                </p>
              </div>
              
              <Button onClick={saveAISettings}>
                <Save className="icon-small mr-2" />
                Save AI Settings
              </Button>
            </CardContent>
          </Card>
          
          {/* Available Tools */}
          <Card>
            <CardHeader>
              <CardTitle>Available AI Tools</CardTitle>
              <CardDescription>
                Tools the AI can use to manage the system
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-4">
                {[
                  { name: 'list_adapters', description: 'List all LoRA adapters' },
                  { name: 'register_adapter', description: 'Register new adapter' },
                  { name: 'delete_adapter', description: 'Remove adapter from system' },
                  { name: 'promote_adapter', description: 'Promote adapter tier' },
                  { name: 'get_adapter_health', description: 'Check adapter health metrics' },
                  { name: 'get_system_metrics', description: 'Get real-time system metrics' },
                  { name: 'get_routing_decisions', description: 'View adapter routing history' },
                  { name: 'debug_routing', description: 'Debug adapter routing for a prompt' },
                ].map((tool) => (
                  <div key={tool.name} className="flex items-start gap-3 p-3 rounded-lg border">
                    <Zap className="icon-small mt-0.5 text-primary" />
                    <div>
                      <p className="font-medium text-sm">{tool.name}</p>
                      <p className="text-sm text-muted-foreground">{tool.description}</p>
                    </div>
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>
        </TabsContent>
        
        {/* Security Tab */}
        <TabsContent value="security" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Security Settings</CardTitle>
              <CardDescription>
                Configure security and access controls
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <Alert>
                <AlertDescription>
                  AI tool access is controlled by your user role. Admin role required for destructive operations.
                </AlertDescription>
              </Alert>
              
              <div className="space-y-2">
                <Label>Your Permissions</Label>
                <div className="space-y-1 text-sm">
                  <div className="flex items-center gap-2">
                    <span className="font-mono">READ</span>
                    <span className="text-muted-foreground">View adapters and metrics</span>
                  </div>
                  {user.role === 'admin' && (
                    <>
                      <div className="flex items-center gap-2">
                        <span className="font-mono">WRITE</span>
                        <span className="text-muted-foreground">Register and modify adapters</span>
                      </div>
                      <div className="flex items-center gap-2">
                        <span className="font-mono">DELETE</span>
                        <span className="text-muted-foreground">Remove adapters from system</span>
                      </div>
                    </>
                  )}
                </div>
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}

