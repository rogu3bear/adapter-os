import React, { useState, useMemo } from 'react';
import { Folder, FolderPlus, ChevronRight } from 'lucide-react';
import { useChatCategories, useCreateCategory, useSetSessionCategory } from '@/hooks/useChatCategories';
import type { ChatCategory } from '@/api/chat-types';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { cn } from '@/components/ui/utils';
import { toast } from 'sonner';

interface ChatCategoriesManagerProps {
  sessionId?: string;
  currentCategoryId?: string | null;
  onCategoryChange?: (categoryId: string | null) => void;
  className?: string;
}

interface CategoryFormData {
  name: string;
  parent_id?: string;
  icon?: string;
  color?: string;
}

/**
 * ChatCategoriesManager Component
 *
 * Manages chat session categories with hierarchical tree structure.
 * Allows selecting and creating categories with customizable icons and colors.
 *
 * Features:
 * - Hierarchical category selection with visual depth indicators
 * - Create new categories with parent selection
 * - Assign categories to chat sessions
 * - Display current category for a session
 *
 * @component
 */
export function ChatCategoriesManager({
  sessionId,
  currentCategoryId,
  onCategoryChange,
  className,
}: ChatCategoriesManagerProps) {
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [formData, setFormData] = useState<CategoryFormData>({
    name: '',
    parent_id: undefined,
    icon: 'Folder',
    color: '#6366f1',
  });

  // Fetch categories (already sorted by path from backend)
  const { data: categories = [], isLoading, error, refetch } = useChatCategories();

  // Mutations
  const createCategoryMutation = useCreateCategory({
    onSuccess: (newCategory) => {
      toast.success('Category created', {
        description: `"${newCategory.name}" has been created successfully.`,
      });
      setIsCreateDialogOpen(false);
      setFormData({ name: '', parent_id: undefined, icon: 'Folder', color: '#6366f1' });
      refetch();
    },
    onError: (error) => {
      toast.error('Failed to create category', {
        description: error.message,
      });
    },
  });

  const setCategoryMutation = useSetSessionCategory({
    onSuccess: () => {
      toast.success('Category updated', {
        description: 'Session category has been updated.',
      });
      onCategoryChange?.(formData.parent_id || null);
    },
    onError: (error) => {
      toast.error('Failed to update category', {
        description: error.message,
      });
    },
  });

  // Find current category details
  const currentCategory = useMemo(
    () => categories.find((cat) => cat.id === currentCategoryId),
    [categories, currentCategoryId]
  );

  // Build category tree for display (categories are already sorted by path)
  const buildCategoryLabel = (category: ChatCategory): string => {
    const indent = '  '.repeat(category.depth);
    return `${indent}${category.name}`;
  };

  const handleCategorySelect = (categoryId: string) => {
    if (!sessionId) {
      toast.error('No session selected', {
        description: 'Please select a session first.',
      });
      return;
    }

    // Handle __none__ sentinel value for clearing category
    const effectiveCategoryId = categoryId === '__none__' ? null : categoryId;
    // Allow deselection by clicking the same category
    const newCategoryId = effectiveCategoryId === currentCategoryId ? null : effectiveCategoryId;
    setCategoryMutation.mutate({ sessionId, categoryId: newCategoryId });
  };

  const handleCreateCategory = () => {
    if (!formData.name.trim()) {
      toast.error('Name required', {
        description: 'Please enter a category name.',
      });
      return;
    }

    createCategoryMutation.mutate({
      name: formData.name.trim(),
      parent_id: formData.parent_id || undefined,
      icon: formData.icon,
      color: formData.color,
    });
  };

  const handleFormChange = (field: keyof CategoryFormData, value: string | undefined) => {
    // Convert __none__ sentinel to undefined for parent_id
    const effectiveValue = value === '__none__' ? undefined : value;
    setFormData((prev) => ({ ...prev, [field]: effectiveValue }));
  };

  if (error) {
    return (
      <div className={cn('text-sm text-destructive', className)}>
        Failed to load categories: {error.message}
      </div>
    );
  }

  return (
    <div className={cn('flex flex-col gap-3', className)}>
      {/* Category Selection */}
      <div className="space-y-2">
        <Label className="text-sm font-medium">
          <Folder className="inline-block mr-1 size-4" />
          Category
        </Label>
        <Select
          value={currentCategoryId || undefined}
          onValueChange={handleCategorySelect}
          disabled={isLoading || !sessionId}
        >
          <SelectTrigger className="w-full">
            <SelectValue placeholder={isLoading ? 'Loading...' : 'Select a category'}>
              {currentCategory && (
                <span className="flex items-center gap-2">
                  {currentCategory.icon && (
                    <span style={{ color: currentCategory.color }}>
                      {currentCategory.icon}
                    </span>
                  )}
                  {currentCategory.name}
                </span>
              )}
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            {categories.length === 0 ? (
              <div className="px-2 py-4 text-center text-sm text-muted-foreground">
                No categories yet. Create one below.
              </div>
            ) : (
              <>
                {/* Option to clear category */}
                {currentCategoryId && (
                  <SelectItem value="__none__">
                    <span className="text-muted-foreground italic">No category</span>
                  </SelectItem>
                )}
                {categories.map((category) => (
                  <SelectItem key={category.id} value={category.id}>
                    <span className="flex items-center gap-2">
                      {category.depth > 0 && (
                        <span className="text-muted-foreground">
                          {'  '.repeat(category.depth)}
                          <ChevronRight className="inline size-3" />
                        </span>
                      )}
                      {category.icon && (
                        <span style={{ color: category.color }}>{category.icon}</span>
                      )}
                      {category.name}
                    </span>
                  </SelectItem>
                ))}
              </>
            )}
          </SelectContent>
        </Select>
      </div>

      {/* Current Category Display */}
      {currentCategory && (
        <div className="flex items-center gap-2 px-3 py-2 bg-accent/50 rounded-md text-sm">
          <Folder className="size-4 text-muted-foreground" />
          <span className="text-muted-foreground">Current:</span>
          <span className="font-medium">{currentCategory.path}</span>
        </div>
      )}

      {/* Create Category Dialog */}
      <Dialog open={isCreateDialogOpen} onOpenChange={setIsCreateDialogOpen}>
        <DialogTrigger asChild>
          <Button variant="outline" className="w-full" size="sm">
            <FolderPlus className="size-4" />
            Create Category
          </Button>
        </DialogTrigger>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Create New Category</DialogTitle>
            <DialogDescription>
              Add a new category to organize your chat sessions. Categories can be nested
              for better organization.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-4">
            {/* Name Input */}
            <div className="space-y-2">
              <Label htmlFor="category-name">Name</Label>
              <Input
                id="category-name"
                placeholder="e.g., Work, Personal, Research"
                value={formData.name}
                onChange={(e) => handleFormChange('name', e.target.value)}
                autoFocus
              />
            </div>

            {/* Parent Category Select */}
            <div className="space-y-2">
              <Label htmlFor="parent-category">Parent Category (Optional)</Label>
              <Select
                value={formData.parent_id}
                onValueChange={(value) => handleFormChange('parent_id', value)}
              >
                <SelectTrigger id="parent-category">
                  <SelectValue placeholder="None (top-level)" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="__none__">
                    <span className="text-muted-foreground italic">None (top-level)</span>
                  </SelectItem>
                  {categories.map((category) => (
                    <SelectItem key={category.id} value={category.id}>
                      {buildCategoryLabel(category)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            {/* Icon Input */}
            <div className="space-y-2">
              <Label htmlFor="category-icon">Icon (Optional)</Label>
              <Input
                id="category-icon"
                placeholder="e.g., 📁, 🏢, 🔬"
                value={formData.icon}
                onChange={(e) => handleFormChange('icon', e.target.value)}
                maxLength={10}
              />
              <p className="text-xs text-muted-foreground">
                Enter an emoji or text icon
              </p>
            </div>

            {/* Color Input */}
            <div className="space-y-2">
              <Label htmlFor="category-color">Color (Optional)</Label>
              <div className="flex gap-2">
                <Input
                  id="category-color"
                  type="color"
                  value={formData.color}
                  onChange={(e) => handleFormChange('color', e.target.value)}
                  className="w-20 h-9 cursor-pointer"
                />
                <Input
                  type="text"
                  value={formData.color}
                  onChange={(e) => handleFormChange('color', e.target.value)}
                  placeholder="#6366f1"
                  className="flex-1"
                />
              </div>
            </div>
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setIsCreateDialogOpen(false)}
              disabled={createCategoryMutation.isPending}
            >
              Cancel
            </Button>
            <Button
              onClick={handleCreateCategory}
              disabled={createCategoryMutation.isPending || !formData.name.trim()}
            >
              {createCategoryMutation.isPending ? 'Creating...' : 'Create Category'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
