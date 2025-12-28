/**
 * ChatControlBar Component Tests
 *
 * Tests for the top control bar in the chat interface including:
 * - Stack selector with options
 * - Collection/Knowledge base selector
 * - Toggle buttons (history, router activity, debugger)
 * - Export button
 * - Adapter mount indicators
 * - Loading states
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ChatControlBar, type ChatControlBarProps } from '@/components/chat/ChatControlBar';
import type { AdapterStack } from '@/api/adapter-types';
import type { Collection } from '@/api/document-types';
import type { AdapterState } from '@/components/chat/AdapterLoadingStatus';
import type { AdapterMountItem, AdapterMountTransition } from '@/components/chat/AdapterMountIndicators';

// Simplify Select to avoid Radix pointer-capture issues in JSDOM
vi.mock('@/components/ui/select', () => {
  const React = require('react');

  // Helper to extract text content from React children
  const getTextContent = (children: React.ReactNode): string => {
    if (typeof children === 'string' || typeof children === 'number') {
      return String(children);
    }
    if (Array.isArray(children)) {
      return children.map(getTextContent).join('');
    }
    if (children && typeof children === 'object' && 'props' in children) {
      return getTextContent((children as React.ReactElement).props?.children);
    }
    return '';
  };

  const Select = ({
    value,
    onValueChange,
    children,
    'aria-label': ariaLabel,
    ...props
  }: {
    value?: string;
    onValueChange?: (value: string) => void;
    children?: React.ReactNode;
    'aria-label'?: string;
  }) => (
    <select
      value={value ?? ''}
      onChange={(e) => onValueChange?.((e.target as HTMLSelectElement).value)}
      aria-label={ariaLabel}
      data-testid={ariaLabel?.toLowerCase().replace(/\s+/g, '-')}
      {...props}
    >
      {children}
    </select>
  );

  return {
    Select,
    SelectTrigger: ({
      children,
    }: {
      children?: React.ReactNode;
      'aria-label'?: string;
      ref?: React.Ref<HTMLButtonElement>;
      className?: string;
    }) => <>{children}</>,
    SelectContent: ({ children }: { children?: React.ReactNode }) => <>{children}</>,
    SelectItem: ({
      value,
      children,
    }: {
      value: string;
      children?: React.ReactNode;
    }) => (
      <option value={value}>{getTextContent(children)}</option>
    ),
    SelectValue: ({ placeholder }: { placeholder?: string }) => (
      <option value="" hidden>
        {placeholder}
      </option>
    ),
  };
});

// Mock child components to isolate ChatControlBar testing
vi.mock('@/components/chat/AdapterLoadingStatus', () => ({
  AdapterLoadingStatus: ({ adapters, compact }: { adapters: AdapterState[]; compact?: boolean }) => (
    <div data-testid="adapter-loading-status" data-compact={compact}>
      {adapters.map((a) => (
        <span key={a.id} data-testid={`adapter-state-${a.id}`}>
          {a.name}: {a.state}
        </span>
      ))}
    </div>
  ),
}));

vi.mock('@/components/chat/AdapterMountIndicators', () => ({
  AdapterMountIndicators: ({
    adapters,
    activeAdapterId,
    isStreaming,
  }: {
    adapters: AdapterMountItem[];
    transitions: AdapterMountTransition[];
    activeAdapterId?: string | null;
    isStreaming?: boolean;
  }) => (
    <div data-testid="adapter-mount-indicators" data-streaming={isStreaming}>
      {adapters.map((a) => (
        <span
          key={a.adapterId}
          data-testid={`mount-indicator-${a.adapterId}`}
          data-active={activeAdapterId === a.adapterId}
        >
          {a.name}: {a.state}
        </span>
      ))}
    </div>
  ),
}));

vi.mock('@/components/chat/ChatTagsManager', () => ({
  ChatTagsManager: ({ sessionId }: { sessionId: string }) => (
    <div data-testid="chat-tags-manager" data-session-id={sessionId}>
      Tags Manager
    </div>
  ),
}));

// Mock logger
vi.mock('@/utils/logger', () => ({
  logger: {
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  },
}));

// ============================================================================
// Test Data Factories
// ============================================================================

function createMockStacks(count = 3): AdapterStack[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `stack-${i + 1}`,
    name: `Test Stack ${i + 1}`,
    adapter_ids: [`adapter-${i + 1}-a`, `adapter-${i + 1}-b`],
    description: `Description for stack ${i + 1}`,
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
    lifecycle_state: 'active',
    tenant_id: 'test-tenant',
    version: 1,
    is_active: true,
  }));
}

function createMockCollections(count = 2): Collection[] {
  return Array.from({ length: count }, (_, i) => ({
    collection_id: `collection-${i + 1}`,
    name: `Knowledge Base ${i + 1}`,
    document_count: (i + 1) * 5,
    created_at: '2025-01-01T00:00:00Z',
  }));
}

function createMockAdapterStates(): Map<string, AdapterState> {
  const states = new Map<string, AdapterState>();
  states.set('adapter-1', {
    id: 'adapter-1',
    name: 'Finance Adapter',
    state: 'hot',
    isLoading: false,
  });
  states.set('adapter-2', {
    id: 'adapter-2',
    name: 'Legal Adapter',
    state: 'warm',
    isLoading: false,
  });
  return states;
}

function createMockAdapterMountItems(): AdapterMountItem[] {
  return [
    { adapterId: 'adapter-1', name: 'Finance Adapter', state: 'hot', isLoading: false },
    { adapterId: 'adapter-2', name: 'Legal Adapter', state: 'warm', isLoading: false },
  ];
}

function createDefaultProps(overrides: Partial<ChatControlBarProps> = {}): ChatControlBarProps {
  return {
    stacks: createMockStacks(),
    selectedStackId: 'stack-1',
    onStackChange: vi.fn(),
    collections: createMockCollections(),
    selectedCollectionId: null,
    onCollectionChange: vi.fn(),
    adapterStates: new Map(),
    adapterMountItems: [],
    adapterTransitions: [],
    isHistoryOpen: false,
    onToggleHistory: vi.fn(),
    isRouterActivityOpen: false,
    onToggleRouterActivity: vi.fn(),
    isDebuggerOpen: false,
    onToggleDebugger: vi.fn(),
    messagesCount: 0,
    ...overrides,
  };
}

// ============================================================================
// Test Suites
// ============================================================================

describe('ChatControlBar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Stack Selector', () => {
    it('renders stack selector with options', () => {
      const stacks = createMockStacks(3);
      const props = createDefaultProps({ stacks, selectedStackId: 'stack-1' });

      render(<ChatControlBar {...props} />);

      // Check that the stack label is present
      expect(screen.getByText('Stack:')).toBeInTheDocument();

      // Check that the select is present (using native select due to mocking)
      const select = screen.getByTestId('select-adapter-stack');
      expect(select).toBeInTheDocument();
    });

    it('displays selected stack value', () => {
      const stacks = createMockStacks(3);
      const props = createDefaultProps({ stacks, selectedStackId: 'stack-2' });

      render(<ChatControlBar {...props} />);

      // The select should have the correct value
      const select = screen.getByTestId('select-adapter-stack') as HTMLSelectElement;
      expect(select.value).toBe('stack-2');
    });

    it('calls onStackChange when stack is selected', () => {
      const stacks = createMockStacks(3);
      const onStackChange = vi.fn();
      const props = createDefaultProps({ stacks, selectedStackId: 'stack-1', onStackChange });

      render(<ChatControlBar {...props} />);

      const select = screen.getByTestId('select-adapter-stack');
      fireEvent.change(select, { target: { value: 'stack-3' } });

      expect(onStackChange).toHaveBeenCalledWith('stack-3');
    });

    it('shows stack description in options when available', () => {
      const stacks = createMockStacks(2);
      const props = createDefaultProps({ stacks, selectedStackId: 'stack-1' });

      render(<ChatControlBar {...props} />);

      // With native select mock, descriptions are concatenated with name in options
      // The option text will be "Test Stack 1(Description for stack 1)"
      const select = screen.getByTestId('select-adapter-stack') as HTMLSelectElement;
      const options = Array.from(select.options);
      const stack1Option = options.find((opt) => opt.value === 'stack-1');
      expect(stack1Option?.textContent).toContain('Test Stack 1');
      expect(stack1Option?.textContent).toContain('Description for stack 1');
    });

    it('shows adapter count badge when selectedStack is provided', () => {
      const stacks = createMockStacks(2);
      const selectedStack = stacks[0];
      const props = createDefaultProps({ stacks, selectedStackId: 'stack-1', selectedStack });

      render(<ChatControlBar {...props} />);

      expect(screen.getByText('2 adapters')).toBeInTheDocument();
    });

    it('shows singular "adapter" when stack has one adapter', () => {
      const stacks = createMockStacks(1);
      stacks[0].adapter_ids = ['single-adapter'];
      const props = createDefaultProps({
        stacks,
        selectedStackId: 'stack-1',
        selectedStack: stacks[0],
      });

      render(<ChatControlBar {...props} />);

      expect(screen.getByText('1 adapter')).toBeInTheDocument();
    });

    it('shows "0 adapters" when stack has no adapters', () => {
      const stacks = createMockStacks(1);
      stacks[0].adapter_ids = [];
      const props = createDefaultProps({
        stacks,
        selectedStackId: 'stack-1',
        selectedStack: stacks[0],
      });

      render(<ChatControlBar {...props} />);

      expect(screen.getByText('0 adapters')).toBeInTheDocument();
    });

    it('shows hint when no stacks are available', () => {
      const props = createDefaultProps({ stacks: [], selectedStackId: '' });

      render(<ChatControlBar {...props} />);

      // The hint should be present (screen reader only)
      expect(screen.getByText('No adapter stacks available. Please create a stack first.')).toBeInTheDocument();
    });
  });

  describe('Collection Selector', () => {
    it('renders collection selector', () => {
      const collections = createMockCollections(2);
      const props = createDefaultProps({ collections });

      render(<ChatControlBar {...props} />);

      expect(screen.getByText('Knowledge Base:')).toBeInTheDocument();
      expect(screen.getByTestId('select-knowledge-base')).toBeInTheDocument();
    });

    it('shows "No knowledge base" option when no collection selected', () => {
      const props = createDefaultProps({ selectedCollectionId: null });

      render(<ChatControlBar {...props} />);

      // The select value should be 'none' (which maps to null)
      const select = screen.getByTestId('select-knowledge-base') as HTMLSelectElement;
      expect(select.value).toBe('none');
    });

    it('displays selected collection value', () => {
      const collections = createMockCollections(2);
      const props = createDefaultProps({
        collections,
        selectedCollectionId: 'collection-2',
      });

      render(<ChatControlBar {...props} />);

      const select = screen.getByTestId('select-knowledge-base') as HTMLSelectElement;
      expect(select.value).toBe('collection-2');
    });

    it('calls onCollectionChange when collection is selected', () => {
      const collections = createMockCollections(2);
      const onCollectionChange = vi.fn();
      const props = createDefaultProps({
        collections,
        selectedCollectionId: null,
        onCollectionChange,
      });

      render(<ChatControlBar {...props} />);

      const select = screen.getByTestId('select-knowledge-base');
      fireEvent.change(select, { target: { value: 'collection-1' } });

      expect(onCollectionChange).toHaveBeenCalledWith('collection-1');
    });

    it('calls onCollectionChange with null when "none" selected', () => {
      const collections = createMockCollections(2);
      const onCollectionChange = vi.fn();
      const props = createDefaultProps({
        collections,
        selectedCollectionId: 'collection-1',
        onCollectionChange,
      });

      render(<ChatControlBar {...props} />);

      const select = screen.getByTestId('select-knowledge-base');
      fireEvent.change(select, { target: { value: 'none' } });

      expect(onCollectionChange).toHaveBeenCalledWith(null);
    });

    it('shows document count for collections in options', () => {
      const collections = createMockCollections(2);
      const props = createDefaultProps({ collections });

      render(<ChatControlBar {...props} />);

      // Document counts are concatenated with names in native select mock
      const select = screen.getByTestId('select-knowledge-base') as HTMLSelectElement;
      const options = Array.from(select.options);
      const kb1Option = options.find((opt) => opt.value === 'collection-1');
      const kb2Option = options.find((opt) => opt.value === 'collection-2');
      expect(kb1Option?.textContent).toContain('Knowledge Base 1');
      expect(kb1Option?.textContent).toContain('5 docs');
      expect(kb2Option?.textContent).toContain('Knowledge Base 2');
      expect(kb2Option?.textContent).toContain('10 docs');
    });
  });

  describe('Toggle Buttons', () => {
    it('calls onToggleHistory when history button is clicked', () => {
      const onToggleHistory = vi.fn();
      const props = createDefaultProps({ onToggleHistory, isHistoryOpen: false });

      render(<ChatControlBar {...props} />);

      const historyButton = screen.getByRole('button', { name: /open history/i });
      fireEvent.click(historyButton);

      expect(onToggleHistory).toHaveBeenCalled();
    });

    it('shows close history label when history is open', () => {
      const props = createDefaultProps({ isHistoryOpen: true });

      render(<ChatControlBar {...props} />);

      expect(screen.getByRole('button', { name: /close history/i })).toBeInTheDocument();
    });

    it('shows custom message when chat history is unsupported', () => {
      const props = createDefaultProps({
        isChatHistoryUnsupported: true,
        chatHistoryUnsupportedMessage: 'History not available',
      });

      render(<ChatControlBar {...props} />);

      expect(screen.getByText('History not available')).toBeInTheDocument();
    });

    it('calls onToggleRouterActivity when router activity button is clicked', () => {
      const onToggleRouterActivity = vi.fn();
      const props = createDefaultProps({ onToggleRouterActivity });

      render(<ChatControlBar {...props} />);

      const routerButton = screen.getByRole('button', { name: /open router activity/i });
      fireEvent.click(routerButton);

      expect(onToggleRouterActivity).toHaveBeenCalled();
    });

    it('shows close router activity label when open', () => {
      const props = createDefaultProps({ isRouterActivityOpen: true });

      render(<ChatControlBar {...props} />);

      expect(screen.getByRole('button', { name: /close router activity/i })).toBeInTheDocument();
    });

    it('calls onToggleDebugger when debugger button is clicked', () => {
      const onToggleDebugger = vi.fn();
      const props = createDefaultProps({ onToggleDebugger });

      render(<ChatControlBar {...props} />);

      const debuggerButton = screen.getByRole('button', { name: /open neural debugger/i });
      fireEvent.click(debuggerButton);

      expect(onToggleDebugger).toHaveBeenCalled();
    });

    it('shows close debugger label when debugger is open', () => {
      const props = createDefaultProps({ isDebuggerOpen: true });

      render(<ChatControlBar {...props} />);

      expect(screen.getByRole('button', { name: /close neural debugger/i })).toBeInTheDocument();
    });
  });

  describe('Export Button', () => {
    it('renders export button when session and messages exist', () => {
      const MockExportButton = () => <button data-testid="export-button">Export</button>;
      const props = createDefaultProps({
        currentSessionId: 'session-1',
        messagesCount: 5,
        ExportButton: MockExportButton,
      });

      render(<ChatControlBar {...props} />);

      expect(screen.getByTestId('export-button')).toBeInTheDocument();
    });

    it('does not render export button when no session', () => {
      const MockExportButton = () => <button data-testid="export-button">Export</button>;
      const props = createDefaultProps({
        currentSessionId: null,
        messagesCount: 5,
        ExportButton: MockExportButton,
      });

      render(<ChatControlBar {...props} />);

      expect(screen.queryByTestId('export-button')).not.toBeInTheDocument();
    });

    it('does not render export button when no messages', () => {
      const MockExportButton = () => <button data-testid="export-button">Export</button>;
      const props = createDefaultProps({
        currentSessionId: 'session-1',
        messagesCount: 0,
        ExportButton: MockExportButton,
      });

      render(<ChatControlBar {...props} />);

      expect(screen.queryByTestId('export-button')).not.toBeInTheDocument();
    });

    it('does not render export button when ExportButton not provided', () => {
      const props = createDefaultProps({
        currentSessionId: 'session-1',
        messagesCount: 5,
        ExportButton: undefined,
      });

      render(<ChatControlBar {...props} />);

      expect(screen.queryByTestId('export-button')).not.toBeInTheDocument();
    });
  });

  describe('Adapter Mount Indicators', () => {
    it('shows adapter mount indicators when adapters exist', () => {
      const adapterMountItems = createMockAdapterMountItems();
      const props = createDefaultProps({ adapterMountItems });

      render(<ChatControlBar {...props} />);

      expect(screen.getByTestId('adapter-mount-indicators')).toBeInTheDocument();
      expect(screen.getByTestId('mount-indicator-adapter-1')).toBeInTheDocument();
      expect(screen.getByTestId('mount-indicator-adapter-2')).toBeInTheDocument();
    });

    it('does not show mount indicators when no adapters', () => {
      const props = createDefaultProps({ adapterMountItems: [] });

      render(<ChatControlBar {...props} />);

      expect(screen.queryByTestId('adapter-mount-indicators')).not.toBeInTheDocument();
    });

    it('passes streaming state to mount indicators', () => {
      const adapterMountItems = createMockAdapterMountItems();
      const props = createDefaultProps({ adapterMountItems, isStreaming: true });

      render(<ChatControlBar {...props} />);

      const indicators = screen.getByTestId('adapter-mount-indicators');
      expect(indicators).toHaveAttribute('data-streaming', 'true');
    });

    it('highlights active adapter from lastDecision', () => {
      const adapterMountItems = createMockAdapterMountItems();
      const props = createDefaultProps({
        adapterMountItems,
        lastDecision: {
          adapterId: 'adapter-1',
          timestamp: Date.now(),
          selectedAdapters: ['adapter-1'],
          candidates: [],
        },
      });

      render(<ChatControlBar {...props} />);

      const activeIndicator = screen.getByTestId('mount-indicator-adapter-1');
      expect(activeIndicator).toHaveAttribute('data-active', 'true');
    });
  });

  describe('Adapter Loading Status', () => {
    it('shows adapter loading status when adapter states exist', () => {
      const adapterStates = createMockAdapterStates();
      const props = createDefaultProps({ adapterStates });

      render(<ChatControlBar {...props} />);

      expect(screen.getByTestId('adapter-loading-status')).toBeInTheDocument();
    });

    it('does not show loading status when no adapter states', () => {
      const props = createDefaultProps({ adapterStates: new Map() });

      render(<ChatControlBar {...props} />);

      expect(screen.queryByTestId('adapter-loading-status')).not.toBeInTheDocument();
    });

    it('shows adapter states with names', () => {
      const adapterStates = createMockAdapterStates();
      const props = createDefaultProps({ adapterStates });

      render(<ChatControlBar {...props} />);

      expect(screen.getByTestId('adapter-state-adapter-1')).toHaveTextContent('Finance Adapter: hot');
      expect(screen.getByTestId('adapter-state-adapter-2')).toHaveTextContent('Legal Adapter: warm');
    });

    it('renders in compact mode', () => {
      const adapterStates = createMockAdapterStates();
      const props = createDefaultProps({ adapterStates });

      render(<ChatControlBar {...props} />);

      const status = screen.getByTestId('adapter-loading-status');
      expect(status).toHaveAttribute('data-compact', 'true');
    });
  });

  describe('Loading States', () => {
    it('shows base model loading button when autoLoadEnabled', () => {
      const props = createDefaultProps({
        autoLoadEnabled: true,
        onLoadBaseModelOnly: vi.fn(),
      });

      render(<ChatControlBar {...props} />);

      expect(
        screen.getByRole('button', { name: /load base model and chat without adapters/i })
      ).toBeInTheDocument();
    });

    it('disables load button when isLoadingModels is true', () => {
      const props = createDefaultProps({
        autoLoadEnabled: true,
        isLoadingModels: true,
        onLoadBaseModelOnly: vi.fn(),
      });

      render(<ChatControlBar {...props} />);

      const loadButton = screen.getByRole('button', { name: /load base model/i });
      expect(loadButton).toBeDisabled();
    });

    it('disables load button when no stack selected', () => {
      const props = createDefaultProps({
        autoLoadEnabled: true,
        selectedStackId: '',
        onLoadBaseModelOnly: vi.fn(),
      });

      render(<ChatControlBar {...props} />);

      const loadButton = screen.getByRole('button', { name: /load base model/i });
      expect(loadButton).toBeDisabled();
    });

    it('calls onLoadBaseModelOnly when load button clicked', () => {
      const onLoadBaseModelOnly = vi.fn();
      const props = createDefaultProps({
        autoLoadEnabled: true,
        selectedStackId: 'stack-1',
        onLoadBaseModelOnly,
      });

      render(<ChatControlBar {...props} />);

      const loadButton = screen.getByRole('button', { name: /load base model/i });
      fireEvent.click(loadButton);

      expect(onLoadBaseModelOnly).toHaveBeenCalled();
    });

    it('shows base model label when provided', () => {
      const props = createDefaultProps({
        autoLoadEnabled: true,
        baseModelLabel: 'llama-3.1-8b',
        onLoadBaseModelOnly: vi.fn(),
      });

      render(<ChatControlBar {...props} />);

      expect(screen.getByText('llama-3.1-8b')).toBeInTheDocument();
    });
  });

  describe('Context Badges', () => {
    it('shows document context badge when documentContext provided', () => {
      const props = createDefaultProps({
        documentContext: {
          documentId: 'doc-1',
          documentName: 'Financial Report.pdf',
        },
      });

      render(<ChatControlBar {...props} />);

      expect(screen.getByText('Financial Report.pdf')).toBeInTheDocument();
    });

    it('shows dataset context badge when datasetContext provided', () => {
      const props = createDefaultProps({
        datasetContext: {
          datasetId: 'dataset-1',
          datasetName: 'Training Data v2',
        },
      });

      render(<ChatControlBar {...props} />);

      expect(screen.getByText('Training Data v2')).toBeInTheDocument();
    });

    it('shows both document and dataset badges when both provided', () => {
      const props = createDefaultProps({
        documentContext: {
          documentId: 'doc-1',
          documentName: 'Report.pdf',
        },
        datasetContext: {
          datasetId: 'dataset-1',
          datasetName: 'Dataset v1',
        },
      });

      render(<ChatControlBar {...props} />);

      expect(screen.getByText('Report.pdf')).toBeInTheDocument();
      expect(screen.getByText('Dataset v1')).toBeInTheDocument();
    });
  });

  describe('Session Tags Manager', () => {
    it('renders tags manager when session exists', () => {
      const props = createDefaultProps({ currentSessionId: 'session-123' });

      render(<ChatControlBar {...props} />);

      const tagsManager = screen.getByTestId('chat-tags-manager');
      expect(tagsManager).toBeInTheDocument();
      expect(tagsManager).toHaveAttribute('data-session-id', 'session-123');
    });

    it('does not render tags manager when no session', () => {
      const props = createDefaultProps({ currentSessionId: undefined });

      render(<ChatControlBar {...props} />);

      expect(screen.queryByTestId('chat-tags-manager')).not.toBeInTheDocument();
    });
  });

  describe('Layout Classes', () => {
    it('applies offset class when history is open', () => {
      const props = createDefaultProps({ isHistoryOpen: true });

      const { container } = render(<ChatControlBar {...props} />);

      // The main control bar div should have ml-80 class
      const controlBarDiv = container.querySelector('.ml-80');
      expect(controlBarDiv).toBeInTheDocument();
    });

    it('applies offset class when right panels are open', () => {
      const props = createDefaultProps({ rightPanelsOpen: true });

      const { container } = render(<ChatControlBar {...props} />);

      // The main control bar div should have mr-96 class
      const controlBarDiv = container.querySelector('.mr-96');
      expect(controlBarDiv).toBeInTheDocument();
    });

    it('applies custom className', () => {
      const props = createDefaultProps({ className: 'custom-class' });

      const { container } = render(<ChatControlBar {...props} />);

      expect(container.firstChild).toHaveClass('custom-class');
    });
  });
});
