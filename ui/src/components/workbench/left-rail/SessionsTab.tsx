/**
 * SessionsTab - Sessions list with search for the Workbench left rail
 *
 * Displays chat sessions with search filtering and quick actions.
 */

import { useState, useMemo } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { Plus, Search, MessageSquare, Trash2, MoreHorizontal } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { cn } from '@/lib/utils';
import { formatDistanceToNow } from 'date-fns';

interface ChatSession {
  id: string;
  name: string;
  stackId?: string;
  stackName?: string;
  updatedAt: Date;
  messageCount?: number;
}

interface SessionsTabProps {
  /** List of sessions */
  sessions: ChatSession[];
  /** Currently active session ID */
  activeSessionId?: string | null;
  /** Callback when a session is selected */
  onSelectSession: (sessionId: string) => void;
  /** Callback to create a new session */
  onCreateSession: () => void;
  /** Callback to delete a session */
  onDeleteSession?: (sessionId: string) => void;
  /** Loading state */
  isLoading?: boolean;
}

export function SessionsTab({
  sessions,
  activeSessionId,
  onSelectSession,
  onCreateSession,
  onDeleteSession,
  isLoading = false,
}: SessionsTabProps) {
  const [searchQuery, setSearchQuery] = useState('');

  const filteredSessions = useMemo(() => {
    if (!searchQuery.trim()) return sessions;
    const query = searchQuery.toLowerCase();
    return sessions.filter(
      (session) =>
        session.name.toLowerCase().includes(query) ||
        session.stackName?.toLowerCase().includes(query)
    );
  }, [sessions, searchQuery]);

  return (
    <div className="flex h-full flex-col" data-testid="sessions-tab">
      {/* Header with search and new button */}
      <div className="flex-none space-y-2 p-3 border-b">
        <div className="flex items-center gap-2">
          <div className="relative flex-1">
            <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Search sessions..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-8 h-9"
              data-testid="sessions-search"
            />
          </div>
          <Button
            size="sm"
            onClick={onCreateSession}
            className="h-9 px-3"
            data-testid="new-session-button"
          >
            <Plus className="h-4 w-4 mr-1" />
            New
          </Button>
        </div>
      </div>

      {/* Sessions list */}
      <ScrollArea className="flex-1">
        <div className="p-2 space-y-1">
          {isLoading ? (
            <div className="p-4 text-center text-sm text-muted-foreground">
              Loading sessions...
            </div>
          ) : filteredSessions.length === 0 ? (
            <div className="p-4 text-center text-sm text-muted-foreground">
              {searchQuery ? 'No sessions found' : 'No sessions yet'}
            </div>
          ) : (
            filteredSessions.map((session) => (
              <SessionItem
                key={session.id}
                session={session}
                isActive={session.id === activeSessionId}
                onSelect={() => onSelectSession(session.id)}
                onDelete={onDeleteSession ? () => onDeleteSession(session.id) : undefined}
              />
            ))
          )}
        </div>
      </ScrollArea>
    </div>
  );
}

interface SessionItemProps {
  session: ChatSession;
  isActive: boolean;
  onSelect: () => void;
  onDelete?: () => void;
}

function SessionItem({ session, isActive, onSelect, onDelete }: SessionItemProps) {
  return (
    <div
      className={cn(
        'group flex items-center gap-2 rounded-md px-2 py-2 cursor-pointer transition-colors',
        isActive
          ? 'bg-accent text-accent-foreground'
          : 'hover:bg-muted'
      )}
      onClick={onSelect}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onSelect();
        }
      }}
      data-testid={`session-${session.id}`}
    >
      <MessageSquare className="h-4 w-4 flex-none text-muted-foreground" />
      <div className="flex-1 min-w-0">
        <div className="font-medium text-sm truncate">{session.name}</div>
        <div className="text-xs text-muted-foreground truncate">
          {session.stackName && <span>{session.stackName} · </span>}
          {formatDistanceToNow(session.updatedAt, { addSuffix: true })}
        </div>
      </div>
      {onDelete && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7 opacity-0 group-hover:opacity-100 transition-opacity"
              onClick={(e) => e.stopPropagation()}
            >
              <MoreHorizontal className="h-4 w-4" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem
              onClick={(e) => {
                e.stopPropagation();
                onDelete();
              }}
              className="text-destructive focus:text-destructive"
            >
              <Trash2 className="h-4 w-4 mr-2" />
              Delete
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}
