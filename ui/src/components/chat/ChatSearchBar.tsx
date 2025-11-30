import React, { useState, useCallback, useRef, useEffect } from 'react';
import { Search, MessageSquare, FileText, Loader2, X } from 'lucide-react';
import { cn } from '@/components/ui/utils';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Command, CommandEmpty, CommandGroup, CommandItem, CommandList } from '@/components/ui/command';
import { useChatSearch } from '@/hooks/useChatSearch';
import type { ChatSearchResult } from '@/api/chat-types';

export interface ChatSearchBarProps {
  /**
   * Callback when a session is selected from search results
   */
  onSelectSession: (sessionId: string) => void;

  /**
   * Callback when a message is selected from search results
   */
  onSelectMessage: (sessionId: string, messageId: string) => void;

  /**
   * Optional className for the container
   */
  className?: string;

  /**
   * Placeholder text for the search input
   * @default "Search chat sessions and messages..."
   */
  placeholder?: string;

  /**
   * Maximum number of results to display
   * @default 20
   */
  maxResults?: number;
}

/**
 * ChatSearchBar - A search component for chat sessions and messages
 *
 * Features:
 * - Debounced search input to reduce API calls
 * - Real-time search results with highlighted snippets
 * - Filtering by scope (sessions, messages, all)
 * - Loading states and empty states
 * - Keyboard navigation support via Command component
 * - Click to navigate to session or specific message
 *
 * @example
 * ```tsx
 * <ChatSearchBar
 *   onSelectSession={(sessionId) => navigate(`/chat/${sessionId}`)}
 *   onSelectMessage={(sessionId, messageId) => {
 *     navigate(`/chat/${sessionId}#${messageId}`);
 *   }}
 * />
 * ```
 */
export function ChatSearchBar({
  onSelectSession,
  onSelectMessage,
  className,
  placeholder = 'Search chat sessions and messages...',
  maxResults = 20,
}: ChatSearchBarProps) {
  const [query, setQuery] = useState('');
  const [scope, setScope] = useState<'sessions' | 'messages' | 'all'>('all');
  const [isOpen, setIsOpen] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Use the chat search hook with debouncing
  const { results, isSearching, isPending, isValidQuery } = useChatSearch(query, {
    scope,
    limit: maxResults,
    debounceDelay: 300,
    minLength: 2,
  });

  // Handle result selection
  const handleSelectResult = useCallback((result: ChatSearchResult) => {
    if (result.match_type === 'session') {
      onSelectSession(result.session_id);
    } else if (result.match_type === 'message' && result.message_id) {
      onSelectMessage(result.session_id, result.message_id);
    }

    // Close the dropdown and clear search
    setIsOpen(false);
    setQuery('');
    inputRef.current?.blur();
  }, [onSelectSession, onSelectMessage]);

  // Handle input change
  const handleInputChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const value = e.target.value;
    setQuery(value);
    setIsOpen(value.length >= 2);
  }, []);

  // Handle clear button
  const handleClear = useCallback(() => {
    setQuery('');
    setIsOpen(false);
    inputRef.current?.focus();
  }, []);

  // Close dropdown when clicking outside
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    }

    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  // Determine if we should show the dropdown
  const shouldShowDropdown = isOpen && (isSearching || isPending || results.length > 0 || (isValidQuery && !isSearching));

  return (
    <div ref={containerRef} className={cn('relative w-full', className)}>
      {/* Search input with scope filters */}
      <div className="flex flex-col gap-2">
        <div className="relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
          <Input
            ref={inputRef}
            type="text"
            value={query}
            onChange={handleInputChange}
            placeholder={placeholder}
            className="pl-9 pr-9"
            aria-label="Search chat sessions and messages"
            aria-expanded={shouldShowDropdown}
            aria-autocomplete="list"
          />
          {query && (
            <button
              onClick={handleClear}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
              aria-label="Clear search"
            >
              <X className="h-4 w-4" />
            </button>
          )}
        </div>

        {/* Scope filter buttons */}
        <div className="flex gap-2">
          <button
            onClick={() => setScope('all')}
            className={cn(
              'px-3 py-1 text-xs rounded-md transition-colors',
              scope === 'all'
                ? 'bg-primary text-primary-foreground'
                : 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
            )}
          >
            All
          </button>
          <button
            onClick={() => setScope('sessions')}
            className={cn(
              'px-3 py-1 text-xs rounded-md transition-colors flex items-center gap-1',
              scope === 'sessions'
                ? 'bg-primary text-primary-foreground'
                : 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
            )}
          >
            <FileText className="h-3 w-3" />
            Sessions
          </button>
          <button
            onClick={() => setScope('messages')}
            className={cn(
              'px-3 py-1 text-xs rounded-md transition-colors flex items-center gap-1',
              scope === 'messages'
                ? 'bg-primary text-primary-foreground'
                : 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
            )}
          >
            <MessageSquare className="h-3 w-3" />
            Messages
          </button>
        </div>
      </div>

      {/* Search results dropdown */}
      {shouldShowDropdown && (
        <div className="absolute top-full left-0 right-0 mt-2 bg-popover border border-border rounded-md shadow-lg z-50 max-h-[400px] overflow-hidden">
          <Command className="rounded-md">
            <CommandList>
              {/* Loading state */}
              {(isSearching || isPending) && (
                <div className="flex items-center justify-center py-6 text-sm text-muted-foreground">
                  <Loader2 className="h-4 w-4 animate-spin mr-2" />
                  {isPending ? 'Typing...' : 'Searching...'}
                </div>
              )}

              {/* Empty state */}
              {!isSearching && !isPending && results.length === 0 && isValidQuery && (
                <CommandEmpty>
                  <div className="text-center py-6">
                    <p className="text-sm text-muted-foreground">No results found</p>
                    <p className="text-xs text-muted-foreground mt-1">
                      Try adjusting your search query or filters
                    </p>
                  </div>
                </CommandEmpty>
              )}

              {/* Results */}
              {!isSearching && !isPending && results.length > 0 && (
                <ScrollArea className="max-h-[350px]">
                  <CommandGroup heading={`${results.length} result${results.length === 1 ? '' : 's'}`}>
                    {results.map((result, index) => (
                      <CommandItem
                        key={`${result.session_id}-${result.message_id || 'session'}-${index}`}
                        onSelect={() => handleSelectResult(result)}
                        className="flex flex-col items-start gap-2 py-3 cursor-pointer"
                      >
                        {/* Result header with session name and type badge */}
                        <div className="flex items-center justify-between w-full">
                          <div className="flex items-center gap-2">
                            {result.match_type === 'session' ? (
                              <FileText className="h-4 w-4 text-muted-foreground" />
                            ) : (
                              <MessageSquare className="h-4 w-4 text-muted-foreground" />
                            )}
                            <span className="font-medium text-sm truncate max-w-[300px]">
                              {result.session_name}
                            </span>
                          </div>
                          <div className="flex items-center gap-2">
                            <Badge variant="outline" className="text-xs">
                              {result.match_type === 'session' ? 'Session' : 'Message'}
                            </Badge>
                            {result.relevance_score > 0 && (
                              <Badge variant="secondary" className="text-xs">
                                {Math.round(result.relevance_score * 100)}%
                              </Badge>
                            )}
                          </div>
                        </div>

                        {/* Snippet with highlighted text */}
                        <div className="text-xs text-muted-foreground line-clamp-2 w-full pl-6">
                          {result.snippet}
                        </div>

                        {/* Message role badge if it's a message result */}
                        {result.match_type === 'message' && result.message_role && (
                          <Badge variant="neutral" className="text-xs ml-6">
                            {result.message_role}
                          </Badge>
                        )}

                        {/* Last activity timestamp */}
                        <div className="text-xs text-muted-foreground/70 pl-6">
                          {new Date(result.last_activity_at).toLocaleString()}
                        </div>
                      </CommandItem>
                    ))}
                  </CommandGroup>
                </ScrollArea>
              )}
            </CommandList>
          </Command>
        </div>
      )}
    </div>
  );
}

export default ChatSearchBar;
