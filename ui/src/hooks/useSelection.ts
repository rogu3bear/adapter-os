/**
 * Selection State Hook
 *
 * Manages multi-select state for lists and tables.
 * Supports single selection, multi-selection, and range selection.
 *
 * Usage:
 * ```tsx
 * const { selectedIds, toggleSelection, selectAll, clearSelection, isSelected } = useSelection({
 *   items: adapters,
 *   getItemId: (adapter) => adapter.id,
 * });
 *
 * // In your table row
 * <Checkbox
 *   checked={isSelected(adapter.id)}
 *   onChange={() => toggleSelection(adapter.id)}
 * />
 * ```
 *
 * Citations:
 * - docs/UI_INTEGRATION.md - Selection patterns
 */

import { useState, useCallback, useMemo } from 'react';

export type SelectionMode = 'single' | 'multiple' | 'range';

export interface UseSelectionOptions<T, K extends string | number = string> {
  /** Items available for selection */
  items: T[];
  /** Function to extract unique ID from item */
  getItemId: (item: T) => K;
  /** Selection mode (default: 'multiple') */
  mode?: SelectionMode;
  /** Initially selected IDs */
  initialSelection?: K[];
  /** Maximum number of items that can be selected (default: unlimited) */
  maxSelection?: number;
  /** Callback when selection changes */
  onSelectionChange?: (selectedIds: K[], selectedItems: T[]) => void;
  /** Items that cannot be selected */
  disabledIds?: K[];
}

export interface UseSelectionReturn<T, K extends string | number = string> {
  /** Set of selected item IDs */
  selectedIds: Set<K>;
  /** Array of selected item IDs */
  selectedIdsArray: K[];
  /** Array of selected items */
  selectedItems: T[];
  /** Number of selected items */
  selectedCount: number;
  /** Check if an item is selected */
  isSelected: (id: K) => boolean;
  /** Check if all items are selected */
  isAllSelected: boolean;
  /** Check if some but not all items are selected */
  isPartiallySelected: boolean;
  /** Toggle selection of a single item */
  toggleSelection: (id: K, shiftKey?: boolean) => void;
  /** Select a single item (replaces current selection in single mode) */
  select: (id: K) => void;
  /** Deselect a single item */
  deselect: (id: K) => void;
  /** Select all items */
  selectAll: () => void;
  /** Clear all selections */
  clearSelection: () => void;
  /** Select multiple items by ID */
  selectMultiple: (ids: K[]) => void;
  /** Deselect multiple items by ID */
  deselectMultiple: (ids: K[]) => void;
  /** Toggle selection of all items */
  toggleSelectAll: () => void;
  /** Set selection directly */
  setSelection: (ids: K[]) => void;
  /** Check if selection has reached max limit */
  isAtMaxSelection: boolean;
  /** Check if an item is disabled */
  isDisabled: (id: K) => boolean;
}

/**
 * Hook for managing multi-select state.
 *
 * @param options - Selection configuration options
 * @returns Selection state and control functions
 */
export function useSelection<T, K extends string | number = string>(
  options: UseSelectionOptions<T, K>
): UseSelectionReturn<T, K> {
  const {
    items,
    getItemId,
    mode = 'multiple',
    initialSelection = [],
    maxSelection,
    onSelectionChange,
    disabledIds = [],
  } = options;

  const [selectedIds, setSelectedIds] = useState<Set<K>>(
    () => new Set(initialSelection)
  );

  // Track last selected index for range selection
  const [lastSelectedIndex, setLastSelectedIndex] = useState<number | null>(null);

  const disabledSet = useMemo(() => new Set(disabledIds), [disabledIds]);

  const selectableItems = useMemo(
    () => items.filter((item) => !disabledSet.has(getItemId(item))),
    [items, disabledSet, getItemId]
  );

  const selectedIdsArray = useMemo(() => Array.from(selectedIds), [selectedIds]);

  const selectedItems = useMemo(
    () => items.filter((item) => selectedIds.has(getItemId(item))),
    [items, selectedIds, getItemId]
  );

  const isAtMaxSelection = useMemo(
    () => maxSelection !== undefined && selectedIds.size >= maxSelection,
    [selectedIds, maxSelection]
  );

  const updateSelection = useCallback(
    (newIds: Set<K>) => {
      setSelectedIds(newIds);
      const newSelectedItems = items.filter((item) => newIds.has(getItemId(item)));
      onSelectionChange?.(Array.from(newIds), newSelectedItems);
    },
    [items, getItemId, onSelectionChange]
  );

  const isSelected = useCallback(
    (id: K): boolean => selectedIds.has(id),
    [selectedIds]
  );

  const isDisabled = useCallback(
    (id: K): boolean => disabledSet.has(id),
    [disabledSet]
  );

  const select = useCallback(
    (id: K) => {
      if (disabledSet.has(id)) return;

      if (mode === 'single') {
        updateSelection(new Set([id]));
      } else {
        if (maxSelection && selectedIds.size >= maxSelection) return;
        const newSelection = new Set(selectedIds);
        newSelection.add(id);
        updateSelection(newSelection);
      }

      const index = items.findIndex((item) => getItemId(item) === id);
      setLastSelectedIndex(index);
    },
    [mode, selectedIds, maxSelection, disabledSet, items, getItemId, updateSelection]
  );

  const deselect = useCallback(
    (id: K) => {
      const newSelection = new Set(selectedIds);
      newSelection.delete(id);
      updateSelection(newSelection);
    },
    [selectedIds, updateSelection]
  );

  const toggleSelection = useCallback(
    (id: K, shiftKey: boolean = false) => {
      if (disabledSet.has(id)) return;

      const currentIndex = items.findIndex((item) => getItemId(item) === id);

      // Range selection with shift key
      if (mode === 'range' || (mode === 'multiple' && shiftKey && lastSelectedIndex !== null)) {
        const start = Math.min(lastSelectedIndex, currentIndex);
        const end = Math.max(lastSelectedIndex, currentIndex);

        const newSelection = new Set(selectedIds);
        for (let i = start; i <= end; i++) {
          const itemId = getItemId(items[i]);
          if (!disabledSet.has(itemId)) {
            if (maxSelection && newSelection.size >= maxSelection) break;
            newSelection.add(itemId);
          }
        }

        updateSelection(newSelection);
        setLastSelectedIndex(currentIndex);
        return;
      }

      if (selectedIds.has(id)) {
        deselect(id);
      } else {
        select(id);
      }
    },
    [mode, items, getItemId, selectedIds, lastSelectedIndex, maxSelection, disabledSet, select, deselect, updateSelection]
  );

  const selectAll = useCallback(() => {
    const allIds = selectableItems.map(getItemId);
    const idsToSelect = maxSelection
      ? allIds.slice(0, maxSelection)
      : allIds;
    updateSelection(new Set(idsToSelect));
  }, [selectableItems, getItemId, maxSelection, updateSelection]);

  const clearSelection = useCallback(() => {
    updateSelection(new Set());
    setLastSelectedIndex(null);
  }, [updateSelection]);

  const selectMultiple = useCallback(
    (ids: K[]) => {
      const newSelection = new Set(selectedIds);
      for (const id of ids) {
        if (disabledSet.has(id)) continue;
        if (maxSelection && newSelection.size >= maxSelection) break;
        newSelection.add(id);
      }
      updateSelection(newSelection);
    },
    [selectedIds, disabledSet, maxSelection, updateSelection]
  );

  const deselectMultiple = useCallback(
    (ids: K[]) => {
      const newSelection = new Set(selectedIds);
      for (const id of ids) {
        newSelection.delete(id);
      }
      updateSelection(newSelection);
    },
    [selectedIds, updateSelection]
  );

  const toggleSelectAll = useCallback(() => {
    if (selectedIds.size === selectableItems.length) {
      clearSelection();
    } else {
      selectAll();
    }
  }, [selectedIds, selectableItems, clearSelection, selectAll]);

  const setSelection = useCallback(
    (ids: K[]) => {
      const validIds = ids.filter((id) => !disabledSet.has(id));
      const idsToSet = maxSelection
        ? validIds.slice(0, maxSelection)
        : validIds;
      updateSelection(new Set(idsToSet));
    },
    [disabledSet, maxSelection, updateSelection]
  );

  const isAllSelected = useMemo(
    () => selectableItems.length > 0 && selectedIds.size === selectableItems.length,
    [selectedIds, selectableItems]
  );

  const isPartiallySelected = useMemo(
    () => selectedIds.size > 0 && selectedIds.size < selectableItems.length,
    [selectedIds, selectableItems]
  );

  return {
    selectedIds,
    selectedIdsArray,
    selectedItems,
    selectedCount: selectedIds.size,
    isSelected,
    isAllSelected,
    isPartiallySelected,
    toggleSelection,
    select,
    deselect,
    selectAll,
    clearSelection,
    selectMultiple,
    deselectMultiple,
    toggleSelectAll,
    setSelection,
    isAtMaxSelection,
    isDisabled,
  };
}

export default useSelection;
