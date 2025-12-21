import React from 'react';
import { Button } from './button';
import { Star } from 'lucide-react';
import { useBookmarks } from '@/contexts/BookmarkContext';
import { cn } from '@/lib/utils';

interface BookmarkButtonProps {
  type: 'page' | 'adapter' | 'tenant' | 'policy' | 'node' | 'worker' | 'bundle' | 'event';
  title: string;
  url: string;
  entityId?: string;
  description?: string;
  variant?: 'default' | 'ghost' | 'outline';
  size?: 'default' | 'sm' | 'lg' | 'icon';
  className?: string;
}

export function BookmarkButton({
  type,
  title,
  url,
  entityId,
  description,
  variant = 'ghost',
  size = 'icon',
  className,
}: BookmarkButtonProps) {
  const { addBookmark, removeBookmark, isBookmarked, getBookmark } = useBookmarks();
  const bookmarked = isBookmarked(url, entityId);
  
  const handleClick = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    
    if (bookmarked) {
      const bookmark = getBookmark(url, entityId);
      if (bookmark) {
        removeBookmark(bookmark.id);
      }
    } else {
      addBookmark({
        type,
        title,
        url,
        entityId,
        description,
      });
    }
  };

  return (
    <Button
      variant={variant}
      size={size}
      onClick={handleClick}
      className={cn(className)}
      aria-label={bookmarked ? `Remove bookmark: ${title}` : `Bookmark: ${title}`}
      title={bookmarked ? `Remove bookmark: ${title}` : `Bookmark: ${title}`}
    >
      <Star
        className={cn(
          'h-4 w-4',
          bookmarked && 'fill-yellow-400 text-yellow-400'
        )}
      />
    </Button>
  );
}

