import React, { useState } from 'react';
import { Archive, Trash2, RotateCcw, AlertTriangle, Clock } from 'lucide-react';
import { toast } from 'sonner';

import {
  useArchivedSessions,
  useDeletedSessions,
  useRestoreSession,
  useHardDeleteSession,
} from '@/hooks/useChatArchive';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { EmptyState } from '@/components/ui/empty-state';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/components/ui/utils';
import type { ChatSessionWithStatus } from '@/api/chat-types';
import { formatTimestamp } from '@/utils/format';

interface ChatArchivePanelProps {
  className?: string;
}

interface SessionCardProps {
  session: ChatSessionWithStatus;
  onRestore: (sessionId: string) => void;
  onDelete?: (sessionId: string) => void;
  isRestoring?: boolean;
  isDeleting?: boolean;
}

function SessionCard({ session, onRestore, onDelete, isRestoring, isDeleting }: SessionCardProps) {
  const [showRestoreConfirm, setShowRestoreConfirm] = useState(false);
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);

  const formatDate = (dateString?: string) => {
    if (!dateString) return 'Unknown';
    return formatTimestamp(dateString, 'long');
  };

  const isArchived = session.status === 'archived';
  const isDeleted = session.status === 'deleted';

  return (
    <>
      <Card className="hover:bg-accent/50 transition-colors">
        <CardHeader className="pb-3">
          <div className="flex items-start justify-between gap-4">
            <div className="flex-1 min-w-0">
              <CardTitle className="text-base font-semibold truncate">
                {session.name}
              </CardTitle>
              <CardDescription className="mt-1">
                {session.description || 'No description'}
              </CardDescription>
            </div>
            <div className="flex items-center gap-2 shrink-0">
              <Button
                size="sm"
                variant="outline"
                onClick={() => setShowRestoreConfirm(true)}
                disabled={isRestoring || isDeleting}
              >
                <RotateCcw className="h-3.5 w-3.5" />
                Restore
              </Button>
              {isDeleted && onDelete && (
                <Button
                  size="sm"
                  variant="destructive"
                  onClick={() => setShowDeleteConfirm(true)}
                  disabled={isRestoring || isDeleting}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  Delete Forever
                </Button>
              )}
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
            <div className="flex items-center gap-1.5">
              <Clock className="h-3.5 w-3.5" />
              <span>Last activity: {formatDate(session.last_activity_at)}</span>
            </div>
            {isArchived && session.archived_at && (
              <div className="flex items-center gap-1.5">
                <Archive className="h-3.5 w-3.5" />
                <span>Archived: {formatDate(session.archived_at)}</span>
              </div>
            )}
            {isDeleted && session.deleted_at && (
              <div className="flex items-center gap-1.5">
                <Trash2 className="h-3.5 w-3.5" />
                <span>Deleted: {formatDate(session.deleted_at)}</span>
              </div>
            )}
            {session.archive_reason && (
              <Badge variant="outline" className="text-xs">
                Reason: {session.archive_reason}
              </Badge>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Restore Confirmation Dialog */}
      <AlertDialog open={showRestoreConfirm} onOpenChange={setShowRestoreConfirm}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Restore Session?</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to restore "{session.name}"? This will move the session back to
              your active sessions list.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                onRestore(session.id);
                setShowRestoreConfirm(false);
              }}
            >
              Restore
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Hard Delete Confirmation Dialog */}
      {isDeleted && onDelete && (
        <AlertDialog open={showDeleteConfirm} onOpenChange={setShowDeleteConfirm}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <div className="flex items-center gap-2 text-destructive">
                <AlertTriangle className="h-5 w-5" />
                <AlertDialogTitle>Permanently Delete Session?</AlertDialogTitle>
              </div>
              <AlertDialogDescription className="space-y-2">
                <p className="font-semibold text-foreground">
                  This action cannot be undone!
                </p>
                <p>
                  Permanently deleting "{session.name}" will remove all messages, metadata, and
                  traces associated with this session. This data cannot be recovered.
                </p>
                <p className="text-sm">
                  Are you absolutely sure you want to continue?
                </p>
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Cancel</AlertDialogCancel>
              <AlertDialogAction
                onClick={() => {
                  onDelete(session.id);
                  setShowDeleteConfirm(false);
                }}
                className="bg-destructive text-white hover:bg-destructive/90"
              >
                Delete Forever
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      )}
    </>
  );
}

export function ChatArchivePanel({ className }: ChatArchivePanelProps) {
  // Fetch archived and deleted sessions
  const {
    data: archivedSessions = [],
    isLoading: isLoadingArchived,
    error: archivedError,
  } = useArchivedSessions();

  const {
    data: deletedSessions = [],
    isLoading: isLoadingDeleted,
    error: deletedError,
  } = useDeletedSessions();

  // Mutations (cache invalidation now handled in the hooks)
  const restoreMutation = useRestoreSession({
    onError: (error) => {
      toast.error(`Failed to restore session: ${error.message}`);
    },
  });

  const hardDeleteMutation = useHardDeleteSession({
    onError: (error) => {
      toast.error(`Failed to delete session: ${error.message}`);
    },
  });

  const handleRestore = (sessionId: string) => {
    restoreMutation.mutate(sessionId);
  };

  const handleHardDelete = (sessionId: string) => {
    hardDeleteMutation.mutate(sessionId);
  };

  return (
    <div className={cn('w-full', className)}>
      <Tabs defaultValue="archived" className="w-full">
        <TabsList className="grid w-full grid-cols-2 max-w-md">
          <TabsTrigger value="archived" className="flex items-center gap-2">
            <Archive className="h-4 w-4" />
            Archived ({archivedSessions.length})
          </TabsTrigger>
          <TabsTrigger value="trash" className="flex items-center gap-2">
            <Trash2 className="h-4 w-4" />
            Trash ({deletedSessions.length})
          </TabsTrigger>
        </TabsList>

        {/* Archived Sessions Tab */}
        <TabsContent value="archived" className="mt-4 space-y-4">
          {isLoadingArchived ? (
            <div className="text-center py-8 text-muted-foreground">
              Loading archived sessions...
            </div>
          ) : archivedError ? (
            <Card className="border-destructive">
              <CardContent className="flex items-center gap-2 py-4 text-destructive">
                <AlertTriangle className="h-4 w-4" />
                <span>Failed to load archived sessions: {archivedError.message}</span>
              </CardContent>
            </Card>
          ) : archivedSessions.length === 0 ? (
            <EmptyState
              icon={Archive}
              title="No Archived Sessions"
              description="Sessions you archive will appear here. Archived sessions can be restored at any time."
            />
          ) : (
            <div className="space-y-3">
              {archivedSessions.map((session) => (
                <SessionCard
                  key={session.id}
                  session={session}
                  onRestore={handleRestore}
                  isRestoring={restoreMutation.isPending}
                />
              ))}
            </div>
          )}
        </TabsContent>

        {/* Trash Tab */}
        <TabsContent value="trash" className="mt-4 space-y-4">
          {isLoadingDeleted ? (
            <div className="text-center py-8 text-muted-foreground">
              Loading deleted sessions...
            </div>
          ) : deletedError ? (
            <Card className="border-destructive">
              <CardContent className="flex items-center gap-2 py-4 text-destructive">
                <AlertTriangle className="h-4 w-4" />
                <span>Failed to load deleted sessions: {deletedError.message}</span>
              </CardContent>
            </Card>
          ) : deletedSessions.length === 0 ? (
            <EmptyState
              icon={Trash2}
              title="Trash is Empty"
              description="Deleted sessions will appear here temporarily. You can restore them or permanently delete them."
            />
          ) : (
            <>
              <Card className="border-warning bg-warning/5">
                <CardContent className="flex items-start gap-3 py-3">
                  <AlertTriangle className="h-5 w-5 text-warning shrink-0 mt-0.5" />
                  <div className="text-sm">
                    <p className="font-semibold text-foreground mb-1">
                      Sessions in trash are temporarily deleted
                    </p>
                    <p className="text-muted-foreground">
                      You can restore sessions from trash or permanently delete them. Permanent
                      deletion cannot be undone.
                    </p>
                  </div>
                </CardContent>
              </Card>
              <div className="space-y-3">
                {deletedSessions.map((session) => (
                  <SessionCard
                    key={session.id}
                    session={session}
                    onRestore={handleRestore}
                    onDelete={handleHardDelete}
                    isRestoring={restoreMutation.isPending}
                    isDeleting={hardDeleteMutation.isPending}
                  />
                ))}
              </div>
            </>
          )}
        </TabsContent>
      </Tabs>
    </div>
  );
}
