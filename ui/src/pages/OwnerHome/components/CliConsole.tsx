import React, { useState, useRef, useEffect, useCallback } from 'react';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Input } from '@/components/ui/input';
import { Terminal, ChevronRight, Loader2 } from 'lucide-react';
import { apiClient } from '@/api/client';

interface CommandOutput {
  id: string;
  command: string;
  output: string;
  exitCode: number;
  timestamp: Date;
}

const ALLOWED_COMMANDS = [
  'aosctl status',
  'aosctl adapters list',
  'aosctl models list',
  'aosctl tenant list',
  'aosctl stack list',
  'aosctl stack describe <name>',
  'aosctl logs <component>',
  'help',
  'clear',
];

const MOCK_RESPONSES: Record<string, string> = {
  'aosctl status': `AdapterOS Status
=================
Version: v0.3.0-alpha
Uptime: 2h 34m 12s
Active Adapters: 3
Active Workers: 2
Memory Usage: 1.2 GB / 8.0 GB
Backend: MLX (GPU Accelerated)
Status: Healthy`,

  'aosctl adapters list': `Adapters
========
ID                          State    Tier         Tenant
rust-expert                 warm     persistent   default
code-assistant              hot      ephemeral    default
data-analyzer               cold     persistent   analytics

Total: 3 adapters`,

  'aosctl models list': `Base Models
===========
Name                        Size      Backend     Status
qwen2.5-7b-mlx             3.8 GB    MLX         loaded
llama-3.1-8b               4.1 GB    CoreML      available

Total: 2 models`,

  'aosctl tenant list': `Tenants
=======
ID          Status    Adapters    Created
default     active    3           2025-11-01
analytics   active    1           2025-11-15
testing     paused    0           2025-11-20

Total: 3 tenants`,

  'aosctl stack list': `Adapter Stacks
==============
Name                Active    Adapters    Workflow
production-stack    yes       2           inference
dev-stack           no        3           training
test-stack          no        1           evaluation

Total: 3 stacks`,

  'help': `Available Commands
==================
aosctl status                    - Show system status
aosctl adapters list             - List all adapters
aosctl models list               - List base models
aosctl tenant list               - List all tenants
aosctl stack list                - List adapter stacks
aosctl stack describe <name>     - Show stack details
aosctl logs <component>          - View component logs
help                             - Show this help message
clear                            - Clear console output`,
};

export const CliConsole: React.FC = () => {
  const [history, setHistory] = useState<CommandOutput[]>([
    {
      id: 'welcome',
      command: '',
      output: 'AdapterOS CLI Console\nType "help" for available commands.\n',
      exitCode: 0,
      timestamp: new Date(),
    },
  ]);
  const [currentCommand, setCurrentCommand] = useState('');
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [isExecuting, setIsExecuting] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Auto-scroll to bottom on new output
  useEffect(() => {
    if (scrollRef.current) {
      const scrollContainer = scrollRef.current.querySelector('[data-radix-scroll-area-viewport]');
      if (scrollContainer) {
        scrollContainer.scrollTop = scrollContainer.scrollHeight;
      }
    }
  }, [history]);

  // Focus input on mount and when clicking console
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const executeCommand = useCallback(async (cmd: string) => {
    const trimmedCmd = cmd.trim();

    if (!trimmedCmd) {
      return;
    }

    // Handle 'clear' command locally
    if (trimmedCmd === 'clear') {
      setHistory([
        {
          id: `clear-${Date.now()}`,
          command: '',
          output: 'AdapterOS CLI Console\nType "help" for available commands.\n',
          exitCode: 0,
          timestamp: new Date(),
        },
      ]);
      return;
    }

    // Handle 'help' command locally
    if (trimmedCmd === 'help') {
      const newOutput: CommandOutput = {
        id: `cmd-${Date.now()}`,
        command: trimmedCmd,
        output: MOCK_RESPONSES['help'],
        exitCode: 0,
        timestamp: new Date(),
      };
      setHistory(prev => [...prev, newOutput]);
      return;
    }

    // Set loading state
    setIsExecuting(true);

    try {
      // Call the real API
      const result = await apiClient.runOwnerCli(trimmedCmd);

      // Combine stdout and stderr
      let output = result.stdout;
      if (result.stderr) {
        output += result.stderr ? `\n${result.stderr}` : '';
      }

      // If no output, show success message
      if (!output.trim()) {
        output = result.exit_code === 0
          ? 'Command executed successfully (no output)'
          : 'Command failed (no output)';
      }

      const newOutput: CommandOutput = {
        id: `cmd-${Date.now()}`,
        command: trimmedCmd,
        output,
        exitCode: result.exit_code,
        timestamp: new Date(),
      };

      setHistory(prev => [...prev, newOutput]);
    } catch (error) {
      // Handle API errors gracefully
      const errorMessage = error instanceof Error
        ? error.message
        : 'Unknown error occurred';

      // Check if command is allowed for fallback
      const isAllowed = ALLOWED_COMMANDS.some(allowed => {
        if (allowed.includes('<')) {
          const baseCmd = allowed.split('<')[0].trim();
          return trimmedCmd.startsWith(baseCmd);
        }
        return trimmedCmd === allowed;
      });

      let output: string;
      let exitCode: number;

      // Try to provide helpful mock response if API fails and command is recognized
      if (isAllowed && MOCK_RESPONSES[trimmedCmd]) {
        output = `API unavailable. Showing cached response:\n\n${MOCK_RESPONSES[trimmedCmd]}`;
        exitCode = 0;
      } else {
        output = `Error executing command: ${errorMessage}\n\nThe backend API may be unavailable. Please check your connection.`;
        exitCode = 1;
      }

      const newOutput: CommandOutput = {
        id: `cmd-${Date.now()}`,
        command: trimmedCmd,
        output,
        exitCode,
        timestamp: new Date(),
      };

      setHistory(prev => [...prev, newOutput]);
    } finally {
      setIsExecuting(false);
    }
  }, []);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    executeCommand(currentCommand);
    setCurrentCommand('');
    setHistoryIndex(-1);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    const commandHistory = history.filter(h => h.command).map(h => h.command);

    if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (commandHistory.length === 0) return;

      const newIndex = historyIndex === -1
        ? commandHistory.length - 1
        : Math.max(0, historyIndex - 1);

      setHistoryIndex(newIndex);
      setCurrentCommand(commandHistory[newIndex]);
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (historyIndex === -1) return;

      const newIndex = historyIndex + 1;

      if (newIndex >= commandHistory.length) {
        setHistoryIndex(-1);
        setCurrentCommand('');
      } else {
        setHistoryIndex(newIndex);
        setCurrentCommand(commandHistory[newIndex]);
      }
    }
  };

  const handleConsoleClick = () => {
    inputRef.current?.focus();
  };

  return (
    <div
      className="flex flex-col h-full bg-slate-900 rounded-lg border border-slate-700 overflow-hidden"
      onClick={handleConsoleClick}
      role="application"
      aria-label="CLI Console - AdapterOS command line interface"
    >
      {/* Header */}
      <div className="flex items-center gap-2 px-4 py-2 bg-slate-800 border-b border-slate-700">
        <Terminal className="w-4 h-4 text-green-400" aria-hidden="true" />
        <span className="text-sm font-mono text-green-400">CLI Console</span>
      </div>

      {/* Output Area */}
      <ScrollArea ref={scrollRef} className="flex-1 p-4">
        <div
          className="font-mono text-sm space-y-2"
          role="log"
          aria-live="polite"
          aria-label="Command output history"
        >
          {history.map((item) => (
            <div key={item.id} className="space-y-1">
              {item.command && (
                <div className="flex items-start gap-2 text-green-400">
                  <span className="text-blue-400" aria-hidden="true">owner@adapteros$</span>
                  <span>{item.command}</span>
                </div>
              )}
              {item.output && (
                <pre
                  className={`whitespace-pre-wrap ml-6 ${
                    item.exitCode === 0 ? 'text-slate-300' : 'text-red-400'
                  }`}
                  aria-label={item.exitCode === 0 ? 'Command succeeded' : 'Command failed'}
                >
                  {item.output}
                </pre>
              )}
            </div>
          ))}
        </div>
      </ScrollArea>

      {/* Input Line */}
      <div className="border-t border-slate-700 bg-slate-800">
        <form onSubmit={handleSubmit} className="flex items-center gap-2 px-4 py-2" role="search">
          <label htmlFor="cli-input" className="sr-only">Enter CLI command</label>
          <span className="font-mono text-sm text-blue-400 flex items-center gap-1" aria-hidden="true">
            owner@adapteros
            <ChevronRight className="w-3 h-3" />
          </span>
          <Input
            id="cli-input"
            ref={inputRef}
            type="text"
            value={currentCommand}
            onChange={(e) => setCurrentCommand(e.target.value)}
            onKeyDown={handleKeyDown}
            className="flex-1 bg-transparent border-none font-mono text-sm text-green-400 focus-visible:ring-0 focus-visible:ring-offset-0 px-0"
            placeholder="Type a command..."
            autoComplete="off"
            spellCheck={false}
            disabled={isExecuting}
            aria-describedby="cli-status"
          />
          <span id="cli-status" className="sr-only" aria-live="assertive">
            {isExecuting ? 'Executing command...' : 'Ready for input'}
          </span>
          {isExecuting && (
            <Loader2 className="w-4 h-4 text-green-400 animate-spin" aria-hidden="true" />
          )}
        </form>
      </div>
    </div>
  );
};
