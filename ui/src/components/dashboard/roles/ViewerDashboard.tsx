import React from 'react';
import { useNavigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { Badge } from '@/components/ui/badge';
import { PageHeader } from '@/components/ui/page-header';
import { MessageSquare, FileText, Sparkles, Clock, ArrowRight } from 'lucide-react';
import { apiClient } from '@/api/services';
import { useAuth } from '@/providers/CoreProviders';
import { useChatSessionsApi } from '@/hooks/chat/useChatSessionsApi';
import { buildChatLink, buildDocumentsLink } from '@/utils/navLinks';
import { getRoleLanguage } from '@/config/roleConfigs';

const RECENT_LIMIT = 4;

export default function ViewerDashboard() {
  const navigate = useNavigate();
  const { user } = useAuth();
  const roleCopy = getRoleLanguage(user?.role);
  const friendlyName = user?.display_name || user?.name || user?.email || 'there';
  const tenantKey = user?.tenant_id || 'default';

  const { data: documents = [], isLoading: documentsLoading } = useQuery({
    queryKey: ['viewer-documents', tenantKey],
    queryFn: () => apiClient.listDocuments(),
    staleTime: 60000,
    select: (docs) => {
      const getTimestamp = (doc: (typeof docs)[number]) =>
        new Date(doc.updated_at || doc.created_at || 0).getTime();
      return docs.slice().sort((a, b) => getTimestamp(b) - getTimestamp(a));
    },
  });

  const { sessions, isLoading: sessionsLoading } = useChatSessionsApi(tenantKey);

  const recentDocuments = documents.slice(0, RECENT_LIMIT);
  const recentSessions = sessions.slice(0, RECENT_LIMIT);

  return (
    <div className="space-y-6">
      <PageHeader
        title={`${roleCopy.welcomeTitle}, ${friendlyName}`}
        description="This is your standard view—focused on conversations and documents without the kernel noise."
        badges={[{ label: roleCopy.roleLabel, variant: 'secondary' }]}
        className="top-0"
      >
        <Button size="sm" onClick={() => navigate(buildChatLink())} data-testid="viewer-start-chat">
          <MessageSquare className="h-4 w-4 mr-2" />
          Start New Chat
        </Button>
      </PageHeader>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4 lg:gap-6">
        <Card className="lg:col-span-1">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Sparkles className="h-5 w-5" />
              Start a New Chat
            </CardTitle>
            <CardDescription>Ask a question, summarize a document, or continue a saved conversation.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            <Button className="w-full justify-between" onClick={() => navigate(buildChatLink())}>
              Talk to the AI
              <ArrowRight className="h-4 w-4" />
            </Button>
            <Button variant="outline" className="w-full justify-between" onClick={() => navigate(buildDocumentsLink())}>
              Reference a document
              <FileText className="h-4 w-4" />
            </Button>
          </CardContent>
        </Card>

        <Card className="lg:col-span-2">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <FileText className="h-5 w-5" />
              Recent Documents
            </CardTitle>
            <CardDescription>Files your team recently opened or updated.</CardDescription>
          </CardHeader>
          <CardContent>
            {documentsLoading ? (
              <div className="space-y-2">
                {[1, 2, 3].map((i) => (
                  <Skeleton key={i} className="h-12 w-full" />
                ))}
              </div>
            ) : recentDocuments.length === 0 ? (
              <div className="text-center py-6">
                <FileText className="h-12 w-12 mx-auto text-muted-foreground/60 mb-3" />
                <p className="text-sm text-muted-foreground mb-2">No recent documents yet.</p>
                <Button size="sm" variant="outline" onClick={() => navigate(buildDocumentsLink())}>
                  Add a document
                </Button>
              </div>
            ) : (
              <div className="space-y-2">
                {recentDocuments.map((doc) => (
                  <div
                    key={doc.document_id}
                    className="flex items-center justify-between rounded-lg border p-3 hover:bg-muted/60 transition-colors"
                  >
                    <div className="flex-1 min-w-0">
                      <p className="font-medium text-sm truncate">{doc.name || doc.document_id}</p>
                      <p className="text-xs text-muted-foreground mt-1">
                        Updated {new Date(doc.updated_at || doc.created_at || '').toLocaleDateString()}
                      </p>
                    </div>
                    <Badge variant="secondary" className="text-xs ml-3">
                      Ready
                    </Badge>
                  </div>
                ))}
                <Button variant="link" size="sm" className="w-full" onClick={() => navigate(buildDocumentsLink())}>
                  View all documents
                </Button>
              </div>
            )}
          </CardContent>
        </Card>

        <Card className="lg:col-span-3">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <MessageSquare className="h-5 w-5" />
              Recent Conversations
            </CardTitle>
            <CardDescription>Pick up where you left off—no system metrics required.</CardDescription>
          </CardHeader>
          <CardContent>
            {sessionsLoading ? (
              <div className="space-y-2">
                {[1, 2].map((i) => (
                  <Skeleton key={i} className="h-14 w-full" />
                ))}
              </div>
            ) : recentSessions.length === 0 ? (
              <div className="text-center py-6">
                <MessageSquare className="h-12 w-12 mx-auto text-muted-foreground/60 mb-3" />
                <p className="text-sm text-muted-foreground mb-2">No conversations yet.</p>
                <Button size="sm" variant="outline" onClick={() => navigate(buildChatLink())}>
                  Start your first chat
                </Button>
              </div>
            ) : (
              <div className="space-y-2">
                {recentSessions.map((session) => (
                  <div
                    key={session.id}
                    className="flex items-center justify-between rounded-lg border p-3 hover:bg-muted/60 transition-colors"
                  >
                    <div className="flex-1 min-w-0">
                      <p className="font-medium text-sm truncate">{session.name}</p>
                      <div className="flex items-center gap-2 text-xs text-muted-foreground mt-1">
                        <Clock className="h-3 w-3" />
                        <span>{session.messages.length} messages</span>
                        {session.stackName && <Badge variant="outline">{session.stackName}</Badge>}
                      </div>
                    </div>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => navigate(buildChatLink({ sessionId: session.id }))}
                    >
                      Continue
                    </Button>
                  </div>
                ))}
                <Button variant="link" size="sm" className="w-full" onClick={() => navigate(buildChatLink())}>
                  View all chats
                </Button>
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
