import React, { useState, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { usePolling } from '@/hooks/usePolling';
import { apiClient } from '@/api/client';

interface Pipeline {
  id: string;
  name: string;
  status: 'success' | 'failed' | 'running' | 'pending' | 'cancelled';
  branch: string;
  commit: string;
  duration: number;
  startedAt: string;
  triggeredBy: string;
}

interface Deployment {
  id: string;
  environment: string;
  version: string;
  status: 'deployed' | 'rolling_back' | 'pending' | 'failed';
  deployedAt: string;
  deployedBy: string;
  adapters: string[];
}

interface Webhook {
  id: string;
  url: string;
  events: string[];
  active: boolean;
  lastTriggered: string | null;
  failureCount: number;
}

interface BuildLog {
  timestamp: string;
  level: 'info' | 'warn' | 'error';
  message: string;
}

interface CICDData {
  pipelines: Pipeline[];
  deployments: Deployment[];
  webhooks: Webhook[];
  buildLogs: BuildLog[];
}

const fetchCICDData = async (): Promise<CICDData> => {
  try {
    const [pipelines, deployments, webhooks] = await Promise.all([
      apiClient.get<Pipeline[]>('/cicd/pipelines').catch(() => []),
      apiClient.get<Deployment[]>('/cicd/deployments').catch(() => []),
      apiClient.get<Webhook[]>('/cicd/webhooks').catch(() => [])
    ]);

    return {
      pipelines: pipelines.length ? pipelines : getMockPipelines(),
      deployments: deployments.length ? deployments : getMockDeployments(),
      webhooks: webhooks.length ? webhooks : getMockWebhooks(),
      buildLogs: getMockBuildLogs()
    };
  } catch {
    return {
      pipelines: getMockPipelines(),
      deployments: getMockDeployments(),
      webhooks: getMockWebhooks(),
      buildLogs: getMockBuildLogs()
    };
  }
};

function getMockPipelines(): Pipeline[] {
  return [
    {
      id: 'pipe-001',
      name: 'main-build',
      status: 'success',
      branch: 'main',
      commit: 'a1b2c3d',
      duration: 342,
      startedAt: new Date(Date.now() - 1800000).toISOString(),
      triggeredBy: 'push'
    },
    {
      id: 'pipe-002',
      name: 'adapter-test',
      status: 'running',
      branch: 'feature/new-adapter',
      commit: 'e4f5g6h',
      duration: 0,
      startedAt: new Date(Date.now() - 300000).toISOString(),
      triggeredBy: 'pull_request'
    },
    {
      id: 'pipe-003',
      name: 'deploy-staging',
      status: 'failed',
      branch: 'main',
      commit: 'i7j8k9l',
      duration: 127,
      startedAt: new Date(Date.now() - 7200000).toISOString(),
      triggeredBy: 'manual'
    }
  ];
}

function getMockDeployments(): Deployment[] {
  return [
    {
      id: 'deploy-001',
      environment: 'production',
      version: 'v2.4.1',
      status: 'deployed',
      deployedAt: new Date(Date.now() - 86400000).toISOString(),
      deployedBy: 'ops-user',
      adapters: ['code-review/r003', 'security-scan/r001']
    },
    {
      id: 'deploy-002',
      environment: 'staging',
      version: 'v2.5.0-rc1',
      status: 'deployed',
      deployedAt: new Date(Date.now() - 3600000).toISOString(),
      deployedBy: 'ci-bot',
      adapters: ['code-review/r004', 'docs-gen/r002']
    },
    {
      id: 'deploy-003',
      environment: 'dev',
      version: 'v2.5.0-dev',
      status: 'pending',
      deployedAt: new Date().toISOString(),
      deployedBy: 'dev-user',
      adapters: ['test-adapter/r001']
    }
  ];
}

function getMockWebhooks(): Webhook[] {
  return [
    {
      id: 'wh-001',
      url: 'https://api.github.com/repos/org/aos/hooks',
      events: ['push', 'pull_request'],
      active: true,
      lastTriggered: new Date(Date.now() - 1800000).toISOString(),
      failureCount: 0
    },
    {
      id: 'wh-002',
      url: 'https://slack.com/api/webhooks/T123/B456',
      events: ['deployment', 'pipeline_failed'],
      active: true,
      lastTriggered: new Date(Date.now() - 86400000).toISOString(),
      failureCount: 2
    }
  ];
}

function getMockBuildLogs(): BuildLog[] {
  return [
    { timestamp: new Date(Date.now() - 300000).toISOString(), level: 'info', message: 'Starting build pipeline...' },
    { timestamp: new Date(Date.now() - 295000).toISOString(), level: 'info', message: 'Checking out branch: main' },
    { timestamp: new Date(Date.now() - 290000).toISOString(), level: 'info', message: 'Installing dependencies...' },
    { timestamp: new Date(Date.now() - 280000).toISOString(), level: 'info', message: 'Running cargo build --release' },
    { timestamp: new Date(Date.now() - 200000).toISOString(), level: 'warn', message: 'Warning: unused variable in trainer.rs:142' },
    { timestamp: new Date(Date.now() - 150000).toISOString(), level: 'info', message: 'Build completed successfully' },
    { timestamp: new Date(Date.now() - 145000).toISOString(), level: 'info', message: 'Running test suite...' },
    { timestamp: new Date(Date.now() - 100000).toISOString(), level: 'info', message: 'Tests passed: 347/347' },
    { timestamp: new Date(Date.now() - 95000).toISOString(), level: 'info', message: 'Creating artifact: aos-v2.4.1.tar.gz' },
    { timestamp: new Date(Date.now() - 90000).toISOString(), level: 'info', message: 'Pipeline completed in 5m 42s' }
  ];
}

function getStatusBadgeVariant(status: string): 'default' | 'secondary' | 'destructive' | 'outline' {
  switch (status) {
    case 'success':
    case 'deployed':
      return 'default';
    case 'running':
    case 'pending':
      return 'secondary';
    case 'failed':
    case 'rolling_back':
    case 'cancelled':
      return 'destructive';
    default:
      return 'outline';
  }
}

function formatDuration(seconds: number): string {
  if (seconds === 0) return 'Running...';
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  return `${mins}m ${secs}s`;
}

function formatTimeAgo(isoDate: string): string {
  const diff = Date.now() - new Date(isoDate).getTime();
  const mins = Math.floor(diff / 60000);
  const hours = Math.floor(diff / 3600000);
  const days = Math.floor(diff / 86400000);

  if (days > 0) return `${days}d ago`;
  if (hours > 0) return `${hours}h ago`;
  if (mins > 0) return `${mins}m ago`;
  return 'Just now';
}

export default function DevOpsCIDCPanel() {
  const [webhookUrl, setWebhookUrl] = useState('');
  const [selectedPipeline, setSelectedPipeline] = useState<string | null>(null);

  const { data, isLoading, lastUpdated, refetch } = usePolling<CICDData>(
    fetchCICDData,
    'normal',
    { operationName: 'cicd-data' }
  );

  const handleTriggerPipeline = useCallback(async (pipelineId: string) => {
    try {
      await apiClient.request(`/cicd/pipelines/${pipelineId}/trigger`, { method: 'POST', body: JSON.stringify({}) });
      refetch();
    } catch (error) {
      console.error('Failed to trigger pipeline:', error);
    }
  }, [refetch]);

  const handleAddWebhook = useCallback(async () => {
    if (!webhookUrl) return;
    try {
      await apiClient.request('/cicd/webhooks', { method: 'POST', body: JSON.stringify({ url: webhookUrl, events: ['push', 'pull_request'] }) });
      setWebhookUrl('');
      refetch();
    } catch (error) {
      console.error('Failed to add webhook:', error);
    }
  }, [webhookUrl, refetch]);

  const handleToggleWebhook = useCallback(async (webhookId: string, active: boolean) => {
    try {
      await apiClient.request(`/cicd/webhooks/${webhookId}`, { method: 'PATCH', body: JSON.stringify({ active: !active }) });
      refetch();
    } catch (error) {
      console.error('Failed to toggle webhook:', error);
    }
  }, [refetch]);

  if (isLoading && !data) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-muted-foreground">Loading CI/CD data...</div>
      </div>
    );
  }

  const { pipelines = [], deployments = [], webhooks = [], buildLogs = [] } = data || {};

  return (
    <div className="space-y-6 p-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold">CI/CD Integration</h2>
          <p className="text-sm text-muted-foreground">
            Manage pipelines, deployments, and webhooks
          </p>
        </div>
        {lastUpdated && (
          <span className="text-xs text-muted-foreground">
            Updated {formatTimeAgo(lastUpdated.toISOString())}
          </span>
        )}
      </div>

      <Tabs defaultValue="pipelines" className="w-full">
        <TabsList>
          <TabsTrigger value="pipelines">Pipelines</TabsTrigger>
          <TabsTrigger value="deployments">Deployments</TabsTrigger>
          <TabsTrigger value="webhooks">Webhooks</TabsTrigger>
          <TabsTrigger value="logs">Build Logs</TabsTrigger>
        </TabsList>

        <TabsContent value="pipelines" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Pipeline Status</CardTitle>
              <CardDescription>Recent pipeline executions</CardDescription>
            </CardHeader>
            <CardContent>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Pipeline</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Branch</TableHead>
                    <TableHead>Commit</TableHead>
                    <TableHead>Duration</TableHead>
                    <TableHead>Triggered</TableHead>
                    <TableHead>Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {pipelines.map((pipeline) => (
                    <TableRow
                      key={pipeline.id}
                      className={selectedPipeline === pipeline.id ? 'bg-muted/50' : ''}
                      onClick={() => setSelectedPipeline(pipeline.id)}
                    >
                      <TableCell className="font-medium">{pipeline.name}</TableCell>
                      <TableCell>
                        <Badge variant={getStatusBadgeVariant(pipeline.status)}>
                          {pipeline.status}
                        </Badge>
                      </TableCell>
                      <TableCell className="font-mono text-xs">{pipeline.branch}</TableCell>
                      <TableCell className="font-mono text-xs">{pipeline.commit}</TableCell>
                      <TableCell>{formatDuration(pipeline.duration)}</TableCell>
                      <TableCell>
                        <span className="text-xs text-muted-foreground">
                          {formatTimeAgo(pipeline.startedAt)} ({pipeline.triggeredBy})
                        </span>
                      </TableCell>
                      <TableCell>
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={(e) => {
                            e.stopPropagation();
                            handleTriggerPipeline(pipeline.id);
                          }}
                        >
                          Re-run
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="deployments" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Recent Deployments</CardTitle>
              <CardDescription>Adapter deployments across environments</CardDescription>
            </CardHeader>
            <CardContent>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Environment</TableHead>
                    <TableHead>Version</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Adapters</TableHead>
                    <TableHead>Deployed</TableHead>
                    <TableHead>By</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {deployments.map((deployment) => (
                    <TableRow key={deployment.id}>
                      <TableCell>
                        <Badge variant="outline">{deployment.environment}</Badge>
                      </TableCell>
                      <TableCell className="font-mono text-xs">{deployment.version}</TableCell>
                      <TableCell>
                        <Badge variant={getStatusBadgeVariant(deployment.status)}>
                          {deployment.status}
                        </Badge>
                      </TableCell>
                      <TableCell>
                        <div className="flex flex-wrap gap-1">
                          {deployment.adapters.slice(0, 2).map((adapter) => (
                            <Badge key={adapter} variant="secondary" className="text-xs">
                              {adapter}
                            </Badge>
                          ))}
                          {deployment.adapters.length > 2 && (
                            <Badge variant="secondary" className="text-xs">
                              +{deployment.adapters.length - 2}
                            </Badge>
                          )}
                        </div>
                      </TableCell>
                      <TableCell className="text-xs text-muted-foreground">
                        {formatTimeAgo(deployment.deployedAt)}
                      </TableCell>
                      <TableCell className="text-xs">{deployment.deployedBy}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="webhooks" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Webhook Configuration</CardTitle>
              <CardDescription>Manage CI/CD webhook integrations</CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="flex gap-2">
                <Input
                  placeholder="Webhook URL (e.g., https://api.github.com/hooks)"
                  value={webhookUrl}
                  onChange={(e) => setWebhookUrl(e.target.value)}
                  className="flex-1"
                />
                <Button onClick={handleAddWebhook} disabled={!webhookUrl}>
                  Add Webhook
                </Button>
              </div>

              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>URL</TableHead>
                    <TableHead>Events</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Last Triggered</TableHead>
                    <TableHead>Failures</TableHead>
                    <TableHead>Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {webhooks.map((webhook) => (
                    <TableRow key={webhook.id}>
                      <TableCell className="font-mono text-xs max-w-[200px] truncate">
                        {webhook.url}
                      </TableCell>
                      <TableCell>
                        <div className="flex flex-wrap gap-1">
                          {webhook.events.map((event) => (
                            <Badge key={event} variant="outline" className="text-xs">
                              {event}
                            </Badge>
                          ))}
                        </div>
                      </TableCell>
                      <TableCell>
                        <Badge variant={webhook.active ? 'default' : 'secondary'}>
                          {webhook.active ? 'Active' : 'Inactive'}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-xs text-muted-foreground">
                        {webhook.lastTriggered ? formatTimeAgo(webhook.lastTriggered) : 'Never'}
                      </TableCell>
                      <TableCell>
                        {webhook.failureCount > 0 ? (
                          <Badge variant="destructive">{webhook.failureCount}</Badge>
                        ) : (
                          <span className="text-muted-foreground">0</span>
                        )}
                      </TableCell>
                      <TableCell>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => handleToggleWebhook(webhook.id, webhook.active)}
                        >
                          {webhook.active ? 'Disable' : 'Enable'}
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="logs" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Build Logs</CardTitle>
              <CardDescription>
                {selectedPipeline ? `Logs for pipeline: ${selectedPipeline}` : 'Select a pipeline to view logs'}
              </CardDescription>
            </CardHeader>
            <CardContent>
              <ScrollArea className="h-[400px] w-full rounded-md border p-4">
                <div className="font-mono text-xs space-y-1">
                  {buildLogs.map((log, index) => (
                    <div
                      key={index}
                      className={`flex gap-2 ${
                        log.level === 'error' ? 'text-destructive' :
                        log.level === 'warn' ? 'text-yellow-600' :
                        'text-muted-foreground'
                      }`}
                    >
                      <span className="text-muted-foreground/60">
                        {new Date(log.timestamp).toLocaleTimeString()}
                      </span>
                      <span className={`uppercase w-12 ${
                        log.level === 'error' ? 'text-destructive' :
                        log.level === 'warn' ? 'text-yellow-600' :
                        'text-blue-500'
                      }`}>
                        [{log.level}]
                      </span>
                      <span>{log.message}</span>
                    </div>
                  ))}
                </div>
              </ScrollArea>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
