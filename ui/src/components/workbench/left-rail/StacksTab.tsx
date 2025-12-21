/**
 * StacksTab - Adapter stacks list with Detach All and Reset Default buttons
 *
 * Displays available stacks and provides escape hatch controls.
 */

import { useState, useMemo } from 'react';
import { Search, Layers, Check, Loader2 } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Badge } from '@/components/ui/badge';
import { Separator } from '@/components/ui/separator';
import { cn } from '@/lib/utils';
import {
  useAdapterStacks,
  useActivateAdapterStack,
  useGetDefaultStack,
} from '@/hooks/admin/useAdmin';
import { useTenant } from '@/providers/FeatureProviders';
import { useWorkbench } from '@/contexts/WorkbenchContext';
import { DetachAllButton } from '@/components/workbench/controls/DetachAllButton';
import { ResetDefaultButton } from '@/components/workbench/controls/ResetDefaultButton';
import { SaveAsDefaultButton } from '@/components/workbench/controls/SaveAsDefaultButton';

interface StacksTabProps {
  /** Currently active stack ID */
  activeStackId?: string | null;
  /** Session ID for clearing stack selection */
  sessionId?: string | null;
  /** Callback when a stack is activated */
  onStackActivated?: (stackId: string) => void;
  /** Callback to clear stack selection (for Detach All) */
  onClearStack?: () => void;
}

export function StacksTab({ activeStackId, sessionId, onStackActivated, onClearStack }: StacksTabProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const { selectedTenant } = useTenant();
  const { strengthOverrides, clearStrengthOverrides } = useWorkbench();

  const { data: stacks = [], isLoading } = useAdapterStacks();
  const { data: defaultStackData } = useGetDefaultStack(selectedTenant);
  const activateStack = useActivateAdapterStack();

  const defaultStackId = defaultStackData?.id ?? null;

  const filteredStacks = useMemo(() => {
    if (!searchQuery.trim()) return stacks;
    const query = searchQuery.toLowerCase();
    return stacks.filter(
      (stack) =>
        stack.name.toLowerCase().includes(query) ||
        stack.description?.toLowerCase().includes(query)
    );
  }, [stacks, searchQuery]);

  const handleActivateStack = async (stackId: string) => {
    try {
      await activateStack.mutateAsync(stackId);
      onStackActivated?.(stackId);
    } catch (error) {
      // Error handling is done in the mutation hook
    }
  };

  return (
    <div className="flex h-full flex-col" data-testid="stacks-tab">
      {/* Header with search */}
      <div className="flex-none space-y-2 p-3 border-b">
        <div className="relative">
          <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
          <Input
            placeholder="Search stacks..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="pl-8 h-9"
            data-testid="stacks-search"
          />
        </div>
      </div>

      {/* Stacks list */}
      <ScrollArea className="flex-1">
        <div className="p-2 space-y-1">
          {isLoading ? (
            <div className="p-4 text-center text-sm text-muted-foreground">
              Loading stacks...
            </div>
          ) : filteredStacks.length === 0 ? (
            <div className="p-4 text-center text-sm text-muted-foreground">
              {searchQuery ? 'No stacks found' : 'No stacks available'}
            </div>
          ) : (
            filteredStacks.map((stack) => (
              <StackItem
                key={stack.id}
                stack={stack}
                isActive={stack.id === activeStackId}
                isDefault={stack.id === defaultStackId}
                isActivating={activateStack.isPending}
                onActivate={() => handleActivateStack(stack.id)}
              />
            ))
          )}
        </div>
      </ScrollArea>

      {/* Footer with escape hatch controls */}
      <div className="flex-none border-t p-3 space-y-2">
        <DetachAllButton
          activeStackId={activeStackId ?? null}
          sessionId={sessionId ?? null}
          adapterOverrides={strengthOverrides}
          onDetach={clearStrengthOverrides}
          onClearStack={onClearStack}
        />
        <SaveAsDefaultButton
          activeStackId={activeStackId ?? null}
          currentDefaultStackId={defaultStackId}
        />
        <ResetDefaultButton
          defaultStackId={defaultStackId}
          activeStackId={activeStackId ?? null}
        />
      </div>
    </div>
  );
}

interface StackItemProps {
  stack: {
    id: string;
    name: string;
    description?: string | null;
    adapters?: Array<{ adapter_id: string }>;
    is_active?: boolean;
  };
  isActive: boolean;
  isDefault: boolean;
  isActivating: boolean;
  onActivate: () => void;
}

function StackItem({
  stack,
  isActive,
  isDefault,
  isActivating,
  onActivate,
}: StackItemProps) {
  const adapterCount = stack.adapters?.length ?? 0;

  return (
    <div
      className={cn(
        'group flex items-center gap-2 rounded-md px-2 py-2 cursor-pointer transition-colors',
        isActive
          ? 'bg-primary/10 border border-primary/20'
          : 'hover:bg-muted'
      )}
      onClick={onActivate}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onActivate();
        }
      }}
      data-testid={`stack-${stack.id}`}
    >
      {isActivating ? (
        <Loader2 className="h-4 w-4 flex-none animate-spin text-muted-foreground" />
      ) : (
        <Layers className="h-4 w-4 flex-none text-muted-foreground" />
      )}
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="font-medium text-sm truncate">{stack.name}</span>
          {isActive && <Check className="h-3.5 w-3.5 text-primary flex-none" />}
          {isDefault && (
            <Badge variant="secondary" className="text-xs py-0 h-4">
              Default
            </Badge>
          )}
        </div>
        <div className="text-xs text-muted-foreground">
          {adapterCount} adapter{adapterCount !== 1 ? 's' : ''}
          {stack.description && (
            <span className="truncate"> · {stack.description}</span>
          )}
        </div>
      </div>
    </div>
  );
}
