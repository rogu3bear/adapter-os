/**
 * ViewerDashboard - Read-only dashboard for Viewer role
 *
 * Provides simplified, read-only view for users with minimal permissions:
 * - System overview (status summary)
 * - Available adapters list
 * - Recent chat sessions
 * - Getting started guide
 * - Limited quick actions (view-only operations)
 *
 * Citation: 【2025-11-25†role-dashboard†viewer】
 */

import React from 'react';
import { useNavigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Skeleton } from '@/components/ui/skeleton';
import { KpiGrid, ContentGrid } from '@/components/ui/grid';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import {
  MessageSquare,
  Layers,
  BookOpen,
  CheckCircle,
  Activity,
  Eye,
  Clock,
  TrendingUp
} from 'lucide-react';
import { apiClient } from '@/api/services';
import { useChatSessionsApi } from '@/hooks/chat/useChatSessionsApi';
import { logger } from '@/utils/logger';
import DashboardLayout from '@/components/dashboard/DashboardLayout';
import {
  buildChatLink,
  buildAdaptersListLink,
  buildTelemetryViewerLink,
} from '@/utils/navLinks';

export default function ViewerDashboard() {
  const navigate = useNavigate();

  // Fetch system metrics (read-only)
  const { data: systemMetrics, isLoading: metricsLoading } = useQuery({
    queryKey: ['system-metrics'],
    queryFn: () => apiClient.getSystemMetrics(),
    refetchInterval: 30000, // Refresh every 30 seconds
  });

  // Fetch adapters (read-only)
  const { data: adapters = [], isLoading: adaptersLoading } = useQuery({
    queryKey: ['adapters', 'viewer'],
    queryFn: () => apiClient.listAdapters(),
    staleTime: 60000,
  });

  // Fetch recent chat sessions
  const { sessions, isLoading: sessionsLoading } = useChatSessionsApi('default');

  // Recent sessions (last 5)
  const recentSessions = sessions.slice(0, 5);

  // Quick actions for Viewer role (read-only operations)
  const quickActions = [
    {
      label: 'Start Chat',
      icon: MessageSquare,
      onClick: () => navigate(buildChatLink()),
      description: 'Begin a conversation with the model',
    },
    {
      label: 'Browse Adapters',
      icon: Layers,
      onClick: () => navigate(buildAdaptersListLink()),
      description: 'View available adapters',
    },
    {
      label: 'View Documentation',
      icon: BookOpen,
      onClick: () => window.open('https://docs.adapteros.local', '_blank'),
      description: 'Access user documentation',
    },
    {
      label: 'Telemetry Viewer',
      icon: Eye,
      onClick: () => navigate(buildTelemetryViewerLink()),
      description: 'Inspect per-session routing and tokens',
    },
  ];

  return (
    <DashboardLayout
      title="Dashboard"
      quickActions={
        <div className="flex gap-2">
          {quickActions.map((action) => {
            const Icon = action.icon;
            return (
              <GlossaryTooltip key={action.label} brief={action.description}>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={action.onClick}
                >
                  <Icon className="h-4 w-4 mr-2" />
                  <span className="hidden sm:inline">{action.label}</span>
                </Button>
              </GlossaryTooltip>
            );
          })}
        </div>
      }
    >
      {/* System Overview */}
      <div className="space-y-6">
        {/* Status Summary KPIs */}
        <div>
          <h2 className="text-lg font-semibold mb-4">System Overview</h2>
          <KpiGrid>
            {/* System Status */}
            <Card>
              <CardHeader className="pb-2">
                <GlossaryTooltip termId="system-status">
                  <CardTitle className="text-sm font-medium cursor-help flex items-center gap-2">
                    <CheckCircle className="h-4 w-4 text-green-600" />
                    System Status
                  </CardTitle>
                </GlossaryTooltip>
              </CardHeader>
              <CardContent>
                {metricsLoading ? (
                  <Skeleton className="h-8 w-24" />
                ) : (
                  <>
                    <div className="text-2xl font-bold text-green-600">Operational</div>
                    <p className="text-xs text-muted-foreground mt-1">
                      All systems running normally
                    </p>
                  </>
                )}
              </CardContent>
            </Card>

            {/* Available Adapters */}
            <Card>
              <CardHeader className="pb-2">
                <GlossaryTooltip termId="adapter-count">
                  <CardTitle className="text-sm font-medium cursor-help flex items-center gap-2">
                    <Layers className="h-4 w-4 text-purple-600" />
                    Available Adapters
                  </CardTitle>
                </GlossaryTooltip>
              </CardHeader>
              <CardContent>
                {adaptersLoading ? (
                  <Skeleton className="h-8 w-16" />
                ) : (
                  <>
                    <div className="text-2xl font-bold text-purple-600">{adapters.length}</div>
                    <p className="text-xs text-muted-foreground mt-1">
                      Ready for inference
                    </p>
                  </>
                )}
              </CardContent>
            </Card>

            {/* Active Sessions */}
            <Card>
              <CardHeader className="pb-2">
                <GlossaryTooltip termId="active-sessions">
                  <CardTitle className="text-sm font-medium cursor-help flex items-center gap-2">
                    <Activity className="h-4 w-4 text-blue-600" />
                    Active Sessions
                  </CardTitle>
                </GlossaryTooltip>
              </CardHeader>
              <CardContent>
                {metricsLoading ? (
                  <Skeleton className="h-8 w-16" />
                ) : (
                  <>
                    <div className="text-2xl font-bold text-blue-600">
                      {systemMetrics?.active_sessions || 0}
                    </div>
                    <p className="text-xs text-muted-foreground mt-1">
                      Current active users
                    </p>
                  </>
                )}
              </CardContent>
            </Card>

            {/* Performance */}
            <Card>
              <CardHeader className="pb-2">
                <GlossaryTooltip termId="tokens-per-second">
                  <CardTitle className="text-sm font-medium cursor-help flex items-center gap-2">
                    <TrendingUp className="h-4 w-4 text-green-600" />
                    Performance
                  </CardTitle>
                </GlossaryTooltip>
              </CardHeader>
              <CardContent>
                {metricsLoading ? (
                  <Skeleton className="h-8 w-20" />
                ) : (
                  <>
                    <div className="text-2xl font-bold text-green-600">
                      {systemMetrics?.tokens_per_second?.toFixed(0) || 0}
                    </div>
                    <p className="text-xs text-muted-foreground mt-1">
                      tokens/sec
                    </p>
                  </>
                )}
              </CardContent>
            </Card>
          </KpiGrid>
        </div>

        {/* Main Content Grid */}
        <ContentGrid>
          {/* Getting Started Guide */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <BookOpen className="h-5 w-5" />
                Getting Started
              </CardTitle>
              <CardDescription>
                New to AdapterOS? Start here to learn the basics
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-3">
                <div className="flex items-start gap-3 p-3 bg-muted/50 rounded-lg">
                  <div className="flex-shrink-0 w-6 h-6 rounded-full bg-primary text-primary-foreground flex items-center justify-center text-xs font-bold">
                    1
                  </div>
                  <div className="flex-1">
                    <h4 className="font-medium text-sm mb-1">Browse Adapters</h4>
                    <p className="text-xs text-muted-foreground">
                      Explore available adapters trained for different tasks and domains
                    </p>
                    <Button
                      variant="link"
                      size="sm"
                      className="px-0 mt-1"
                      onClick={() => navigate(buildAdaptersListLink())}
                    >
                      View Adapters →
                    </Button>
                  </div>
                </div>

                <div className="flex items-start gap-3 p-3 bg-muted/50 rounded-lg">
                  <div className="flex-shrink-0 w-6 h-6 rounded-full bg-primary text-primary-foreground flex items-center justify-center text-xs font-bold">
                    2
                  </div>
                  <div className="flex-1">
                    <h4 className="font-medium text-sm mb-1">Start Chatting</h4>
                    <p className="text-xs text-muted-foreground">
                      Begin a conversation with the model and selected adapters
                    </p>
                    <Button
                      variant="link"
                      size="sm"
                      className="px-0 mt-1"
                      onClick={() => navigate(buildChatLink())}
                    >
                      Open Chat →
                    </Button>
                  </div>
                </div>

                <div className="flex items-start gap-3 p-3 bg-muted/50 rounded-lg">
                  <div className="flex-shrink-0 w-6 h-6 rounded-full bg-primary text-primary-foreground flex items-center justify-center text-xs font-bold">
                    3
                  </div>
                  <div className="flex-1">
                    <h4 className="font-medium text-sm mb-1">View Telemetry</h4>
                    <p className="text-xs text-muted-foreground">
                      Inspect routing and token timelines for recent sessions
                    </p>
                    <Button
                      variant="link"
                      size="sm"
                      className="px-0 mt-1"
                      onClick={() => navigate(buildTelemetryViewerLink())}
                    >
                      Open Telemetry Viewer →
                    </Button>
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>

          {/* Recent Chat Sessions */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <MessageSquare className="h-5 w-5" />
                Recent Conversations
              </CardTitle>
              <CardDescription>
                Your recent chat sessions
              </CardDescription>
            </CardHeader>
            <CardContent>
              {sessionsLoading ? (
                <div className="space-y-2">
                  {[1, 2, 3].map((i) => (
                    <Skeleton key={i} className="h-16 w-full" />
                  ))}
                </div>
              ) : recentSessions.length === 0 ? (
                <div className="text-center py-6">
                  <MessageSquare className="h-12 w-12 mx-auto text-muted-foreground/50 mb-2" />
                  <p className="text-sm text-muted-foreground mb-3">
                    No conversations yet
                  </p>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => navigate(buildChatLink())}
                  >
                    Start Your First Chat
                  </Button>
                </div>
              ) : (
                <div className="space-y-2">
                  {recentSessions.map((session) => (
                    <div
                      key={session.id}
                      className="flex items-center justify-between p-3 bg-muted/50 rounded-lg hover:bg-muted transition-colors cursor-pointer"
                      onClick={() => navigate(buildChatLink({ sessionId: session.id }))}
                    >
                      <div className="flex-1 min-w-0">
                        <h4 className="font-medium text-sm truncate">{session.name}</h4>
                        <div className="flex items-center gap-2 mt-1">
                          <Clock className="h-3 w-3 text-muted-foreground" />
                          <p className="text-xs text-muted-foreground">
                            {session.messages.length} messages
                          </p>
                          {session.stackName && (
                            <Badge variant="outline" className="text-xs">
                              {session.stackName}
                            </Badge>
                          )}
                        </div>
                      </div>
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={(e) => {
                          e.stopPropagation();
                          navigate(buildChatLink({ sessionId: session.id }));
                        }}
                      >
                        <Eye className="h-4 w-4" />
                      </Button>
                    </div>
                  ))}
                  {sessions.length > 5 && (
                    <Button
                      variant="link"
                      size="sm"
                      className="w-full mt-2"
                      onClick={() => navigate(buildChatLink())}
                    >
                      View All Sessions →
                    </Button>
                  )}
                </div>
              )}
            </CardContent>
          </Card>

          {/* Available Adapters */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Layers className="h-5 w-5" />
                Available Adapters
              </CardTitle>
              <CardDescription>
                Adapters ready for use in conversations
              </CardDescription>
            </CardHeader>
            <CardContent>
              {adaptersLoading ? (
                <div className="space-y-2">
                  {[1, 2, 3].map((i) => (
                    <Skeleton key={i} className="h-12 w-full" />
                  ))}
                </div>
              ) : adapters.length === 0 ? (
                <div className="text-center py-6">
                  <Layers className="h-12 w-12 mx-auto text-muted-foreground/50 mb-2" />
                  <p className="text-sm text-muted-foreground">
                    No adapters available
                  </p>
                </div>
              ) : (
                <div className="space-y-2">
                  {adapters.slice(0, 5).map((adapter) => (
                    <div
                      key={adapter.adapter_id}
                      className="flex items-center justify-between p-3 bg-muted/50 rounded-lg"
                    >
                      <div className="flex-1 min-w-0">
                        <h4 className="font-medium text-sm truncate">
                          {adapter.name || adapter.adapter_id}
                        </h4>
                        <div className="flex items-center gap-2 mt-1">
                          {adapter.tier && (
                            <Badge variant="outline" className="text-xs">
                              {adapter.tier}
                            </Badge>
                          )}
                          {adapter.lifecycle_state && (
                            <Badge variant="secondary" className="text-xs">
                              {adapter.lifecycle_state}
                            </Badge>
                          )}
                        </div>
                      </div>
                    </div>
                  ))}
                  {adapters.length > 5 && (
                    <Button
                      variant="link"
                      size="sm"
                      className="w-full mt-2"
                      onClick={() => navigate(buildAdaptersListLink())}
                    >
                      View All Adapters →
                    </Button>
                  )}
                </div>
              )}
            </CardContent>
          </Card>

          {/* Help & Resources */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <BookOpen className="h-5 w-5" />
                Help & Resources
              </CardTitle>
              <CardDescription>
                Learn more about using AdapterOS
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="space-y-2">
                <Button
                  variant="outline"
                  size="sm"
                  className="w-full justify-start"
                  onClick={() => window.open('https://docs.adapteros.local', '_blank')}
                >
                  <BookOpen className="h-4 w-4 mr-2" />
                  Documentation
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  className="w-full justify-start"
                  onClick={() => window.open('https://docs.adapteros.local/faq', '_blank')}
                >
                  <BookOpen className="h-4 w-4 mr-2" />
                  FAQ
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  className="w-full justify-start"
                  onClick={() => window.open('https://docs.adapteros.local/api', '_blank')}
                >
                  <Activity className="h-4 w-4 mr-2" />
                  API Reference
                </Button>
              </div>
            </CardContent>
          </Card>
        </ContentGrid>
      </div>
    </DashboardLayout>
  );
}
