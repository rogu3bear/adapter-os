import React, { useEffect, useRef } from 'react';
import { ScrollArea } from './ui/scroll-area';
import { cn } from '@/lib/utils';

interface TerminalOutputProps {
  logs: string[];
  className?: string;
  maxHeight?: string;
}

export function TerminalOutput({
  logs,
  className,
  maxHeight = "400px"
}: TerminalOutputProps) {
  const scrollRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom when new logs arrive
  useEffect(() => {
    if (scrollRef.current) {
      const scrollElement = scrollRef.current.querySelector('[data-radix-scroll-area-viewport]');
      if (scrollElement) {
        scrollElement.scrollTop = scrollElement.scrollHeight;
      }
    }
  }, [logs]);

  const formatTimestamp = (timestamp?: string) => {
    if (!timestamp) return '';
    try {
      const date = new Date(timestamp);
      return date.toLocaleTimeString('en-US', {
        hour12: false,
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit'
      });
    } catch {
      return '';
    }
  };

  const parseLogLine = (line: string, index: number) => {
    // Try to extract timestamp and level from common log formats
    const timestampRegex = /^\[?(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z?)\]?/;
    const levelRegex = /\b(INFO|WARN|ERROR|DEBUG|TRACE)\b/i;

    const timestampMatch = line.match(timestampRegex);
    const levelMatch = line.match(levelRegex);

    let timestamp = '';
    let level = '';
    let message = line;

    if (timestampMatch) {
      timestamp = formatTimestamp(timestampMatch[1]);
      message = line.replace(timestampMatch[0], '').trim();
    }

    if (levelMatch) {
      level = levelMatch[1].toUpperCase();
      message = message.replace(levelMatch[0], '').trim();
    }

    return { timestamp, level, message, original: line };
  };

  const getLevelColor = (level: string) => {
    switch (level.toUpperCase()) {
      case 'ERROR': return 'text-red-600';
      case 'WARN':
      case 'WARNING': return 'text-yellow-600';
      case 'INFO': return 'text-blue-600';
      case 'DEBUG': return 'text-gray-600';
      case 'TRACE': return 'text-gray-500';
      default: return 'text-gray-900';
    }
  };

  return (
    <div
      ref={scrollRef}
      className={cn(
        "bg-gray-900 text-gray-100 rounded-lg border border-gray-700 overflow-hidden",
        className
      )}
      style={{ maxHeight }}
    >
      <ScrollArea className="h-full">
        <div className="p-4 font-mono text-sm space-y-1">
          {logs.length === 0 ? (
            <div className="text-gray-500 italic">
              No logs available. Start the service to see output here.
            </div>
          ) : (
            logs.map((line, index) => {
              const { timestamp, level, message, original } = parseLogLine(line, index);

              return (
                <div key={index} className="flex gap-2 leading-relaxed">
                  {timestamp && (
                    <span className="text-gray-500 text-xs whitespace-nowrap">
                      {timestamp}
                    </span>
                  )}
                  {level && (
                    <span className={cn(
                      "text-xs font-bold whitespace-nowrap px-1 rounded",
                      getLevelColor(level)
                    )}>
                      {level}
                    </span>
                  )}
                  <span className="flex-1 break-words">
                    {message || original}
                  </span>
                </div>
              );
            })
          )}
        </div>
      </ScrollArea>

      {/* Terminal-style prompt at bottom */}
      <div className="border-t border-gray-700 bg-gray-800 px-4 py-2">
        <div className="flex items-center gap-2">
          <span className="text-green-400">$</span>
          <span className="text-gray-300">
            {logs.length} lines • Auto-scroll enabled
          </span>
        </div>
      </div>
    </div>
  );
}
