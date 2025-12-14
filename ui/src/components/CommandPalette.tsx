import React, { useEffect } from 'react';
import {
  CommandDialog,
  CommandInput,
  CommandList,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandShortcut,
  CommandSeparator,
} from './ui/command';
import { useCommandPalette, type CommandItem as CmdItem } from '@/contexts/CommandPaletteContext';
import { useBookmarks } from '@/contexts/BookmarkContext';
import {
  Box,
  Building,
  Shield,
  Server,
  Zap,
  LayoutDashboard,
  Clock,
  Star,
  FileText,
  Route,
  Activity,
  Play,
  Eye,
  Settings,
  FlaskConical,
  GitCompare,
  RotateCcw,
  AlertCircle,
  Sparkles,
} from 'lucide-react';

const typeIcons: Record<string, React.ComponentType<{ className?: string }>> = {
  page: LayoutDashboard,
  adapter: Box,
  tenant: Building,
  policy: Shield,
  node: Server,
  worker: Zap,
  action: Sparkles,
};

const routeIcons: Record<string, React.ComponentType<{ className?: string }>> = {
  '/dashboard': LayoutDashboard,
  '/training': Zap,
  '/testing': FlaskConical,
  '/golden': GitCompare,
  '/adapters': Box,
  '/metrics': Activity,
  '/routing': Route,
  '/inference': Play,
  '/telemetry': Eye,
  '/replay': RotateCcw,
  '/security/policies': Shield,
  '/security/audit': FileText,
  '/admin': Settings,
  '/admin/tenants': Building,
};

function getIconForItem(item: CmdItem): React.ComponentType<{ className?: string }> {
  if (item.icon) {
    return item.icon;
  }
  if (item.type === 'page') {
    const url = item.url ?? '';
    return routeIcons[url] || LayoutDashboard;
  }
  return typeIcons[item.type] || FileText;
}

function groupResults(results: CmdItem[]): Map<string, CmdItem[]> {
  const grouped = new Map<string, CmdItem[]>();
  
  for (const item of results) {
    const groupKey = item.group || item.type;
    const existing = grouped.get(groupKey);
    if (existing) {
      existing.push(item);
    } else {
      grouped.set(groupKey, [item]);
    }
  }
  
  return grouped;
}

const groupOrder = ['Quick Actions', 'Pages', 'Favorites', 'Recent', 'Adapters', 'Organizations', 'Policies', 'Nodes', 'Workers'];

export function CommandPalette() {
  const {
    isOpen,
    closePalette,
    searchQuery,
    setSearchQuery,
    searchResults,
    recentCommands,
    executeCommand,
    loading,
    refreshError,
    lastUpdated,
    refreshEntities,
    routes,
  } = useCommandPalette();
  
  const { bookmarks } = useBookmarks();

  const quickActionItems = routes.filter(item => item.type === 'action');
  
  // Convert bookmarks to command items
  const bookmarkItems: CmdItem[] = bookmarks.map(bookmark => ({
    id: bookmark.id,
    type: bookmark.type as CmdItem['type'],
    title: bookmark.title,
    description: bookmark.description,
    url: bookmark.url,
    entityId: bookmark.entityId,
    group: 'Favorites',
    icon: typeIcons[bookmark.type] || FileText,
  }));

  // Focus input when opened
  useEffect(() => {
    if (isOpen) {
      // Small delay to ensure dialog is rendered
      setTimeout(() => {
        const input = document.querySelector('[data-slot="command-input"]') as HTMLInputElement;
        input?.focus();
      }, 100);
    }
  }, [isOpen]);

  // Handle keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        // Toggle handled by parent component
      }
      if (e.key === 'Escape' && isOpen) {
        closePalette();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, closePalette]);

  // Combine search results with bookmarks if search matches
  const allSearchResults = searchQuery
    ? [...searchResults, ...bookmarkItems.filter(b => 
        b.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
        b.description?.toLowerCase().includes(searchQuery.toLowerCase())
      )]
    : searchResults;
  
  const groupedResults = groupResults(allSearchResults);

  // Show recent commands when no search query
  const showRecent = !searchQuery && recentCommands.length > 0;
  const showEmpty = searchQuery && allSearchResults.length === 0 && !loading;
  const lastUpdatedLabel = lastUpdated ? new Date(lastUpdated).toLocaleTimeString() : null;

  return (
    <CommandDialog
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) {
          closePalette();
        }
      }}
      title="Command Palette"
      description="Search for pages, adapters, tenants, policies, and more. Press / to open."
    >
      <CommandInput
        placeholder="Search pages, adapters, tenants..."
        value={searchQuery}
        onValueChange={setSearchQuery}
      />
      {(refreshError || lastUpdatedLabel) && (
        <div className="border-b px-3 py-2 space-y-1">
          {refreshError && (
            <div className="flex items-center gap-2 text-xs text-destructive">
              <AlertCircle className="h-4 w-4" />
              <span className="flex-1">{refreshError}</span>
              <button
                type="button"
                onClick={() => {
                  void refreshEntities();
                }}
                className="text-xs font-medium text-primary underline underline-offset-2 hover:text-primary/80"
              >
                Retry
              </button>
            </div>
          )}
          {lastUpdatedLabel && (
            <div className="text-[11px] text-muted-foreground">
              Updated {lastUpdatedLabel}
            </div>
          )}
        </div>
      )}
      <CommandList>
        {loading && searchQuery && (
          <div className="py-6 text-center text-sm text-muted-foreground">
            Searching...
          </div>
        )}
        
        {showEmpty && (
          <CommandEmpty>No results found.</CommandEmpty>
        )}

        {!showEmpty && (
          <>
            {searchQuery ? (
              // Show grouped search results
              Array.from(groupedResults.entries())
                .sort(([a], [b]) => {
                  const aIdx = groupOrder.indexOf(a);
                  const bIdx = groupOrder.indexOf(b);
                  if (aIdx !== -1 && bIdx !== -1) return aIdx - bIdx;
                  if (aIdx !== -1) return -1;
                  if (bIdx !== -1) return 1;
                  return a.localeCompare(b);
                })
                .map(([groupName, items]) => (
                  <CommandGroup key={groupName} heading={groupName}>
                    {items.map((item) => {
                      const Icon = getIconForItem(item);
                      const shortcut = item.shortcut ?? (item.type === 'page' ? '⌘K' : undefined);
                      return (
                        <CommandItem
                          key={item.id}
                          onSelect={() => executeCommand(item)}
                          value={`${item.id}-${item.title}`}
                        >
                          <Icon className="size-4" />
                          <div className="flex flex-col">
                            <span>{item.title}</span>
                            {item.description && (
                              <span className="text-xs text-muted-foreground">
                                {item.description}
                              </span>
                            )}
                          </div>
                          {shortcut && (
                            <CommandShortcut>{shortcut}</CommandShortcut>
                          )}
                        </CommandItem>
                      );
                    })}
                  </CommandGroup>
                ))
            ) : (
              // Show favorites and recent commands when no search
              <>
                {bookmarkItems.length > 0 && (
                  <CommandGroup heading="Favorites">
                    {bookmarkItems.slice(0, 5).map((item) => {
                      const Icon = getIconForItem(item);
                      return (
                        <CommandItem
                          key={item.id}
                          onSelect={() => executeCommand(item)}
                          value={`${item.id}-${item.title}`}
                        >
                          <Star className="size-4 fill-yellow-400 text-yellow-400" />
                          <Icon className="size-4" />
                          <div className="flex flex-col">
                            <span>{item.title}</span>
                            {item.description && (
                              <span className="text-xs text-muted-foreground">
                                {item.description}
                              </span>
                            )}
                          </div>
                        </CommandItem>
                      );
                    })}
                  </CommandGroup>
                )}
                {recentCommands.length > 0 && (
                  <CommandGroup heading="Recent">
                    {recentCommands.slice(0, 5).map((cmd) => {
                      const Icon = getIconForItem(cmd.item);
                      const shortcut = cmd.item.shortcut ?? (cmd.item.type === 'page' ? '⌘K' : undefined);
                      return (
                        <CommandItem
                          key={cmd.item.id}
                          onSelect={() => executeCommand(cmd.item)}
                          value={`${cmd.item.id}-${cmd.item.title}`}
                        >
                          <Clock className="size-4" />
                          <Icon className="size-4" />
                          <div className="flex flex-col">
                            <span>{cmd.item.title}</span>
                            {cmd.item.description && (
                              <span className="text-xs text-muted-foreground">
                                {cmd.item.description}
                              </span>
                            )}
                          </div>
                          {shortcut && <CommandShortcut>{shortcut}</CommandShortcut>}
                        </CommandItem>
                      );
                    })}
                  </CommandGroup>
                )}
                {quickActionItems.length > 0 && (
                  <CommandGroup heading="Quick Actions">
                    {quickActionItems.slice(0, 5).map((item) => {
                      const Icon = getIconForItem(item);
                      return (
                        <CommandItem
                          key={item.id}
                          onSelect={() => executeCommand(item)}
                          value={`${item.id}-${item.title}`}
                        >
                          <Icon className="size-4" />
                          <span>{item.title}</span>
                          {item.shortcut && <CommandShortcut>{item.shortcut}</CommandShortcut>}
                        </CommandItem>
                      );
                    })}
                  </CommandGroup>
                )}
                <CommandGroup heading="Quick Navigation">
                  <CommandItem
                    onSelect={() => executeCommand({ id: 'dashboard', type: 'page', title: 'Dashboard', url: '/dashboard', shortcut: '/' })}
                    value="dashboard"
                  >
                    <LayoutDashboard className="size-4" />
                    <span>Dashboard</span>
                    <CommandShortcut>/</CommandShortcut>
                  </CommandItem>
                  <CommandItem
                    onSelect={() => executeCommand({ id: 'adapters', type: 'page', title: 'Adapters', url: '/adapters', shortcut: '⌘2' })}
                    value="adapters"
                  >
                    <Box className="size-4" />
                    <span>Adapters</span>
                    <CommandShortcut>⌘2</CommandShortcut>
                  </CommandItem>
                  <CommandItem
                    onSelect={() => executeCommand({ id: 'inference', type: 'page', title: 'Inference', url: '/inference', shortcut: '⌘3' })}
                    value="inference"
                  >
                    <Play className="size-4" />
                    <span>Inference</span>
                    <CommandShortcut>⌘3</CommandShortcut>
                  </CommandItem>
                </CommandGroup>
              </>
            )}
          </>
        )}
      </CommandList>
    </CommandDialog>
  );
}
