import React from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { Telemetry } from '@/components/Telemetry';
import { apiClient } from '@/api/services';

// Mock heavy UI children to keep render minimal
function passthrough(tag: string) {
  return ({ children }: any) => <div data-testid={tag}>{children}</div>;
}

vi.mock('lucide-react', () => {
  const Icon = () => <span />;
  return {
    Activity: Icon,
    Download: Icon,
    Eye: Icon,
    MoreHorizontal: Icon,
    Pause: Icon,
    Play: Icon,
    RefreshCw: Icon,
    Shield: Icon,
    Trash2: Icon,
  };
});

vi.mock('@/components/ui/card', () => ({
  Card: passthrough('card'),
  CardContent: passthrough('card-content'),
  CardHeader: passthrough('card-header'),
  CardTitle: passthrough('card-title'),
}));

vi.mock('@/components/ui/table', () => ({
  Table: passthrough('table'),
  TableBody: passthrough('table-body'),
  TableCell: passthrough('table-cell'),
  TableHead: passthrough('table-head'),
  TableHeader: passthrough('table-header'),
  TableRow: passthrough('table-row'),
}));

vi.mock('@/components/ui/virtualized-table', () => ({
  VirtualizedTableRows: ({ items, children }: any) => <>{items.map((item: any) => children(item))}</>,
}));

vi.mock('@/components/ui/dialog', () => ({
  Dialog: passthrough('dialog'),
  DialogContent: passthrough('dialog-content'),
  DialogHeader: passthrough('dialog-header'),
  DialogTitle: passthrough('dialog-title'),
  DialogFooter: passthrough('dialog-footer'),
}));

vi.mock('@/components/ui/dropdown-menu', () => ({
  DropdownMenu: passthrough('dropdown-menu'),
  DropdownMenuContent: passthrough('dropdown-menu-content'),
  DropdownMenuItem: passthrough('dropdown-menu-item'),
  DropdownMenuTrigger: passthrough('dropdown-menu-trigger'),
}));

vi.mock('@/components/ui/accordion', () => ({
  Accordion: passthrough('accordion'),
  AccordionContent: passthrough('accordion-content'),
  AccordionItem: passthrough('accordion-item'),
  AccordionTrigger: passthrough('accordion-trigger'),
}));

vi.mock('@/components/ui/tooltip', () => ({
  Tooltip: passthrough('tooltip'),
  TooltipContent: passthrough('tooltip-content'),
  TooltipProvider: passthrough('tooltip-provider'),
  TooltipTrigger: passthrough('tooltip-trigger'),
}));

vi.mock('@/components/ui/scroll-area', () => ({
  ScrollArea: passthrough('scroll-area'),
}));

vi.mock('@/components/ui/export-menu', () => ({
  ExportMenu: passthrough('export-menu'),
}));

vi.mock('@/components/ui/advanced-filter', () => ({
  AdvancedFilter: passthrough('advanced-filter'),
}));

vi.mock('@/components/ui/density-controls', () => ({
  DensityControls: passthrough('density-controls'),
}));

vi.mock('@/components/ui/bulk-action-bar', () => ({
  BulkActionBar: passthrough('bulk-action-bar'),
}));

vi.mock('@/components/ui/glossary-tooltip', () => ({
  GlossaryTooltip: passthrough('glossary-tooltip'),
}));

vi.mock('@/components/ui/empty-state', () => ({
  EmptyState: passthrough('empty-state'),
}));

vi.mock('@/components/ui/loading-state', () => ({
  LoadingState: passthrough('loading-state'),
}));

vi.mock('@/components/ui/alert', () => ({
  Alert: passthrough('alert'),
  AlertDescription: passthrough('alert-description'),
}));

vi.mock('@/components/ui/badge', () => ({
  Badge: passthrough('badge'),
}));

vi.mock('@/components/ui/button', () => ({
  Button: ({ children, onClick }: any) => <button onClick={onClick} data-testid="button">{children}</button>,
}));

vi.mock('@/components/ui/checkbox', () => ({
  Checkbox: ({ onCheckedChange, ...rest }: any) => (
    <input
      type="checkbox"
      data-testid="checkbox"
      aria-label="checkbox"
      onChange={(e) => onCheckedChange?.((e.target as HTMLInputElement).checked)}
      {...rest}
    />
  ),
}));

vi.mock('@/components/ui/input', () => ({
  Input: (props: any) => <input data-testid="input" {...props} />,
}));

vi.mock('@/components/ui/select', () => ({
  Select: ({ children, value }: any) => <div data-testid="select" data-value={value}>{children}</div>,
  SelectContent: passthrough('select-content'),
  SelectItem: ({ children, value }: any) => <div data-testid="select-item" data-value={value}>{children}</div>,
  SelectTrigger: passthrough('select-trigger'),
  SelectValue: ({ placeholder, children }: any) => (
    <div data-testid="select-value" placeholder={placeholder}>
      {children}
    </div>
  ),
}));

vi.mock('@/components/ui/switch', () => ({
  Switch: ({ checked }: any) => <div data-testid="switch">{String(checked)}</div>,
}));

vi.mock('@/components/HashChainView', () => ({
  HashChainView: passthrough('hash-chain-view'),
}));

vi.mock('@/components/GoldenCompareModal', () => ({
  GoldenCompareModal: passthrough('golden-compare-modal'),
}));

// Mock live data hook
const mockUseLiveData = vi.fn();
vi.mock('@/hooks/realtime/useLiveData', () => ({
  ConnectionStatus: {},
  useLiveData: (options: any) => mockUseLiveData(options),
}));

// Mock apiClient
vi.mock('@/api/client', () => ({
  default: {
    getTelemetryEvents: vi.fn(),
    listTelemetryBundles: vi.fn(),
    exportTelemetryBundle: vi.fn(),
    getTelemetryBundle: vi.fn(),
    verifyBundleSignature: vi.fn(),
    deleteTelemetryBundle: vi.fn(),
    purgeTelemetryBundles: vi.fn(),
  },
}));

// Mock provider hooks
vi.mock('@/providers/CoreProviders', () => ({
  useAuth: () => ({
    user: {
      user_id: 'test-user',
      email: 'test@example.com',
      role: 'admin',
    },
  }),
}));

vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: () => ({
    selectedTenant: 'default',
  }),
}));

// Mock useDensity hook
vi.mock('@/contexts/DensityContext', () => ({
  useDensity: () => ({
    density: 'normal',
    setDensity: vi.fn(),
  }),
}));

describe('Telemetry', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    mockUseLiveData.mockImplementation((options: any) => {
      if (options?.operationName === 'TelemetryEvents') {
        return {
          data: [],
          isLoading: false,
          error: null,
          sseConnected: true,
          connectionStatus: 'sse',
          lastUpdated: null,
          freshnessLevel: 'fresh',
          refetch: vi.fn(),
          reconnect: vi.fn(),
          toggleSSE: vi.fn(),
        };
      }
      if (options?.operationName === 'TelemetryBundles') {
        return {
          data: [],
          isLoading: false,
          error: null,
          sseConnected: true,
          connectionStatus: 'sse',
          lastUpdated: null,
          freshnessLevel: 'fresh',
          refetch: vi.fn(),
          reconnect: vi.fn(),
          toggleSSE: vi.fn(),
        };
      }
      return {
        data: [],
        isLoading: false,
        error: null,
        sseConnected: false,
        connectionStatus: 'idle',
        lastUpdated: null,
        freshnessLevel: 'fresh',
        refetch: vi.fn(),
        reconnect: vi.fn(),
        toggleSSE: vi.fn(),
      };
    });

    (apiClient.getTelemetryEvents as any).mockResolvedValue([]);
    (apiClient.listTelemetryBundles as any).mockResolvedValue([]);
    (apiClient.exportTelemetryBundle as any).mockResolvedValue({});
  });

  it('wires live telemetry events stream through useLiveData', () => {
    render(<Telemetry />);

    const call = mockUseLiveData.mock.calls.find(
      (c) => c[0]?.sseEndpoint === '/v1/stream/telemetry' && c[0]?.operationName === 'TelemetryEvents',
    );
    expect(call).toBeDefined();
  });

  it('renders a bundle row from live data', async () => {
    const bundles = [
      {
        id: 'bundle-1',
        cpid: 'cp-1',
        event_count: 10,
        size_bytes: 1024,
        created_at: '2024-01-01T00:00:00Z',
        start_time: '2024-01-01T00:00:00Z',
        end_time: '2024-01-01T00:05:00Z',
        merkle_root: 'abc',
        tenant_id: 'default',
      },
    ];

    mockUseLiveData.mockImplementation((options: any) => {
      if (options?.operationName === 'TelemetryEvents') {
        return {
          data: [],
          isLoading: false,
          error: null,
          sseConnected: true,
          connectionStatus: 'sse',
          lastUpdated: null,
          freshnessLevel: 'fresh',
          refetch: vi.fn(),
          reconnect: vi.fn(),
          toggleSSE: vi.fn(),
        };
      }
      if (options?.operationName === 'TelemetryBundles') {
        setTimeout(() => {
          options?.onSSEMessage?.(bundles);
        }, 0);
        return {
          data: bundles,
          isLoading: false,
          error: null,
          sseConnected: true,
          connectionStatus: 'sse',
          lastUpdated: null,
          freshnessLevel: 'fresh',
          refetch: vi.fn(),
          reconnect: vi.fn(),
          toggleSSE: vi.fn(),
        };
      }
      return {
        data: [],
        isLoading: false,
        error: null,
        sseConnected: false,
        connectionStatus: 'idle',
        lastUpdated: null,
        freshnessLevel: 'fresh',
        refetch: vi.fn(),
        reconnect: vi.fn(),
        toggleSSE: vi.fn(),
      };
    });

    render(<Telemetry />);

    await waitFor(() => {
      expect(screen.getByText('bundle-1')).toBeInTheDocument();
    });
  });
});
