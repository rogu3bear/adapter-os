import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { useLocation } from 'react-router-dom';
import { Eye, MessageSquare, Orbit, Send, Sparkles, X } from 'lucide-react';
import { toast } from 'sonner';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Textarea } from '@/components/ui/textarea';
import { useCopilot } from '@/contexts/CopilotContext';
import { useTopologyData } from '@/hooks/topology/useTopologyData';
import { useRouterEvents } from '@/hooks/realtime/useRouterEvents';
import { ThoughtTopologyPanel } from '@/components/thought-topology/ThoughtTopologyPanel';
import { cn } from '@/lib/utils';

export function CopilotDrawer() {
  const { isOpen, toggleDrawer, closeDrawer, currentContext, messageHistory, addMessage, assistantLabel } = useCopilot();
  const location = useLocation();
  const [draft, setDraft] = useState('');
  const scrollAnchorRef = useRef<HTMLDivElement | null>(null);
  const [driftWarning, setDriftWarning] = useState(false);
  const [driftDistance, setDriftDistance] = useState<number | null>(null);

  const { data: topology, isLoading: topologyLoading, refetch: refetchTopology } = useTopologyData({
    enabled: isOpen,
    previewText: draft,
  });
  const routerEvents = useRouterEvents({
    enabled: isOpen,
    startingClusterId: topology?.startingClusterId ?? null,
    trailLimit: 32,
  });
  const {
    state: routerState,
    steps: routerSteps,
    swaps: routerSwaps,
    connected,
    circuitOpen,
    reconnectAttempts,
    error,
    reconnect,
    forceClusterLock,
  } = routerEvents;

  const highlightClusterId = useMemo(() => {
    if (!topology || !location.pathname.toLowerCase().includes('settings')) return null;
    const match = topology.clusters.find((cluster) => {
      const id = cluster.id.toLowerCase();
      const name = (cluster.name ?? '').toLowerCase();
      return id.includes('config') || name.includes('config');
    });
    return match?.id ?? null;
  }, [location.pathname, topology]);

  const handleForceCluster = useCallback(async (clusterId: string) => {
    try {
      await forceClusterLock(clusterId);
      toast.success(`Router locked to ${clusterId}`);
    } catch (error) {
      toast.error('Failed to lock router cluster');
    }
  }, [forceClusterLock]);

  const connection = useMemo(
    () => ({
      connected,
      circuitOpen,
      reconnectAttempts,
      error,
      reconnect,
    }),
    [connected, circuitOpen, error, reconnect, reconnectAttempts],
  );

  const visibleMessages = useMemo(() => messageHistory.filter((msg) => !msg.hidden), [messageHistory]);

  useEffect(() => {
    if (!isOpen) return;
    const id = window.setTimeout(() => {
      scrollAnchorRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' });
    }, 50);
    return () => window.clearTimeout(id);
  }, [isOpen, visibleMessages.length]);

  const handleSend = (event?: FormEvent | ReactKeyboardEvent) => {
    event?.preventDefault();
    const content = draft.trim();
    if (!content) return;
    addMessage('user', content);
    setDraft('');
  };

  return (
    <>
      <div
        className={cn(
          'fixed inset-0 z-40 bg-background/40 backdrop-blur-[2px] transition-opacity duration-300',
          isOpen ? 'opacity-100 pointer-events-auto' : 'opacity-0 pointer-events-none'
        )}
        onClick={closeDrawer}
        aria-hidden
      />

      <div
        id="copilot-drawer"
        aria-label="Global copilot"
        className={cn(
          'fixed inset-y-0 right-0 z-50 flex h-full w-full flex-col border-l border-border/60 bg-background/80 shadow-2xl backdrop-blur-md transition-transform duration-300 ease-in-out',
          'md:my-4 md:w-[440px] md:max-w-[480px] md:rounded-l-2xl',
          isOpen ? 'translate-x-0' : 'translate-x-full'
        )}
      >
        <div className="flex items-start justify-between border-b border-border/70 px-5 py-4">
          <div className="space-y-1">
            <div className="flex items-center gap-2 text-sm font-semibold">
              <Sparkles className="h-4 w-4 text-primary" />
              <span>{assistantLabel}</span>
              <Badge variant="secondary" className="text-[11px] font-medium">
                {currentContext.pageTitle}
              </Badge>
            </div>
            <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
              <Badge variant="outline" className="border-dashed">
                Context-aware
              </Badge>
              <Badge variant="secondary" className="flex items-center gap-1" title={currentContext.screen.systemContext}>
                <Eye className="h-3 w-3" />
                {currentContext.screen.adapterLabel}
              </Badge>
              {driftWarning && (
                <Badge variant="destructive" className="flex items-center gap-1">
                  <Orbit className="h-3 w-3" />
                  Drift
                  {typeof driftDistance === 'number' && Number.isFinite(driftDistance) ? Math.round(driftDistance) : null}
                </Badge>
              )}
              <span className="truncate max-w-[240px]" title={currentContext.url}>
                {currentContext.url}
              </span>
              <span className="hidden text-muted-foreground md:inline">• Hotkey: Cmd/Ctrl + \</span>
            </div>
          </div>
          <Button variant="ghost" size="icon" aria-label="Close Copilot drawer" onClick={closeDrawer}>
            <X className="h-4 w-4" />
          </Button>
        </div>

        <div className="flex flex-1 flex-col">
          <div className="px-5 py-4">
            <ThoughtTopologyPanel
              topology={topology}
              isLoading={topologyLoading}
              routerState={routerState}
              routerSteps={routerSteps}
              reasoningSwaps={routerSwaps}
              connection={connection}
              highlightClusterId={highlightClusterId}
              onRefresh={refetchTopology}
              onForceCluster={handleForceCluster}
              onDriftChange={(warning, distance) => {
                setDriftWarning(warning);
                setDriftDistance(distance);
              }}
            />
          </div>

          <div className="flex min-h-0 flex-1 flex-col">
            <ScrollArea className="flex-1 px-5 py-4">
              {visibleMessages.length === 0 ? (
                <div className="rounded-xl border border-dashed border-border/70 bg-muted/40 px-4 py-3 text-sm text-muted-foreground shadow-inner">
                  <div className="flex items-center gap-2 text-foreground">
                    <MessageSquare className="h-4 w-4 text-primary" />
                    <span>Global Copilot</span>
                  </div>
                  <p className="mt-2">
                    Ask anything from here. We keep your thread alive as you move between pages and include the page title
                    and role context automatically.
                  </p>
                </div>
              ) : (
                <div className="space-y-3">
                  {visibleMessages.map((message) => (
                    <div
                      key={message.id}
                      className={cn(
                        'rounded-xl border px-3 py-2 shadow-sm',
                        message.role === 'assistant'
                          ? 'border-primary/40 bg-primary/5'
                          : 'border-border/70 bg-muted/40'
                      )}
                    >
                      <div className="flex items-center gap-2 text-[11px] font-medium uppercase text-muted-foreground">
                        <Badge variant="outline" className="px-1.5 py-0 text-[10px]">
                          {message.role === 'assistant' ? assistantLabel : 'You'}
                        </Badge>
                        <span className="text-[10px]">
                          {new Date(message.createdAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                        </span>
                      </div>
                      <div className="mt-1 whitespace-pre-wrap text-sm text-foreground/90">{message.content}</div>
                    </div>
                  ))}
                  <div ref={scrollAnchorRef} />
                </div>
              )}
            </ScrollArea>

            <form onSubmit={handleSend} className="border-t border-border/70 bg-muted/30 px-4 py-3">
              <div className="rounded-xl border border-border/70 bg-background/80 shadow-inner">
                <Textarea
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  placeholder="Ask anything. We’ll include where you are and who you are."
                  className="min-h-[84px] resize-none border-0 bg-transparent focus-visible:ring-0"
                  onKeyDown={(e) => {
                    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
                      e.preventDefault();
                      handleSend(e);
                    }
                  }}
                />
                <div className="flex items-center justify-between border-t border-border/70 px-3 py-2 text-xs text-muted-foreground">
                  <span className="hidden md:inline">Cmd/Ctrl + Enter to send • Cmd/Ctrl + \ to toggle</span>
                  <Button type="submit" size="sm" className="gap-2">
                    <Send className="h-4 w-4" />
                    Send
                  </Button>
                </div>
              </div>
            </form>
          </div>
        </div>
      </div>

      <Button
        variant="default"
        size="lg"
        className="fixed bottom-6 right-6 z-40 flex items-center gap-2 rounded-full shadow-lg backdrop-blur px-4 py-3 md:px-5"
        onClick={toggleDrawer}
        aria-expanded={isOpen}
        aria-controls="copilot-drawer"
      >
        <Sparkles className="h-4 w-4" />
        <span className="hidden sm:inline">{assistantLabel}</span>
        <span className="text-xs text-primary-foreground/70 sm:hidden">Chat</span>
      </Button>
    </>
  );
}
