import React from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  GripVertical,
  Trash2,
  Eye,
  Code,
  Layers,
  GitBranch,
  Clock,
} from 'lucide-react';
import { cn } from '@/components/ui/utils';
import { Adapter } from '@/api/types';
import { LIFECYCLE_STATE_LABELS } from '@/constants/terminology';

interface StackAdapter {
  adapter: Adapter;
  order: number;
  enabled: boolean;
}

interface SortableAdapterItemProps {
  item: StackAdapter;
  onRemove: () => void;
  onToggle: () => void;
}

const getCategoryIcon = (category: string) => {
  switch (category) {
    case 'code':
      return <Code className="h-4 w-4" />;
    case 'framework':
      return <Layers className="h-4 w-4" />;
    case 'codebase':
      return <GitBranch className="h-4 w-4" />;
    case 'ephemeral':
      return <Clock className="h-4 w-4" />;
    default:
      return <Code className="h-4 w-4" />;
  }
};

const getStateColor = (state: string) => {
  switch (state) {
    case 'unloaded':
      return 'bg-muted text-muted-foreground';
    case 'cold':
      return 'bg-info/10 text-info';
    case 'warm':
      return 'bg-warning/10 text-warning';
    case 'hot':
      return 'bg-warning text-warning-foreground';
    case 'resident':
      return 'bg-success/10 text-success';
    default:
      return 'bg-muted text-muted-foreground';
  }
};

const getLifecycleColor = (state: string) => {
  switch (state) {
    case 'draft':
      return 'bg-muted text-muted-foreground';
    case 'active':
      return 'bg-success/10 text-success';
    case 'deprecated':
      return 'bg-warning/10 text-warning';
    case 'retired':
      return 'bg-destructive/10 text-destructive';
    default:
      return 'bg-muted text-muted-foreground';
  }
};

export const SortableAdapterItem: React.FC<SortableAdapterItemProps> = ({
  item,
  onRemove,
  onToggle,
}) => {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({
      id: item.adapter.adapter_id,
    });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  // Create instructions for screen readers
  const dragInstructions = `Press space to start dragging ${item.adapter.name}. While dragging, use the arrow keys to move the item. Press space again to drop, or press escape to cancel.`;

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={cn(
        'border rounded-lg p-4 transition-all',
        isDragging && 'opacity-50 bg-muted',
        !item.enabled && 'opacity-60 bg-muted/50'
      )}
    >
      <div className="flex items-start gap-3">
        {/* Drag Handle */}
        <button
          {...attributes}
          {...listeners}
          type="button"
          aria-label={`Drag to reorder ${item.adapter.name}`}
          aria-describedby={`drag-instructions-${item.adapter.adapter_id}`}
          className="cursor-grab active:cursor-grabbing pt-1 text-muted-foreground hover:text-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 rounded"
          tabIndex={0}
        >
          <GripVertical className="h-5 w-5" />
        </button>
        {/* Hidden instructions for screen readers */}
        <span id={`drag-instructions-${item.adapter.adapter_id}`} className="sr-only">
          {dragInstructions}
        </span>

        {/* Order Number */}
        <div className="flex items-center justify-center w-8 h-8 rounded-full bg-muted text-muted-foreground font-medium text-sm flex-shrink-0">
          {item.order + 1}
        </div>

        {/* Adapter Info */}
        <div className="flex-1 min-w-0">
          <div className="flex items-start justify-between gap-2 mb-2">
            <div>
              <p className="font-medium text-sm leading-tight">
                {item.adapter.name}
              </p>
              <p className="text-xs text-muted-foreground mt-1">
                {item.adapter.adapter_id}
              </p>
            </div>
            {!item.enabled && (
              <Badge variant="secondary" className="flex-shrink-0">
                Disabled
              </Badge>
            )}
          </div>

          {/* Metadata Badges */}
          <div className="flex flex-wrap gap-2 mt-2">
            {/* Category */}
            <Badge
              variant="outline"
              className="flex items-center gap-1"
              title={item.adapter.category}
            >
              {getCategoryIcon(item.adapter.category)}
              <span className="hidden sm:inline text-xs">
                {item.adapter.category}
              </span>
            </Badge>

            {/* State */}
            <Badge
              className={cn('text-xs', getStateColor(item.adapter.current_state || 'unknown'))}
              title={item.adapter.current_state}
            >
              {LIFECYCLE_STATE_LABELS[item.adapter.current_state || 'unknown'] || item.adapter.current_state || 'unknown'}
            </Badge>

            {/* Lifecycle */}
            <Badge
              className={cn('text-xs', getLifecycleColor(item.adapter.lifecycle_state || 'unknown'))}
              title={item.adapter.lifecycle_state}
            >
              {item.adapter.lifecycle_state || 'unknown'}
            </Badge>

            {/* Rank & Tier */}
            <Badge variant="outline" className="text-xs">
              Rank: {item.adapter.rank}
            </Badge>

            <Badge variant="outline" className="text-xs">
              Tier: {item.adapter.tier}
            </Badge>

            {/* Memory */}
            {item.adapter.memory_bytes > 0 && (
              <Badge variant="outline" className="text-xs">
                {(item.adapter.memory_bytes / 1024 / 1024).toFixed(1)}MB
              </Badge>
            )}

            {/* Framework */}
            {item.adapter.framework && (
              <Badge variant="outline" className="text-xs">
                {item.adapter.framework}
              </Badge>
            )}

            {/* Activation Count */}
            {item.adapter.activation_count > 0 && (
              <Badge variant="outline" className="text-xs">
                {item.adapter.activation_count} activations
              </Badge>
            )}
          </div>

          {/* Description or Additional Info */}
          {item.adapter.intent && (
            <p className="text-xs text-muted-foreground mt-2 italic">
              {item.adapter.intent}
            </p>
          )}
        </div>

        {/* Action Buttons */}
        <div className="flex gap-1 flex-shrink-0">
          <Button
            variant="ghost"
            size="sm"
            onClick={onToggle}
            title={item.enabled ? 'Disable adapter' : 'Enable adapter'}
            className="h-8 w-8 p-0"
          >
            <Eye className={cn('h-4 w-4', !item.enabled && 'opacity-40')} />
          </Button>

          <Button
            variant="ghost"
            size="sm"
            onClick={onRemove}
            className="h-8 w-8 p-0 text-destructive hover:text-destructive hover:bg-destructive/10"
            title="Remove adapter from stack"
          >
            <Trash2 className="h-4 w-4" />
          </Button>
        </div>
      </div>
    </div>
  );
};

export default SortableAdapterItem;
