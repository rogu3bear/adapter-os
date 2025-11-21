//! Messages page for workspace communication
//!
//! Displays workspace-scoped messaging with real-time updates.
//! Shows message threads and allows sending new messages.
//!
//! Citation: Page structure from ui/src/pages/DashboardPage.tsx
//! - FeatureLayout wrapper per ui/src/layout/FeatureLayout.tsx L18-L121

import React, { useState } from 'react';
import { useAuth, useTenant } from '@/layout/LayoutProvider';
import FeatureLayout from '@/layout/FeatureLayout';
import { DensityProvider } from '@/contexts/DensityContext';
import { useMessages } from '@/hooks/useMessages';
import { useWorkspaces } from '@/hooks/useWorkspaces';
import { useRBAC } from '@/hooks/useRBAC';
import { MessageThread } from '@/components/MessageThread';
import { MessageComposer } from '@/components/MessageComposer';
import { WorkspaceSelector } from '@/components/WorkspaceSelector';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { ErrorRecovery, errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { HelpTooltip } from '@/components/ui/help-tooltip';
import { MessageSquare, Users, AlertCircle, RefreshCw } from 'lucide-react';
import { logger } from '@/utils/logger';

export default function MessagesPage() {
  const { user } = useAuth();
  const { selectedTenant } = useTenant();
  const { can, userRole } = useRBAC();
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<string>('');

  const { userWorkspaces, loading: workspacesLoading, error: workspacesError, refresh: refreshWorkspaces } = useWorkspaces({
    enabled: true,
    includeMembers: true,
  });

  const {
    messages,
    loading: messagesLoading,
    error: messagesError,
    sendMessage,
    editMessage,
    getThread,
    refresh: refreshMessages
  } = useMessages({
    workspaceId: selectedWorkspaceId,
    enabled: !!selectedWorkspaceId,
  });

  const selectedWorkspace = userWorkspaces.find(w => w.id === selectedWorkspaceId);

  const handleSendMessage = async (content: string, threadId?: string) => {
    if (!selectedWorkspaceId) {
      throw new Error('No workspace selected');
    }

    try {
      await sendMessage(content, threadId);
      logger.info('Message sent from UI', {
        component: 'MessagesPage',
        operation: 'send_message',
        workspaceId: selectedWorkspaceId,
        threadId,
      });
    } catch (err) {
      logger.error('Failed to send message from UI', {
        component: 'MessagesPage',
        operation: 'send_message',
        workspaceId: selectedWorkspaceId,
        threadId,
      }, err instanceof Error ? err : new Error(String(err)));
      throw err;
    }
  };

  const handleEditMessage = async (messageId: string, content: string) => {
    try {
      await editMessage(messageId, content);
      logger.info('Message edited from UI', {
        component: 'MessagesPage',
        operation: 'edit_message',
        messageId,
        workspaceId: selectedWorkspaceId,
      });
    } catch (err) {
      logger.error('Failed to edit message from UI', {
        component: 'MessagesPage',
        operation: 'edit_message',
        messageId,
        workspaceId: selectedWorkspaceId,
      }, err instanceof Error ? err : new Error(String(err)));
      throw err;
    }
  };

  const handleRefresh = async () => {
    await Promise.all([refreshWorkspaces(), selectedWorkspaceId ? refreshMessages() : Promise.resolve()]);
  };

  // Auto-select first workspace if none selected
  React.useEffect(() => {
    if (!selectedWorkspaceId && userWorkspaces.length > 0) {
      setSelectedWorkspaceId(userWorkspaces[0].id);
    }
  }, [selectedWorkspaceId, userWorkspaces]);

  return (
    <DensityProvider pageKey="messages">
      <FeatureLayout
        title="Messages"
        description="Workspace communication and collaboration"
        headerActions={
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={handleRefresh}
            disabled={workspacesLoading || messagesLoading}
            aria-label="Refresh messages"
          >
            <RefreshCw className={`h-4 w-4 ${workspacesLoading || messagesLoading ? 'animate-spin' : ''}`} />
            Refresh
          </Button>
        </div>
      }
    >
      <div className="space-y-6">
        {/* Workspace Selector */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Users className="h-5 w-5" />
              Select Workspace
            </CardTitle>
          </CardHeader>
          <CardContent>
            <WorkspaceSelector
              workspaces={userWorkspaces}
              selectedWorkspaceId={selectedWorkspaceId}
              onWorkspaceSelect={setSelectedWorkspaceId}
              loading={workspacesLoading}
            />
            {workspacesError && (
              <Alert className="mt-4">
                <AlertCircle className="h-4 w-4" />
                <AlertDescription>
                  Failed to load workspaces: {workspacesError}
                </AlertDescription>
              </Alert>
            )}
          </CardContent>
        </Card>

        {/* Messages Area */}
        {selectedWorkspace ? (
          <Tabs defaultValue="messages" className="space-y-4">
            <TabsList>
              <TabsTrigger value="messages" className="flex items-center gap-2">
                <MessageSquare className="h-4 w-4" />
                Messages ({messages.length})
              </TabsTrigger>
            </TabsList>

            <TabsContent value="messages" className="space-y-4">
              <Card>
                <CardHeader>
                  <CardTitle className="flex items-center gap-2">
                    <MessageSquare className="h-5 w-5" />
                    {selectedWorkspace.name}
                  </CardTitle>
                  {selectedWorkspace.description && (
                    <p className="text-sm text-muted-foreground">
                      {selectedWorkspace.description}
                    </p>
                  )}
                </CardHeader>
                <CardContent className="space-y-4">
                  {/* Message Thread */}
                  <MessageThread
                    messages={messages}
                    workspaceId={selectedWorkspaceId}
                    workspace={selectedWorkspace}
                    loading={messagesLoading}
                    error={messagesError}
                    onEditMessage={handleEditMessage}
                    onGetThread={getThread}
                    onRefresh={refreshMessages}
                  />

                  {/* Message Composer */}
                  <MessageComposer
                    workspaceId={selectedWorkspaceId}
                    onSendMessage={handleSendMessage}
                    disabled={!selectedWorkspaceId}
                  />
                </CardContent>
              </Card>
            </TabsContent>
          </Tabs>
        ) : (
          <Card>
            <CardContent className="flex items-center justify-center py-12">
              <div className="text-center">
                <MessageSquare className="h-12 w-12 text-muted-foreground mx-auto mb-4" />
                <h3 className="text-lg font-semibold mb-2">No Workspace Selected</h3>
                <p className="text-muted-foreground">
                  Select a workspace above to view and send messages.
                </p>
              </div>
            </CardContent>
          </Card>
        )}
      </div>
      </FeatureLayout>
    </DensityProvider>
  );
}
