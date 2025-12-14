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
  Play,
  Square,
  Trash2,
  Pin,
  PinOff,
  ArrowUp,
  Download,
  Activity,
  Power,
  PowerOff,
} from 'lucide-react';
import type { Adapter } from '@/api/adapter-types';

interface AdapterActionsProps {
  adapter: Adapter;
  onLoad?: (adapterId: string) => void;
  onUnload?: (adapterId: string) => void;
  onDelete?: (adapterId: string) => void;
  onPin?: (adapterId: string, pinned: boolean) => void;
  onPromote?: (adapterId: string) => void;
  onEvict?: (adapterId: string) => void;
  onViewHealth?: (adapterId: string) => void;
  onDownloadManifest?: (adapterId: string) => void;
  isLoading?: boolean;
  canLoad?: boolean;
  canUnload?: boolean;
  canDelete?: boolean;
}

export function AdapterActions({
  adapter,
  onLoad,
  onUnload,
  onDelete,
  onPin,
  onPromote,
  onEvict,
  onViewHealth,
  onDownloadManifest,
  isLoading = false,
  canLoad = true,
  canUnload = true,
  canDelete = true,
}: AdapterActionsProps) {
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);
  const [showEvictDialog, setShowEvictDialog] = useState(false);

  const isLoaded = adapter.current_state ? ['warm', 'hot', 'resident'].includes(adapter.current_state) : false;
  const isResident = adapter.current_state === 'resident';

  const handleDeleteClick = () => {
    setShowDeleteDialog(true);
  };

  const handleDeleteConfirm = () => {
    onDelete?.(adapter.adapter_id);
    setShowDeleteDialog(false);
  };

  const handleEvictClick = () => {
    setShowEvictDialog(true);
  };

  const handleEvictConfirm = () => {
    onEvict?.(adapter.adapter_id);
    setShowEvictDialog(false);
  };

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="sm"
            className="h-8 w-8 p-0"
            disabled={isLoading}
            aria-label={`Actions for ${adapter.name}`}
          >
            <MoreHorizontal className="h-4 w-4" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-48">
          {/* Load/Unload Actions */}
          {!isLoaded ? (
            <DropdownMenuItem
              onClick={() => onLoad?.(adapter.adapter_id)}
              disabled={!canLoad || isLoading}
              title="Activate - load into memory"
            >
              <Power className="mr-2 h-4 w-4" />
              {adapter.version ? `Activate v${adapter.version}` : 'Activate'}
            </DropdownMenuItem>
          ) : (
            <DropdownMenuItem
              onClick={() => onUnload?.(adapter.adapter_id)}
              disabled={!canUnload || isLoading || isResident}
              title="Deactivate - remove from memory"
            >
              <PowerOff className="mr-2 h-4 w-4" />
              {adapter.version ? `Deactivate v${adapter.version}` : 'Deactivate'}
            </DropdownMenuItem>
          )}

          <DropdownMenuSeparator />

          {/* State Management */}
          <DropdownMenuItem
            onClick={() => onPromote?.(adapter.adapter_id)}
            disabled={isLoading || isResident}
            title="Moves adapter through lifecycle stages: cold->warm->hot->resident"
          >
            <ArrowUp className="mr-2 h-4 w-4" />
            Promote Lifecycle
          </DropdownMenuItem>

          <DropdownMenuItem
            onClick={() => onPin?.(adapter.adapter_id, !adapter.pinned)}
            disabled={isLoading}
            title={adapter.pinned ? "Allow this adapter to be removed during memory pressure" : "Prevents this adapter from being removed during memory pressure"}
          >
            {adapter.pinned ? (
              <>
                <PinOff className="mr-2 h-4 w-4" />
                Allow Removal
              </>
            ) : (
              <>
                <Pin className="mr-2 h-4 w-4" />
                Protect Adapter
              </>
            )}
          </DropdownMenuItem>

          <DropdownMenuSeparator />

          {/* View/Export Actions */}
          <DropdownMenuItem onClick={() => onViewHealth?.(adapter.adapter_id)}>
            <Activity className="mr-2 h-4 w-4" />
            View Health
          </DropdownMenuItem>

          <DropdownMenuItem onClick={() => onDownloadManifest?.(adapter.adapter_id)}>
            <Download className="mr-2 h-4 w-4" />
            Download Manifest
          </DropdownMenuItem>

          <DropdownMenuSeparator />

          {/* Destructive Actions */}
          <DropdownMenuItem
            onClick={handleEvictClick}
            disabled={!canUnload || isLoading || adapter.pinned || adapter.current_state === 'unloaded'}
            title="Forces memory release due to pressure. Adapter remains stored and can be reloaded."
          >
            <Square className="mr-2 h-4 w-4" />
            Remove
          </DropdownMenuItem>

          <DropdownMenuItem
            onClick={handleDeleteClick}
            disabled={!canDelete || isLoading}
            className="text-destructive focus:text-destructive"
          >
            <Trash2 className="mr-2 h-4 w-4" />
            Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      {/* Delete Confirmation Dialog */}
      <AlertDialog open={showDeleteDialog} onOpenChange={setShowDeleteDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete Adapter</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to delete <strong>{adapter.name}</strong>?
              This action cannot be undone and will permanently remove the adapter
              and its weights from the system.
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

      {/* Remove Confirmation Dialog */}
      <AlertDialog open={showEvictDialog} onOpenChange={setShowEvictDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove Adapter</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to remove <strong>{adapter.name}</strong> from memory?
              This will free up memory resources but the adapter can be loaded again later.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handleEvictConfirm}>
              Remove
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

// Inline action buttons for quick actions
interface InlineAdapterActionsProps {
  adapter: Adapter;
  onLoad?: (adapterId: string) => void;
  onUnload?: (adapterId: string) => void;
  isLoading?: boolean;
  canLoad?: boolean;
  canUnload?: boolean;
}

export function InlineAdapterActions({
  adapter,
  onLoad,
  onUnload,
  isLoading = false,
  canLoad = true,
  canUnload = true,
}: InlineAdapterActionsProps) {
  const isLoaded = adapter.current_state ? ['warm', 'hot', 'resident'].includes(adapter.current_state) : false;
  const isResident = adapter.current_state === 'resident';

  if (isLoaded) {
    return (
      <Button
        variant="outline"
        size="sm"
        onClick={() => onUnload?.(adapter.adapter_id)}
        disabled={!canUnload || isLoading || isResident}
        className="h-7"
      >
        <PowerOff className="mr-1 h-3 w-3" />
        {adapter.version ? `Deactivate v${adapter.version}` : 'Deactivate'}
      </Button>
    );
  }

  return (
    <Button
      variant="outline"
      size="sm"
      onClick={() => onLoad?.(adapter.adapter_id)}
      disabled={!canLoad || isLoading}
      className="h-7"
    >
      <Power className="mr-1 h-3 w-3" />
      {adapter.version ? `Activate v${adapter.version}` : 'Activate'}
    </Button>
  );
}

export default AdapterActions;
