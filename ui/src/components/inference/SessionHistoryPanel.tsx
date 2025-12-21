import React from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { History } from 'lucide-react';
import { InferenceSession } from '@/api/types';

export interface SessionHistoryPanelProps {
  /** Recent inference sessions */
  sessions: InferenceSession[];
  /** Callback when session is selected to restore */
  onLoadSession: (session: InferenceSession) => void;
  /** Maximum number of sessions to display */
  maxSessions?: number;
}

/**
 * Panel showing recent inference sessions for quick restoration.
 */
export function SessionHistoryPanel({
  sessions,
  onLoadSession,
  maxSessions = 5,
}: SessionHistoryPanelProps) {
  if (sessions.length === 0) {
    return null;
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base flex items-center gap-2">
          <History className="h-4 w-4" aria-hidden="true" />
          Recent Sessions
        </CardTitle>
      </CardHeader>
      <CardContent>
        <ul className="space-y-2" role="list" aria-label="Recent inference sessions">
          {sessions.slice(0, maxSessions).map((session) => (
            <li key={session.id}>
              <Button
                variant="ghost"
                className="w-full justify-start text-left h-auto py-2"
                onClick={() => onLoadSession(session)}
                aria-label={`Load session from ${new Date(session.created_at).toLocaleString()}: ${session.prompt.slice(0, 50)}${session.prompt.length > 50 ? '...' : ''}`}
              >
                <div className="truncate">
                  <p className="text-sm truncate">{session.prompt}</p>
                  <p className="text-xs text-muted-foreground">
                    {new Date(session.created_at).toLocaleString()}
                  </p>
                </div>
              </Button>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
