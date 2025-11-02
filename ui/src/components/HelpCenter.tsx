import React, { useState, useMemo } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from './ui/dialog';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './ui/card';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './ui/tabs';
import { ScrollArea } from './ui/scroll-area';
import { Search, X, HelpCircle, BookOpen, ChevronRight } from 'lucide-react';
import { helpTextDatabase, getHelpTextByCategory, type HelpTextItem } from '@/data/help-text';
import type { HelpTextItem as HelpTextItemType } from '@/data/help-text';
import { cn } from './ui/utils';

interface HelpCenterProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  initialSearch?: string;
  initialCategory?: HelpTextItemType['category'];
}

export function HelpCenter({ open, onOpenChange, initialSearch = '', initialCategory }: HelpCenterProps) {
  const [searchQuery, setSearchQuery] = useState(initialSearch);
  const [selectedCategory, setSelectedCategory] = useState<HelpTextItemType['category'] | 'all'>(
    initialCategory || 'all'
  );

  const categories: Array<{ value: HelpTextItemType['category'] | 'all'; label: string }> = [
    { value: 'all', label: 'All' },
    { value: 'navigation', label: 'Navigation' },
    { value: 'operations', label: 'Operations' },
    { value: 'adapters', label: 'Adapters' },
    { value: 'policies', label: 'Policies' },
    { value: 'settings', label: 'Settings' },
    { value: 'technical', label: 'Technical' }
  ];

  const filteredItems = useMemo(() => {
    let items = helpTextDatabase;

    // Filter by category
    if (selectedCategory !== 'all') {
      items = items.filter(item => item.category === selectedCategory);
    }

    // Filter by search query
    if (searchQuery.trim()) {
      const query = searchQuery.toLowerCase().trim();
      items = items.filter(item => {
        return (
          item.title.toLowerCase().includes(query) ||
          item.content.toLowerCase().includes(query) ||
          item.id.toLowerCase().includes(query)
        );
      });
    }

    return items;
  }, [searchQuery, selectedCategory]);

  const handleClearSearch = () => {
    setSearchQuery('');
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[90vh] flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <HelpCircle className="h-5 w-5" />
            Help Center
          </DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-4 flex-1 min-h-0">
          {/* Search Bar */}
          <div className="relative">
            <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Search help topics..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-9 pr-9"
            />
            {searchQuery && (
              <Button
                variant="ghost"
                size="sm"
                className="absolute right-1 top-1/2 transform -translate-y-1/2 h-6 w-6 p-0"
                onClick={handleClearSearch}
              >
                <X className="h-4 w-4" />
              </Button>
            )}
          </div>

          {/* Category Filter */}
          <div className="flex flex-wrap gap-2">
            {categories.map((cat) => (
              <Button
                key={cat.value}
                variant={selectedCategory === cat.value ? 'default' : 'outline'}
                size="sm"
                onClick={() => setSelectedCategory(cat.value)}
                className="text-xs"
              >
                {cat.label}
              </Button>
            ))}
          </div>

          {/* Results */}
          <ScrollArea className="flex-1">
            {filteredItems.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-12 text-center">
                <BookOpen className="h-12 w-12 text-muted-foreground mb-4" />
                <p className="text-lg font-medium mb-2">No results found</p>
                <p className="text-sm text-muted-foreground">
                  {searchQuery
                    ? `Try a different search term or clear your filters.`
                    : 'No help topics available in this category.'}
                </p>
              </div>
            ) : (
              <div className="space-y-3 pr-4">
                {filteredItems.map((item) => (
                  <Card key={item.id} className="hover:bg-accent/50 transition-colors">
                    <CardHeader className="pb-3">
                      <div className="flex items-start justify-between">
                        <div className="flex-1">
                          <CardTitle className="text-base flex items-center gap-2">
                            {item.title}
                            <Badge variant="secondary" className="text-xs">
                              {item.category}
                            </Badge>
                          </CardTitle>
                        </div>
                      </div>
                    </CardHeader>
                    <CardContent>
                      <CardDescription className="text-sm leading-relaxed">
                        {item.content}
                      </CardDescription>
                      <div className="mt-2 text-xs text-muted-foreground">
                        ID: <code className="px-1 py-0.5 bg-muted rounded">{item.id}</code>
                      </div>
                    </CardContent>
                  </Card>
                ))}
              </div>
            )}
          </ScrollArea>

          {/* Footer */}
          <div className="flex items-center justify-between pt-4 border-t text-xs text-muted-foreground">
            <div>
              {filteredItems.length > 0 && (
                <span>
                  Showing {filteredItems.length} {filteredItems.length === 1 ? 'result' : 'results'}
                  {searchQuery && ` for "${searchQuery}"`}
                </span>
              )}
            </div>
            <div className="flex items-center gap-2">
              <kbd className="px-2 py-1 bg-muted rounded text-xs">?</kbd>
              <span>for quick help</span>
            </div>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

