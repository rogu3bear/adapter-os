// Sortable adapter item for stack form
// 【2025-01-20†rectification†stack_sortable_item】

import React from 'react';
import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { GripVertical, X } from 'lucide-react';
import { cn } from '@/lib/utils';
import type { UseFormRegister, FieldValues } from 'react-hook-form';

interface StackSortableAdapterItemProps<T extends FieldValues> {
  id: string;
  adapterId: string;
  gate: number;
  availableAdapters?: Array<{ id: string; name?: string }>;
  onAdapterChange: (adapterId: string) => void;
  onGateChange: (gate: number) => void;
  onRemove: () => void;
  register: UseFormRegister<T>;
  index: number;
}

export function StackSortableAdapterItem<T extends FieldValues = FieldValues>({
  id,
  adapterId,
  gate,
  availableAdapters,
  onAdapterChange,
  onGateChange,
  onRemove,
  register,
  index,
}: StackSortableAdapterItemProps<T>) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={cn(
        'flex gap-2 items-start p-2 border rounded-lg bg-background',
        isDragging && 'opacity-50 bg-muted'
      )}
    >
      {/* Drag Handle */}
      <div
        {...attributes}
        {...listeners}
        className="cursor-grab active:cursor-grabbing pt-2 text-muted-foreground hover:text-foreground"
        role="button"
        aria-label={`Drag to reorder adapter ${index + 1}`}
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            // Keyboard reordering could be added here
          }
        }}
      >
        <GripVertical className="h-5 w-5" aria-hidden="true" />
      </div>

      {/* Adapter Select */}
      <div className="flex-1">
        <Select
          value={adapterId}
          onValueChange={onAdapterChange}
          aria-label={`Select adapter for position ${index + 1}`}
        >
          <SelectTrigger>
            <SelectValue placeholder="Select adapter" />
          </SelectTrigger>
          <SelectContent>
            {availableAdapters?.map((adapter) => (
              <SelectItem key={adapter.id} value={adapter.id}>
                {adapter.name || adapter.id}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {/* Gate Input */}
      <div className="w-32">
        <Input
          type="number"
          placeholder="Gate (Q15)"
          {...register(`adapters.${index}.gate` as any, {
            valueAsNumber: true,
            min: { value: 0, message: 'Gate must be >= 0' },
            max: { value: 32767, message: 'Gate must be <= 32767' },
          })}
          onChange={(e) => {
            const value = parseInt(e.target.value, 10);
            if (!isNaN(value)) {
              onGateChange(value);
            }
          }}
          aria-label={`Gate value (Q15) for adapter ${index + 1}`}
          aria-describedby={`gate-hint-${index}`}
        />
        <span id={`gate-hint-${index}`} className="sr-only">
          Q15 quantized gate value from 0 to 32767. Higher values give the adapter more weight.
        </span>
      </div>

      {/* Remove Button */}
      <Button
        type="button"
        variant="ghost"
        size="sm"
        onClick={onRemove}
        className="mt-0"
        aria-label={`Remove adapter ${index + 1}`}
      >
        <X className="h-4 w-4" aria-hidden="true" />
      </Button>
    </div>
  );
}

