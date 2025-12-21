import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type Dispatch,
  type SetStateAction,
} from 'react';
import type { Adapter } from '@/api/types';

export type AdapterSortColumn = 'name' | 'state' | 'memory' | 'activations' | 'created_at';
export type AdapterSortDirection = 'asc' | 'desc';

export interface AdapterSortState {
  column: AdapterSortColumn;
  direction: AdapterSortDirection;
}

export interface AdapterFilters {
  state?: string;
  category?: string;
  pinnedOnly?: boolean;
}

export interface UseAdapterFilterStateReturn {
  search: string;
  filters: AdapterFilters;
  sort: AdapterSortState;
  setSearch: Dispatch<SetStateAction<string>>;
  updateFilters: (updates: Partial<AdapterFilters>) => void;
  setSort: Dispatch<SetStateAction<AdapterSortState>>;
  resetFilters: () => void;
  applyFiltersAndSort: (adapters: Adapter[]) => Adapter[];
}

interface UseAdapterFilterStateOptions {
  tenantId?: string;
  userId?: string;
}

interface PersistedState {
  search: string;
  filters: AdapterFilters;
  sort: AdapterSortState;
}

const DEFAULT_FILTERS: AdapterFilters = {
  state: undefined,
  category: undefined,
  pinnedOnly: false,
};

const DEFAULT_SORT: AdapterSortState = {
  column: 'name',
  direction: 'asc',
};

const DEFAULT_SEARCH = '';

const STORAGE_PREFIX = 'adapteros:adapter-filter-state';

const getStorageKey = (tenantId?: string, userId?: string) =>
  `${STORAGE_PREFIX}:${tenantId ?? 'tenant-none'}:${userId ?? 'user-none'}`;

const isValidSort = (value: unknown): value is AdapterSortState => {
  if (!value || typeof value !== 'object') return false;
  const sort = value as AdapterSortState;
  const validColumn: AdapterSortColumn[] = ['name', 'state', 'memory', 'activations', 'created_at'];
  const validDirection: AdapterSortDirection[] = ['asc', 'desc'];
  return validColumn.includes(sort.column) && validDirection.includes(sort.direction);
};

const loadPersistedState = (storageKey: string): PersistedState => {
  if (typeof window === 'undefined') {
    return {
      search: DEFAULT_SEARCH,
      filters: { ...DEFAULT_FILTERS },
      sort: { ...DEFAULT_SORT },
    };
  }

  try {
    const raw = window.localStorage.getItem(storageKey);
    if (!raw) {
      return {
        search: DEFAULT_SEARCH,
        filters: { ...DEFAULT_FILTERS },
        sort: { ...DEFAULT_SORT },
      };
    }
    const parsed = JSON.parse(raw);
    return {
      search: typeof parsed.search === 'string' ? parsed.search : DEFAULT_SEARCH,
      filters: { ...DEFAULT_FILTERS, ...(parsed.filters ?? {}) },
      sort: isValidSort(parsed.sort) ? parsed.sort : { ...DEFAULT_SORT },
    };
  } catch {
    return {
      search: DEFAULT_SEARCH,
      filters: { ...DEFAULT_FILTERS },
      sort: { ...DEFAULT_SORT },
    };
  }
};

const getComparableName = (adapter: Adapter) =>
  adapter.name ||
  adapter.adapter_name ||
  adapter.adapter_id ||
  adapter.id ||
  '';

const getRuntimeState = (adapter: Adapter) =>
  (adapter.current_state ||
    adapter.runtime_state ||
    adapter.state ||
    adapter.lifecycle_state ||
    '').toLowerCase();

const getStateRank = (state: string) => {
  const order = ['resident', 'active', 'hot', 'warm', 'cold', 'loading', 'unloaded'];
  const index = order.indexOf(state);
  return index === -1 ? order.length : index;
};

export function useAdapterFilterState({
  tenantId,
  userId,
}: UseAdapterFilterStateOptions): UseAdapterFilterStateReturn {
  const storageKey = useMemo(() => getStorageKey(tenantId, userId), [tenantId, userId]);
  const initialState = useMemo(() => loadPersistedState(storageKey), [storageKey]);

  const [search, setSearch] = useState<string>(initialState.search);
  const [filters, setFilters] = useState<AdapterFilters>(initialState.filters);
  const [sort, setSort] = useState<AdapterSortState>(initialState.sort);

  useEffect(() => {
    const next = loadPersistedState(storageKey);
    setSearch(next.search);
    setFilters(next.filters);
    setSort(next.sort);
  }, [storageKey]);

  useEffect(() => {
    if (typeof window === 'undefined') return;
    const payload = JSON.stringify({ search, filters, sort });
    window.localStorage.setItem(storageKey, payload);
  }, [storageKey, search, filters, sort]);

  const updateFilters = useCallback((updates: Partial<AdapterFilters>) => {
    setFilters((prev) => ({ ...prev, ...updates }));
  }, []);

  const resetFilters = useCallback(() => {
    setSearch(DEFAULT_SEARCH);
    setFilters({ ...DEFAULT_FILTERS });
    setSort({ ...DEFAULT_SORT });
  }, []);

  const applyFiltersAndSort = useCallback(
    (adapters: Adapter[]) => {
      const searchTerm = search.trim().toLowerCase();

      const filtered = adapters.filter((adapter) => {
        const runtimeState = getRuntimeState(adapter);
        const lifecycleState = (adapter.lifecycle_state || '').toLowerCase();

        if (searchTerm) {
          const haystack = [
            adapter.name,
            adapter.adapter_name,
            adapter.adapter_id,
            adapter.id,
          ]
            .filter(Boolean)
            .map((value) => String(value).toLowerCase());

          if (!haystack.some((value) => value.includes(searchTerm))) {
            return false;
          }
        }

        if (filters.category && adapter.category !== filters.category) {
          return false;
        }

        if (filters.pinnedOnly && !adapter.pinned) {
          return false;
        }

        if (filters.state) {
          const target = filters.state.toLowerCase();
          const matchesState =
            (target === 'active' &&
              (runtimeState === 'resident' || runtimeState === 'active' || lifecycleState === 'active')) ||
            (target === 'loading' && runtimeState === 'loading') ||
            (target === 'unloaded' && (runtimeState === 'unloaded' || runtimeState === 'cold')) ||
            target === lifecycleState ||
            (target === runtimeState);

          if (!matchesState) {
            return false;
          }
        }

        return true;
      });

      const direction = sort.direction === 'asc' ? 1 : -1;

      return [...filtered].sort((a, b) => {
        let result = 0;

        switch (sort.column) {
          case 'state': {
            result = getStateRank(getRuntimeState(a)) - getStateRank(getRuntimeState(b));
            break;
          }
          case 'memory': {
            result = (a.memory_bytes ?? 0) - (b.memory_bytes ?? 0);
            break;
          }
          case 'activations': {
            result = (a.activation_count ?? 0) - (b.activation_count ?? 0);
            break;
          }
          case 'created_at': {
            const timeA = a.created_at ? new Date(a.created_at).getTime() : 0;
            const timeB = b.created_at ? new Date(b.created_at).getTime() : 0;
            result = timeA - timeB;
            break;
          }
          case 'name':
          default: {
            result = getComparableName(a).localeCompare(getComparableName(b));
            break;
          }
        }

        if (result === 0) {
          result = getComparableName(a).localeCompare(getComparableName(b));
        }

        return result * direction;
      });
    },
    [filters.category, filters.pinnedOnly, filters.state, search, sort.column, sort.direction],
  );

  return {
    search,
    filters,
    sort,
    setSearch,
    updateFilters,
    setSort,
    resetFilters,
    applyFiltersAndSort,
  };
}

