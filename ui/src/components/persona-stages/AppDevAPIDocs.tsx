import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../ui/card';
import { Button } from '../ui/button';
import { Badge } from '../ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../ui/tabs';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import { Code, Copy, ExternalLink, Book, Zap, Users } from 'lucide-react';

export default function AppDevAPIDocs() {
  return (
    <div className="space-y-4 h-full">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center space-x-2">
          <Book className="h-5 w-5" />
          <h2 className="text-lg font-semibold">AdapterOS API Documentation</h2>
        </div>
        <Select defaultValue="javascript">
          <SelectTrigger className="w-32">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="javascript">JavaScript</SelectItem>
            <SelectItem value="python">Python</SelectItem>
            <SelectItem value="go">Go</SelectItem>
            <SelectItem value="rust">Rust</SelectItem>
          </SelectContent>
        </Select>
      </div>

      <Tabs defaultValue="inference" className="h-full">
        <TabsList className="grid w-full grid-cols-4">
          <TabsTrigger value="inference">Inference</TabsTrigger>
          <TabsTrigger value="adapters">Adapters</TabsTrigger>
          <TabsTrigger value="metrics">Metrics</TabsTrigger>
          <TabsTrigger value="auth">Auth</TabsTrigger>
        </TabsList>

        <TabsContent value="inference" className="space-y-4 mt-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center space-x-2">
                <Zap className="h-4 w-4" />
                <span>POST /v1/inference</span>
                <Badge>Core</Badge>
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <p className="text-sm text-muted-foreground">
                Run inference with a specific adapter configuration.
              </p>

              <div>
                <h4 className="font-medium mb-2">Request Body</h4>
                <div className="bg-slate-900 text-green-400 p-3 rounded font-mono text-sm overflow-x-auto">
                  <div>{'{'}</div>
                  <div className="ml-4">"adapter_id": "my_adapter_v1",</div>
                  <div className="ml-4">"prompt": "Hello, how are you?",</div>
                  <div className="ml-4">"max_tokens": 100,</div>
                  <div className="ml-4">"temperature": 0.7</div>
                  <div>{'}'}</div>
                </div>
                <Button variant="ghost" size="sm" className="mt-2">
                  <Copy className="h-3 w-3 mr-1" />
                  Copy
                </Button>
              </div>

              <div>
                <h4 className="font-medium mb-2">Response</h4>
                <div className="bg-slate-900 text-blue-400 p-3 rounded font-mono text-sm overflow-x-auto">
                  <div>{'{'}</div>
                  <div className="ml-4">"text": "Hello! I'm doing well, thank you for asking...",</div>
                  <div className="ml-4">"tokens_used": 24,</div>
                  <div className="ml-4">"adapter_used": "my_adapter_v1",</div>
                  <div className="ml-4">"processing_time_ms": 150</div>
                  <div>{'}'}</div>
                </div>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="adapters" className="space-y-4 mt-4">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center space-x-2">
                <Users className="h-4 w-4" />
                <span>GET /v1/adapters</span>
                <Badge variant="secondary">Management</Badge>
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <p className="text-sm text-muted-foreground">
                List available adapters for your tenant.
              </p>

              <div>
                <h4 className="font-medium mb-2">Query Parameters</h4>
                <div className="space-y-1 text-sm">
                  <div><code>?status=active</code> - Filter by status</div>
                  <div><code>?base_model=llama-2-7b</code> - Filter by base model</div>
                  <div><code>?limit=50</code> - Limit results</div>
                </div>
              </div>

              <div>
                <h4 className="font-medium mb-2">Example Response</h4>
                <div className="bg-slate-900 text-yellow-400 p-3 rounded font-mono text-sm overflow-x-auto">
                  <div>[</div>
                  <div className="ml-4">{'{'}</div>
                  <div className="ml-8">"id": "my_adapter_v1",</div>
                  <div className="ml-8">"name": "Custom Chat Adapter",</div>
                  <div className="ml-8">"base_model": "llama-2-7b",</div>
                  <div className="ml-8">"status": "active",</div>
                  <div className="ml-8">"created_at": "2024-01-15T10:30:00Z"</div>
                  <div className="ml-4">{'}'}</div>
                  <div>]</div>
                </div>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="metrics" className="space-y-4 mt-4">
          <Card>
            <CardHeader>
              <CardTitle>GET /v1/metrics</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-sm text-muted-foreground mb-4">
                Retrieve system and adapter performance metrics.
              </p>
              <div className="text-center py-8 text-muted-foreground">
                <Code className="h-8 w-8 mx-auto mb-2" />
                <p>Interactive metrics explorer coming soon...</p>
              </div>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="auth" className="space-y-4 mt-4">
          <Card>
            <CardHeader>
              <CardTitle>Authentication</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-sm text-muted-foreground mb-4">
                All API requests require JWT authentication.
              </p>
              <div className="space-y-2">
                <div className="flex items-center space-x-2">
                  <Badge>Header</Badge>
                  <code className="text-sm">Authorization: Bearer {'<jwt_token>'}</code>
                </div>
                <div className="flex items-center space-x-2">
                  <Badge variant="secondary">Cookie</Badge>
                  <code className="text-sm">adapteros_session</code>
                </div>
              </div>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      <div className="flex space-x-2">
        <Button variant="outline" className="flex-1">
          <ExternalLink className="h-4 w-4 mr-2" />
          View Full API Reference
        </Button>
        <Button>
          <Code className="h-4 w-4 mr-2" />
          Try It Out
        </Button>
      </div>
    </div>
  );
}
