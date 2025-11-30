import React, { useState } from 'react';
import { Share2, Users, X, Link, UserPlus, Check } from 'lucide-react';
import { useQueryClient } from '@tanstack/react-query';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
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
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Avatar } from '@/components/ui/avatar';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { Switch } from '@/components/ui/switch';
import { Label } from '@/components/ui/label';
import {
  useSessionShares,
  useShareSession,
  useRevokeShare,
} from '@/hooks/useChatSharing';
import type { SharePermission } from '@/api/chat-types';
import { toast } from 'sonner';
import { useTenant } from '@/layout/LayoutProvider';

interface Props {
  sessionId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function ChatShareDialog({ sessionId, open, onOpenChange }: Props) {
  const queryClient = useQueryClient();
  const { selectedTenant } = useTenant();
  const [userInput, setUserInput] = useState('');
  const [selectedPermission, setSelectedPermission] = useState<SharePermission>('view');
  const [shareWithWorkspace, setShareWithWorkspace] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [revokeConfirmOpen, setRevokeConfirmOpen] = useState(false);
  const [shareIdToRevoke, setShareIdToRevoke] = useState<string | null>(null);

  // Fetch current shares
  const { data: shares = [], isLoading: sharesLoading } = useSessionShares(sessionId, {
    enabled: open,
  });

  // Share mutation
  const shareMutation = useShareSession({
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['chat', 'sessions', sessionId, 'shares'] });
      toast.success('Session shared successfully');
      setUserInput('');
      setShareWithWorkspace(false);
    },
    onError: (error) => {
      toast.error(`Failed to share session: ${error.message}`);
    },
    onSettled: () => {
      setIsSubmitting(false);
    },
  });

  // Revoke mutation
  const revokeMutation = useRevokeShare({
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['chat', 'sessions', sessionId, 'shares'] });
      toast.success('Share revoked successfully');
    },
    onError: (error) => {
      toast.error(`Failed to revoke share: ${error.message}`);
    },
  });

  const handleShare = async () => {
    if (!shareWithWorkspace && !userInput.trim()) {
      toast.error('Please enter a user email or ID');
      return;
    }

    setIsSubmitting(true);

    const request = {
      permission: selectedPermission,
      ...(shareWithWorkspace
        ? { workspace_id: selectedTenant || 'default' }
        : { user_ids: [userInput.trim()] }),
    };

    shareMutation.mutate({ sessionId, request });
  };

  const handleRevoke = (shareId: string) => {
    setShareIdToRevoke(shareId);
    setRevokeConfirmOpen(true);
  };

  const confirmRevoke = () => {
    if (shareIdToRevoke) {
      revokeMutation.mutate({ sessionId, shareId: shareIdToRevoke });
    }
    setRevokeConfirmOpen(false);
    setShareIdToRevoke(null);
  };

  const getPermissionLabel = (permission: SharePermission) => {
    switch (permission) {
      case 'view':
        return 'Can view';
      case 'comment':
        return 'Can comment';
      case 'collaborate':
        return 'Can edit';
      default:
        return permission;
    }
  };

  const getPermissionColor = (permission: SharePermission) => {
    switch (permission) {
      case 'view':
        return 'bg-blue-100 text-blue-700';
      case 'comment':
        return 'bg-yellow-100 text-yellow-700';
      case 'collaborate':
        return 'bg-green-100 text-green-700';
      default:
        return 'bg-gray-100 text-gray-700';
    }
  };

  const activeShares = shares.filter((s) => !s.revoked_at);
  const workspaceShares = activeShares.filter((s) => s.workspace_id);
  const userShares = activeShares.filter((s) => s.shared_with_user_id);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[80vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Share2 className="h-5 w-5" />
            Share Session
          </DialogTitle>
        </DialogHeader>

        <div className="space-y-6">
          {/* Share with workspace toggle */}
          <div className="flex items-center justify-between p-4 bg-slate-50 rounded-lg">
            <div className="flex items-center gap-3">
              <Users className="h-5 w-5 text-slate-600" />
              <div>
                <Label htmlFor="workspace-share" className="font-medium">
                  Share with workspace
                </Label>
                <p className="text-sm text-muted-foreground">
                  Everyone in your workspace can access this session
                </p>
              </div>
            </div>
            <Switch
              id="workspace-share"
              checked={shareWithWorkspace}
              onCheckedChange={setShareWithWorkspace}
            />
          </div>

          <Separator />

          {/* Share with specific users */}
          {!shareWithWorkspace && (
            <div className="space-y-3">
              <Label className="text-sm font-medium">Share with specific users</Label>
              <div className="flex gap-2">
                <Input
                  placeholder="Enter user email or ID"
                  value={userInput}
                  onChange={(e) => setUserInput(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      handleShare();
                    }
                  }}
                />
                <Select
                  value={selectedPermission}
                  onValueChange={(value) => setSelectedPermission(value as SharePermission)}
                >
                  <SelectTrigger className="w-[140px]">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="view">Can view</SelectItem>
                    <SelectItem value="comment">Can comment</SelectItem>
                    <SelectItem value="collaborate">Can edit</SelectItem>
                  </SelectContent>
                </Select>
                <Button
                  onClick={handleShare}
                  disabled={isSubmitting || (!shareWithWorkspace && !userInput.trim())}
                  size="icon"
                >
                  {isSubmitting ? (
                    <div className="h-4 w-4 border-2 border-white border-t-transparent rounded-full animate-spin" />
                  ) : (
                    <UserPlus className="h-4 w-4" />
                  )}
                </Button>
              </div>
            </div>
          )}

          {/* Share button for workspace */}
          {shareWithWorkspace && (
            <div className="flex justify-end">
              <Button onClick={handleShare} disabled={isSubmitting}>
                {isSubmitting ? (
                  <>
                    <div className="h-4 w-4 border-2 border-white border-t-transparent rounded-full animate-spin mr-2" />
                    Sharing...
                  </>
                ) : (
                  <>
                    <Check className="h-4 w-4 mr-2" />
                    Share with Workspace
                  </>
                )}
              </Button>
            </div>
          )}

          <Separator />

          {/* Current shares */}
          <div className="space-y-3">
            <Label className="text-sm font-medium">Current shares</Label>

            {sharesLoading ? (
              <div className="text-center py-8 text-sm text-muted-foreground">
                Loading shares...
              </div>
            ) : activeShares.length === 0 ? (
              <div className="text-center py-8 text-sm text-muted-foreground">
                This session hasn't been shared yet
              </div>
            ) : (
              <div className="space-y-2">
                {/* Workspace shares */}
                {workspaceShares.map((share) => (
                  <div
                    key={share.id}
                    className="flex items-center justify-between p-3 border rounded-lg hover:bg-slate-50"
                  >
                    <div className="flex items-center gap-3">
                      <div className="h-10 w-10 rounded-full bg-purple-100 flex items-center justify-center">
                        <Users className="h-5 w-5 text-purple-600" />
                      </div>
                      <div>
                        <div className="font-medium">Workspace</div>
                        <div className="text-xs text-muted-foreground">
                          Shared {new Date(share.shared_at).toLocaleDateString()}
                        </div>
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      <Badge
                        variant="secondary"
                        className={getPermissionColor(share.permission)}
                      >
                        {getPermissionLabel(share.permission)}
                      </Badge>
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => handleRevoke(share.id)}
                        disabled={revokeMutation.isPending}
                      >
                        <X className="h-4 w-4" />
                      </Button>
                    </div>
                  </div>
                ))}

                {/* User shares */}
                {userShares.map((share) => (
                  <div
                    key={share.id}
                    className="flex items-center justify-between p-3 border rounded-lg hover:bg-slate-50"
                  >
                    <div className="flex items-center gap-3">
                      <Avatar className="h-10 w-10">
                        <div className="h-full w-full bg-gradient-to-br from-blue-400 to-purple-500 flex items-center justify-center text-white font-medium">
                          {share.shared_with_user_id?.charAt(0).toUpperCase() || 'U'}
                        </div>
                      </Avatar>
                      <div>
                        <div className="font-medium">{share.shared_with_user_id}</div>
                        <div className="text-xs text-muted-foreground">
                          Shared {new Date(share.shared_at).toLocaleDateString()}
                          {share.expires_at && (
                            <> • Expires {new Date(share.expires_at).toLocaleDateString()}</>
                          )}
                        </div>
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      <Badge
                        variant="secondary"
                        className={getPermissionColor(share.permission)}
                      >
                        {getPermissionLabel(share.permission)}
                      </Badge>
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => handleRevoke(share.id)}
                        disabled={revokeMutation.isPending}
                      >
                        <X className="h-4 w-4" />
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Copy link section */}
          <Separator />
          <div className="space-y-3">
            <Label className="text-sm font-medium">Share link</Label>
            <div className="flex gap-2">
              <Input
                readOnly
                value={`${window.location.origin}/chat/sessions/${sessionId}`}
                className="bg-slate-50"
              />
              <Button
                variant="outline"
                size="icon"
                onClick={() => {
                  navigator.clipboard.writeText(
                    `${window.location.origin}/chat/sessions/${sessionId}`
                  );
                  toast.success('Link copied to clipboard');
                }}
              >
                <Link className="h-4 w-4" />
              </Button>
            </div>
            <p className="text-xs text-muted-foreground">
              Anyone with access to this session can use this link
            </p>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Close
          </Button>
        </DialogFooter>
      </DialogContent>

      <AlertDialog open={revokeConfirmOpen} onOpenChange={setRevokeConfirmOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Revoke Share Access?</AlertDialogTitle>
            <AlertDialogDescription>
              This will remove the user's access to this session. They will no longer be able to view or interact with it.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={confirmRevoke}>Revoke Access</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Dialog>
  );
}
