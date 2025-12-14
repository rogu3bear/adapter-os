import React, { useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
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
import {
  MoreHorizontal,
  Pencil,
  Tag,
  Folder,
  Share2,
  Archive,
  Trash2,
} from 'lucide-react';
import { useArchiveSession } from '@/hooks/chat/useChatArchive';
import { useChatSessionsApi } from '@/hooks/chat/useChatSessionsApi';
import { toast } from 'sonner';
import { logger } from '@/utils/logger';

interface ChatSessionActionsProps {
  sessionId: string;
  tenantId: string;
  onRename: () => void;
  onManageTags: () => void;
  onSetCategory: () => void;
  onShare: () => void;
  isLoading?: boolean;
}

export function ChatSessionActions({
  sessionId,
  tenantId,
  onRename,
  onManageTags,
  onSetCategory,
  onShare,
  isLoading = false,
}: ChatSessionActionsProps) {
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);
  const [showArchiveDialog, setShowArchiveDialog] = useState(false);

  const { deleteSession } = useChatSessionsApi(tenantId);
  const { mutate: archiveSession, isPending: isArchiving } = useArchiveSession({
    onSuccess: () => {
      logger.info('Chat session archived', {
        component: 'ChatSessionActions',
        sessionId,
        tenantId,
      });
    },
    onError: (error) => {
      toast.error('Failed to archive session');
      logger.error('Failed to archive session', {
        component: 'ChatSessionActions',
        sessionId,
      }, error);
    },
  });

  const handleArchiveClick = () => {
    setShowArchiveDialog(true);
  };

  const handleArchiveConfirm = () => {
    archiveSession({ sessionId });
    setShowArchiveDialog(false);
  };

  const handleDeleteClick = () => {
    setShowDeleteDialog(true);
  };

  const handleDeleteConfirm = () => {
    deleteSession(sessionId);
    setShowDeleteDialog(false);
    toast.success('Session deleted');
  };

  const isActionDisabled = isLoading || isArchiving;

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="h-8 w-8 p-0"
            disabled={isActionDisabled}
            aria-label="Session actions"
          >
            <MoreHorizontal className="h-4 w-4" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-48">
          {/* Edit Actions */}
          <DropdownMenuItem
            onClick={onRename}
            disabled={isActionDisabled}
            title="Rename this session"
          >
            <Pencil className="mr-2 h-4 w-4" />
            Edit name
          </DropdownMenuItem>

          <DropdownMenuItem
            onClick={onManageTags}
            disabled={isActionDisabled}
            title="Add or remove tags"
          >
            <Tag className="mr-2 h-4 w-4" />
            Assign tags
          </DropdownMenuItem>

          <DropdownMenuItem
            onClick={onSetCategory}
            disabled={isActionDisabled}
            title="Set session category"
          >
            <Folder className="mr-2 h-4 w-4" />
            Set category
          </DropdownMenuItem>

          <DropdownMenuSeparator />

          {/* Share Action */}
          <DropdownMenuItem
            onClick={onShare}
            disabled={isActionDisabled}
            title="Share this session"
          >
            <Share2 className="mr-2 h-4 w-4" />
            Share session
          </DropdownMenuItem>

          <DropdownMenuSeparator />

          {/* Archive Action */}
          <DropdownMenuItem
            onClick={handleArchiveClick}
            disabled={isActionDisabled}
            title="Move to archive"
          >
            <Archive className="mr-2 h-4 w-4" />
            Archive session
          </DropdownMenuItem>

          {/* Delete Action */}
          <DropdownMenuItem
            onClick={handleDeleteClick}
            disabled={isActionDisabled}
            variant="destructive"
            title="Delete permanently"
          >
            <Trash2 className="mr-2 h-4 w-4" />
            Delete session
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      {/* Archive Confirmation Dialog */}
      <AlertDialog open={showArchiveDialog} onOpenChange={setShowArchiveDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Archive Session</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to archive this session? You can restore it later from the archive.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handleArchiveConfirm}>
              Archive
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Delete Confirmation Dialog */}
      <AlertDialog open={showDeleteDialog} onOpenChange={setShowDeleteDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete Session</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to delete this session? This action cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDeleteConfirm}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

export default ChatSessionActions;
