import React, { useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { Tag, Plus, X } from 'lucide-react';
import { toast } from 'sonner';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { cn } from '@/components/ui/utils';
import {
  useChatTags,
  useSessionTags,
  useCreateTag,
  useAssignTagsToSession,
  useRemoveTagFromSession,
} from '@/hooks/useChatTags';
import type { ChatTag } from '@/api/chat-types';

interface ChatTagsManagerProps {
  sessionId: string;
  className?: string;
}

const DEFAULT_COLORS = [
  '#ef4444', // red
  '#f59e0b', // amber
  '#10b981', // emerald
  '#3b82f6', // blue
  '#8b5cf6', // violet
  '#ec4899', // pink
  '#6366f1', // indigo
  '#14b8a6', // teal
];

export function ChatTagsManager({ sessionId, className }: ChatTagsManagerProps) {
  const queryClient = useQueryClient();
  const [isPopoverOpen, setIsPopoverOpen] = useState(false);
  const [newTagName, setNewTagName] = useState('');
  const [newTagColor, setNewTagColor] = useState(DEFAULT_COLORS[0]);

  // Fetch all available tags and current session tags
  const { data: allTags = [], isLoading: isLoadingAllTags } = useChatTags();
  const { data: sessionTags = [], isLoading: isLoadingSessionTags } = useSessionTags(sessionId);

  // Mutations
  const createTag = useCreateTag({
    onSuccess: (newTag) => {
      // Invalidate tags query to refetch
      queryClient.invalidateQueries({ queryKey: ['chat', 'tags'] });

      // Automatically assign the newly created tag to the session
      assignTags.mutate({ sessionId, tagIds: [newTag.id] });

      // Reset form
      setNewTagName('');
      setNewTagColor(DEFAULT_COLORS[0]);
      setIsPopoverOpen(false);
    },
    onError: (error) => {
      toast.error('Failed to create tag', {
        description: error.message,
      });
    },
  });

  const assignTags = useAssignTagsToSession({
    onSuccess: () => {
      // Invalidate session tags to refetch
      queryClient.invalidateQueries({ queryKey: ['chat', 'sessions', sessionId, 'tags'] });
    },
    onError: (error) => {
      toast.error('Failed to assign tag', {
        description: error.message,
      });
    },
  });

  const removeTag = useRemoveTagFromSession({
    onSuccess: () => {
      // Invalidate session tags to refetch
      queryClient.invalidateQueries({ queryKey: ['chat', 'sessions', sessionId, 'tags'] });
    },
    onError: (error) => {
      toast.error('Failed to remove tag', {
        description: error.message,
      });
    },
  });

  // Get tags that are not yet assigned to this session
  const sessionTagIds = new Set(sessionTags.map((tag) => tag.id));
  const availableTags = allTags.filter((tag) => !sessionTagIds.has(tag.id));

  const handleCreateTag = (e: React.FormEvent) => {
    e.preventDefault();
    if (!newTagName.trim()) return;

    createTag.mutate({
      name: newTagName.trim(),
      color: newTagColor,
    });
  };

  const handleAssignTag = (tagId: string) => {
    assignTags.mutate({ sessionId, tagIds: [tagId] });
  };

  const handleRemoveTag = (tagId: string) => {
    removeTag.mutate({ sessionId, tagId });
  };

  const isLoading = isLoadingAllTags || isLoadingSessionTags;

  return (
    <div className={cn('flex flex-wrap items-center gap-2', className)}>
      {/* Currently assigned tags */}
      {sessionTags.map((tag) => (
        <Badge
          key={tag.id}
          variant="outline"
          className="group flex items-center gap-1 pr-1"
          style={{
            backgroundColor: tag.color ? `${tag.color}15` : undefined,
            borderColor: tag.color || undefined,
            color: tag.color || undefined,
          }}
        >
          <Tag className="w-3 h-3" />
          <span>{tag.name}</span>
          <Button
            variant="ghost"
            size="icon"
            className="h-4 w-4 p-0 ml-1 hover:bg-transparent"
            onClick={(e) => {
              e.stopPropagation();
              handleRemoveTag(tag.id);
            }}
            disabled={removeTag.isPending}
            aria-label={`Remove tag ${tag.name}`}
          >
            <X className="h-3 w-3" />
          </Button>
        </Badge>
      ))}

      {/* Add tag popover */}
      <Popover open={isPopoverOpen} onOpenChange={setIsPopoverOpen}>
        <PopoverTrigger asChild>
          <Button
            variant="outline"
            size="sm"
            className="h-6 px-2 text-xs"
            disabled={isLoading}
          >
            <Plus className="w-3 h-3 mr-1" />
            Add Tag
          </Button>
        </PopoverTrigger>
        <PopoverContent className="w-80" align="start">
          <div className="space-y-4">
            <div>
              <h4 className="font-medium text-sm mb-2">Create New Tag</h4>
              <form onSubmit={handleCreateTag} className="space-y-3">
                <div>
                  <Input
                    placeholder="Tag name"
                    value={newTagName}
                    onChange={(e) => setNewTagName(e.target.value)}
                    className="text-sm"
                  />
                </div>
                <div>
                  <label className="text-xs text-muted-foreground mb-2 block">
                    Color
                  </label>
                  <div className="flex gap-2 flex-wrap">
                    {DEFAULT_COLORS.map((color) => (
                      <button
                        key={color}
                        type="button"
                        onClick={() => setNewTagColor(color)}
                        className={cn(
                          'w-6 h-6 rounded-full border-2 transition-all',
                          newTagColor === color
                            ? 'border-foreground scale-110'
                            : 'border-transparent hover:scale-105'
                        )}
                        style={{ backgroundColor: color }}
                        aria-label={`Select color ${color}`}
                      />
                    ))}
                  </div>
                </div>
                <Button
                  type="submit"
                  size="sm"
                  className="w-full"
                  disabled={!newTagName.trim() || createTag.isPending}
                >
                  {createTag.isPending ? 'Creating...' : 'Create Tag'}
                </Button>
              </form>
            </div>

            {availableTags.length > 0 && (
              <div className="pt-3 border-t">
                <h4 className="font-medium text-sm mb-2">Existing Tags</h4>
                <div className="flex flex-wrap gap-2">
                  {availableTags.map((tag) => (
                    <button
                      key={tag.id}
                      type="button"
                      onClick={() => handleAssignTag(tag.id)}
                      disabled={assignTags.isPending}
                      className="transition-opacity hover:opacity-80"
                    >
                      <Badge
                        variant="outline"
                        className="cursor-pointer"
                        style={{
                          backgroundColor: tag.color ? `${tag.color}15` : undefined,
                          borderColor: tag.color || undefined,
                          color: tag.color || undefined,
                        }}
                      >
                        <Tag className="w-3 h-3 mr-1" />
                        {tag.name}
                      </Badge>
                    </button>
                  ))}
                </div>
              </div>
            )}
          </div>
        </PopoverContent>
      </Popover>
    </div>
  );
}
