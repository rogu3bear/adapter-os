import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { useBookmarks } from '@/contexts/BookmarkContext';
import { useNavigate } from 'react-router-dom';
import { Star, X, ExternalLink } from 'lucide-react';
import {
  Box,
  Building,
  Shield,
  Server,
  Zap,
  LayoutDashboard,
  FileText,
  Eye,
} from 'lucide-react';

const typeIcons: Record<string, React.ComponentType<{ className?: string }>> = {
  page: LayoutDashboard,
  adapter: Box,
  tenant: Building,
  policy: Shield,
  node: Server,
  worker: Zap,
  bundle: FileText,
  event: Eye,
};

export function FavoritesPanel() {
  const { bookmarks, removeBookmark } = useBookmarks();
  const navigate = useNavigate();

  if (bookmarks.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Star className="h-4 w-4" />
            Favorites
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground">
            No bookmarks yet. Use the star icon to bookmark frequently accessed items.
          </p>
        </CardContent>
      </Card>
    );
  }

  // Sort by creation date (newest first)
  const sortedBookmarks = [...bookmarks].sort((a, b) => 
    new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
  );

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Star className="h-4 w-4 fill-yellow-400 text-yellow-400" />
          Favorites ({bookmarks.length})
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="space-y-2">
          {sortedBookmarks.map((bookmark) => {
            const Icon = typeIcons[bookmark.type] || FileText;
            return (
              <div
                key={bookmark.id}
                className="flex items-center gap-2 p-2 rounded-md hover:bg-muted transition-colors group"
              >
                <Icon className="h-4 w-4 shrink-0 text-muted-foreground" />
                <button
                  onClick={() => navigate(bookmark.url)}
                  className="flex-1 text-left text-sm font-medium hover:text-primary transition-colors truncate"
                  title={bookmark.description || bookmark.title}
                >
                  <div className="truncate">{bookmark.title}</div>
                  {bookmark.description && (
                    <div className="text-xs text-muted-foreground truncate">
                      {bookmark.description}
                    </div>
                  )}
                </button>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity"
                  onClick={(e) => {
                    e.stopPropagation();
                    removeBookmark(bookmark.id);
                  }}
                  aria-label={`Remove bookmark: ${bookmark.title}`}
                >
                  <X className="h-3 w-3" />
                </Button>
              </div>
            );
          })}
        </div>
      </CardContent>
    </Card>
  );
}

