import React from 'react';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { Checkbox } from '@/components/ui/checkbox';
import { Progress } from '@/components/ui/progress';
import { Skeleton } from '@/components/ui/skeleton';
import { EmptyState } from '@/components/ui/empty-state';
import {
  Code,
  Layers,
  GitBranch,
  Clock,
  Pin,
  Snowflake,
  Thermometer,
  Flame,
  Anchor,
  Square,
  MemoryStick,
} from 'lucide-react';
import type { Adapter, AdapterState } from '@/api/adapter-types';
import { AdapterActions } from './AdapterActions';

interface AdapterTableProps {
  adapters: Adapter[];
  isLoading?: boolean;
  selectedAdapters?: string[];
  onSelectionChange?: (selectedIds: string[]) => void;
  onLoad?: (adapterId: string) => void;
  onUnload?: (adapterId: string) => void;
  onDelete?: (adapterId: string) => void;
  onPin?: (adapterId: string, pinned: boolean) => void;
  onPromote?: (adapterId: string) => void;
  onEvict?: (adapterId: string) => void;
  onViewHealth?: (adapterId: string) => void;
  onDownloadManifest?: (adapterId: string) => void;
  canLoad?: boolean;
  canUnload?: boolean;
  canDelete?: boolean;
  totalMemory?: number;
}

export function AdapterTable({
  adapters,
  isLoading = false,
  selectedAdapters = [],
  onSelectionChange,
  onLoad,
  onUnload,
  onDelete,
  onPin,
  onPromote,
  onEvict,
  onViewHealth,
  onDownloadManifest,
  canLoad = true,
  canUnload = true,
  canDelete = true,
  totalMemory = 0,
}: AdapterTableProps) {
  const allSelected = adapters.length > 0 && selectedAdapters.length === adapters.length;
  const someSelected = selectedAdapters.length > 0 && selectedAdapters.length < adapters.length;

  const toggleSelectAll = () => {
    if (allSelected) {
      onSelectionChange?.([]);
    } else {
      onSelectionChange?.(adapters.map(a => a.adapter_id));
    }
  };

  const toggleSelectOne = (adapterId: string) => {
    if (selectedAdapters.includes(adapterId)) {
      onSelectionChange?.(selectedAdapters.filter(id => id !== adapterId));
    } else {
      onSelectionChange?.([...selectedAdapters, adapterId]);
    }
  };

  if (isLoading) {
    return <AdapterTableSkeleton />;
  }

  if (adapters.length === 0) {
    return (
      <EmptyState
        icon={Code}
        title="No adapters found"
        description="No adapters match your current filters. Try adjusting your search criteria or train a new adapter."
      />
    );
  }

  return (
    <div className="rounded-md border">
      <Table>
        <TableHeader>
          <TableRow>
            {onSelectionChange && (
              <TableHead className="w-12">
                <Checkbox
                  checked={allSelected}
                  ref={(el) => {
                    if (el) {
                      (el as HTMLButtonElement & { indeterminate?: boolean }).indeterminate = someSelected;
                    }
                  }}
                  onCheckedChange={toggleSelectAll}
                  aria-label="Select all adapters"
                />
              </TableHead>
            )}
            <TableHead className="min-w-[200px]">Name</TableHead>
            <TableHead className="w-[100px]">Status</TableHead>
            <TableHead className="w-[80px]">Tier</TableHead>
            <TableHead className="w-[100px]">Activation %</TableHead>
            <TableHead className="w-[100px]">Memory</TableHead>
            <TableHead className="w-[100px]">Category</TableHead>
            <TableHead className="w-[60px] text-right">Actions</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {adapters.map(adapter => (
            <AdapterTableRow
              key={adapter.adapter_id}
              adapter={adapter}
              isSelected={selectedAdapters.includes(adapter.adapter_id)}
              onSelect={onSelectionChange ? () => toggleSelectOne(adapter.adapter_id) : undefined}
              onLoad={onLoad}
              onUnload={onUnload}
              onDelete={onDelete}
              onPin={onPin}
              onPromote={onPromote}
              onEvict={onEvict}
              onViewHealth={onViewHealth}
              onDownloadManifest={onDownloadManifest}
              canLoad={canLoad}
              canUnload={canUnload}
              canDelete={canDelete}
              totalMemory={totalMemory}
            />
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

interface AdapterTableRowProps {
  adapter: Adapter;
  isSelected?: boolean;
  onSelect?: () => void;
  onLoad?: (adapterId: string) => void;
  onUnload?: (adapterId: string) => void;
  onDelete?: (adapterId: string) => void;
  onPin?: (adapterId: string, pinned: boolean) => void;
  onPromote?: (adapterId: string) => void;
  onEvict?: (adapterId: string) => void;
  onViewHealth?: (adapterId: string) => void;
  onDownloadManifest?: (adapterId: string) => void;
  canLoad?: boolean;
  canUnload?: boolean;
  canDelete?: boolean;
  totalMemory?: number;
}

function AdapterTableRow({
  adapter,
  isSelected = false,
  onSelect,
  onLoad,
  onUnload,
  onDelete,
  onPin,
  onPromote,
  onEvict,
  onViewHealth,
  onDownloadManifest,
  canLoad = true,
  canUnload = true,
  canDelete = true,
  totalMemory = 0,
}: AdapterTableRowProps) {
  const memoryMB = adapter.memory_bytes / (1024 * 1024);
  const memoryPercent = totalMemory > 0 ? (adapter.memory_bytes / totalMemory) * 100 : 0;

  // Calculate activation percentage (placeholder - would come from metrics)
  const activationPercent = Math.min(100, (adapter.activation_count / 100) * 100);

  return (
    <TableRow className={isSelected ? 'bg-accent/50' : undefined}>
      {onSelect && (
        <TableCell>
          <Checkbox
            checked={isSelected}
            onCheckedChange={onSelect}
            aria-label={`Select ${adapter.name}`}
          />
        </TableCell>
      )}
      <TableCell>
        <div className="flex items-center gap-2">
          {getCategoryIcon(adapter.category)}
          <div>
            <div className="font-medium flex items-center gap-1">
              {adapter.name}
              {adapter.pinned && (
                <Pin className="h-3 w-3 text-muted-foreground" />
              )}
            </div>
            <div className="text-xs text-muted-foreground">
              {adapter.adapter_id}
              {adapter.framework && ` - ${adapter.framework}`}
              {adapter.version && ` v${adapter.version}`}
            </div>
          </div>
        </div>
      </TableCell>
      <TableCell>
        <div className="flex items-center gap-2">
          {getStateIcon(adapter.current_state)}
          <Badge variant={getStateBadgeVariant(adapter.current_state)}>
            {adapter.current_state}
          </Badge>
        </div>
      </TableCell>
      <TableCell>
        <Badge variant="outline">
          Tier {adapter.tier || 1}
        </Badge>
      </TableCell>
      <TableCell>
        <div className="flex flex-col gap-1">
          <div className="flex items-center justify-between text-xs">
            <span>{adapter.activation_count}</span>
            <span className="text-muted-foreground">{activationPercent.toFixed(0)}%</span>
          </div>
          <Progress value={activationPercent} className="h-1" />
        </div>
      </TableCell>
      <TableCell>
        <div className="flex items-center gap-1 text-sm">
          <MemoryStick className="h-3 w-3 text-muted-foreground" />
          <span>{memoryMB.toFixed(1)} MB</span>
        </div>
        {totalMemory > 0 && (
          <div className="text-xs text-muted-foreground">
            {memoryPercent.toFixed(1)}% of total
          </div>
        )}
      </TableCell>
      <TableCell>
        <Badge variant="secondary" className="flex items-center gap-1 w-fit">
          {getCategoryIcon(adapter.category, 'h-3 w-3')}
          {adapter.category}
        </Badge>
      </TableCell>
      <TableCell className="text-right">
        <AdapterActions
          adapter={adapter}
          onLoad={onLoad}
          onUnload={onUnload}
          onDelete={onDelete}
          onPin={onPin}
          onPromote={onPromote}
          onEvict={onEvict}
          onViewHealth={onViewHealth}
          onDownloadManifest={onDownloadManifest}
          canLoad={canLoad}
          canUnload={canUnload}
          canDelete={canDelete}
        />
      </TableCell>
    </TableRow>
  );
}

function AdapterTableSkeleton() {
  return (
    <div className="rounded-md border">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead className="w-12">
              <Skeleton className="h-4 w-4" />
            </TableHead>
            <TableHead className="min-w-[200px]">Name</TableHead>
            <TableHead className="w-[100px]">Status</TableHead>
            <TableHead className="w-[80px]">Tier</TableHead>
            <TableHead className="w-[100px]">Activation %</TableHead>
            <TableHead className="w-[100px]">Memory</TableHead>
            <TableHead className="w-[100px]">Category</TableHead>
            <TableHead className="w-[60px]">Actions</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {Array.from({ length: 5 }).map((_, i) => (
            <TableRow key={i}>
              <TableCell>
                <Skeleton className="h-4 w-4" />
              </TableCell>
              <TableCell>
                <div className="flex items-center gap-2">
                  <Skeleton className="h-4 w-4 rounded" />
                  <div className="space-y-1">
                    <Skeleton className="h-4 w-32" />
                    <Skeleton className="h-3 w-24" />
                  </div>
                </div>
              </TableCell>
              <TableCell>
                <Skeleton className="h-5 w-16" />
              </TableCell>
              <TableCell>
                <Skeleton className="h-5 w-12" />
              </TableCell>
              <TableCell>
                <Skeleton className="h-4 w-full" />
              </TableCell>
              <TableCell>
                <Skeleton className="h-4 w-16" />
              </TableCell>
              <TableCell>
                <Skeleton className="h-5 w-20" />
              </TableCell>
              <TableCell>
                <Skeleton className="h-8 w-8" />
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

function getCategoryIcon(category: string, className = 'h-4 w-4') {
  switch (category) {
    case 'code':
      return <Code className={`${className} text-blue-500`} />;
    case 'framework':
      return <Layers className={`${className} text-green-500`} />;
    case 'codebase':
      return <GitBranch className={`${className} text-purple-500`} />;
    case 'ephemeral':
      return <Clock className={`${className} text-orange-500`} />;
    default:
      return <Code className={className} />;
  }
}

function getStateIcon(state: AdapterState) {
  switch (state) {
    case 'unloaded':
      return <Square className="h-3 w-3 text-gray-400" />;
    case 'cold':
      return <Snowflake className="h-3 w-3 text-blue-400" />;
    case 'warm':
      return <Thermometer className="h-3 w-3 text-yellow-500" />;
    case 'hot':
      return <Flame className="h-3 w-3 text-orange-500" />;
    case 'resident':
      return <Anchor className="h-3 w-3 text-green-500" />;
    default:
      return null;
  }
}

function getStateBadgeVariant(state: AdapterState): 'default' | 'secondary' | 'outline' | 'destructive' {
  switch (state) {
    case 'resident':
      return 'default';
    case 'hot':
      return 'default';
    case 'warm':
      return 'secondary';
    case 'cold':
      return 'outline';
    case 'unloaded':
      return 'outline';
    default:
      return 'secondary';
  }
}

export default AdapterTable;
