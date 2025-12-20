import { describe, it, expect, vi, beforeEach } from 'vitest';
import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ChatTagsManager } from '@/components/chat/ChatTagsManager';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ChatTag } from '@/api/chat-types';

// Mock data
const mockTags: ChatTag[] = [
  {
    id: 'tag-1',
    tenant_id: 'tenant-1',
    name: 'Bug',
    color: '#ef4444',
    created_at: '2025-01-01T00:00:00Z',
  },
  {
    id: 'tag-2',
    tenant_id: 'tenant-1',
    name: 'Feature',
    color: '#10b981',
    description: 'Feature requests',
    created_at: '2025-01-01T00:00:00Z',
  },
  {
    id: 'tag-3',
    tenant_id: 'tenant-1',
    name: 'Question',
    color: '#3b82f6',
    created_at: '2025-01-01T00:00:00Z',
  },
];

const mockSessionTags: ChatTag[] = [mockTags[0], mockTags[1]];

// Mock hooks
const mockUseChatTags = vi.fn();
const mockUseSessionTags = vi.fn();
const mockUseCreateTag = vi.fn();
const mockUseAssignTagsToSession = vi.fn();
const mockUseRemoveTagFromSession = vi.fn();

vi.mock('@/hooks/chat/useChatTags', () => ({
  useChatTags: () => mockUseChatTags(),
  useSessionTags: (sessionId: string) => mockUseSessionTags(sessionId),
  useCreateTag: (options?: unknown) => mockUseCreateTag(options),
  useAssignTagsToSession: (options?: unknown) => mockUseAssignTagsToSession(options),
  useRemoveTagFromSession: (options?: unknown) => mockUseRemoveTagFromSession(options),
}));

// Mock toast (hoisted to satisfy vi.mock hoisting)
const mockToastError = vi.hoisted(() => vi.fn());
vi.mock('sonner', () => ({
  toast: {
    error: mockToastError,
    success: vi.fn(),
  },
}));

// Test wrapper component
function TestWrapper({ children }: { children: React.ReactNode }) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  return (
    <QueryClientProvider client={queryClient}>
      {children}
    </QueryClientProvider>
  );
}

describe('ChatTagsManager', () => {
  const sessionId = 'session-123';
  const createTagMutate = vi.fn();
  const assignTagsMutate = vi.fn();
  const removeTagMutate = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();

    // Default mock implementations
    mockUseChatTags.mockReturnValue({
      data: mockTags,
      isLoading: false,
    });

    mockUseSessionTags.mockReturnValue({
      data: mockSessionTags,
      isLoading: false,
    });

    mockUseCreateTag.mockImplementation((options) => ({
      mutate: createTagMutate,
      isPending: false,
      ...options,
    }));

    mockUseAssignTagsToSession.mockImplementation((options) => ({
      mutate: assignTagsMutate,
      isPending: false,
      ...options,
    }));

    mockUseRemoveTagFromSession.mockImplementation((options) => ({
      mutate: removeTagMutate,
      isPending: false,
      ...options,
    }));
  });

  describe('Rendering', () => {
    it('renders existing session tags correctly', () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      // Should show the two session tags
      expect(screen.getByText('Bug')).toBeTruthy();
      expect(screen.getByText('Feature')).toBeTruthy();
      // Should not show the unassigned tag
      expect(screen.queryByText('Question')).toBeNull();
    });

    it('renders tag badges with correct styling', () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const bugTag = screen.getByText('Bug').closest('span');
      expect(bugTag).toBeTruthy();
      expect(bugTag?.style.borderColor).toMatch(/#ef4444|rgb\(239,\s?68,\s?68\)/);
      expect(bugTag?.style.color).toMatch(/#ef4444|rgb\(239,\s?68,\s?68\)/);
    });

    it('renders remove button for each tag', () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const removeButtons = screen.getAllByLabelText(/Remove tag/i);
      expect(removeButtons).toHaveLength(2); // One for each session tag
      expect(screen.getByLabelText('Remove tag Bug')).toBeTruthy();
      expect(screen.getByLabelText('Remove tag Feature')).toBeTruthy();
    });

    it('renders "Add Tag" button', () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      expect(screen.getByText('Add Tag')).toBeTruthy();
    });

    it('applies custom className', () => {
      const { container } = render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} className="custom-class" />
        </TestWrapper>
      );

      const wrapper = container.firstChild as HTMLElement;
      expect(wrapper.className).toContain('custom-class');
    });
  });

  describe('Loading States', () => {
    it('disables Add Tag button when loading all tags', () => {
      mockUseChatTags.mockReturnValue({
        data: [],
        isLoading: true,
      });

      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const addButton = screen.getByText('Add Tag').closest('button');
      expect(addButton?.disabled).toBe(true);
    });

    it('disables Add Tag button when loading session tags', () => {
      mockUseSessionTags.mockReturnValue({
        data: [],
        isLoading: true,
      });

      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const addButton = screen.getByText('Add Tag').closest('button');
      expect(addButton?.disabled).toBe(true);
    });

    it('disables Add Tag button when both are loading', () => {
      mockUseChatTags.mockReturnValue({
        data: [],
        isLoading: true,
      });
      mockUseSessionTags.mockReturnValue({
        data: [],
        isLoading: true,
      });

      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const addButton = screen.getByText('Add Tag').closest('button');
      expect(addButton?.disabled).toBe(true);
    });
  });

  describe('Remove Tag Functionality', () => {
    it('calls removeTag mutation when remove button is clicked', async () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const removeButton = screen.getByLabelText('Remove tag Bug');
      await user.click(removeButton);

      expect(removeTagMutate).toHaveBeenCalledWith({
        sessionId,
        tagId: 'tag-1',
      });
    });

    it('disables remove button during pending state', () => {
      mockUseRemoveTagFromSession.mockImplementation((options) => ({
        mutate: removeTagMutate,
        isPending: true,
        ...options,
      }));

      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const removeButtons = screen.getAllByLabelText(/Remove tag/i);
      removeButtons.forEach((button) => {
        expect(button).toBeDisabled();
      });
    });

    it('shows toast error when remove fails', async () => {
      const errorMessage = 'Failed to remove tag from session';
      let capturedOnError: ((error: Error) => void) | undefined;

      mockUseRemoveTagFromSession.mockImplementation((options) => {
        capturedOnError = options?.onError;
        return {
          mutate: removeTagMutate,
          isPending: false,
          ...options,
        };
      });

      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      // Trigger the error callback
      expect(capturedOnError).toBeDefined();
      capturedOnError!(new Error(errorMessage));

      await waitFor(() => {
        expect(mockToastError).toHaveBeenCalledWith('Failed to remove tag', {
          description: errorMessage,
        });
      });
    });
  });

  describe('Create Tag Functionality', () => {
    it('opens popover when Add Tag button is clicked', async () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const addButton = screen.getByText('Add Tag');
      await user.click(addButton);

      await waitFor(() => {
        expect(screen.getByText('Create New Tag')).toBeTruthy();
        expect(screen.getByPlaceholderText('Tag name')).toBeTruthy();
      });
    });

    it('shows color picker in create tag form', async () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const addButton = screen.getByText('Add Tag');
      await user.click(addButton);

      await waitFor(() => {
        expect(screen.getByText('Color')).toBeTruthy();
        // Should have 8 color buttons (DEFAULT_COLORS)
        const colorButtons = screen.getAllByLabelText(/Select color/i);
        expect(colorButtons).toHaveLength(8);
      });
    });

    it('allows typing in tag name input', async () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const addButton = screen.getByText('Add Tag');
      await user.click(addButton);

      const input = await screen.findByPlaceholderText('Tag name');
      await user.type(input, 'New Tag');

      expect(input).toHaveValue('New Tag');
    });

    it('allows selecting a color', async () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const addButton = screen.getByText('Add Tag');
      await user.click(addButton);

      await waitFor(() => {
        const colorButton = screen.getByLabelText('Select color #10b981');
        expect(colorButton).toBeTruthy();
      });

      const colorButton = screen.getByLabelText('Select color #10b981');
      await user.click(colorButton);

      // Check that the button has the selected styling (scale-110 class)
      expect(colorButton.className).toContain('scale-110');
    });

    it('creates tag when form is submitted', async () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const addButton = screen.getByText('Add Tag');
      await user.click(addButton);

      const input = await screen.findByPlaceholderText('Tag name');
      await user.type(input, 'New Tag');

      const createButton = screen.getByText('Create Tag');
      await user.click(createButton);

      expect(createTagMutate).toHaveBeenCalledWith({
        name: 'New Tag',
        color: '#ef4444', // Default color
      });
    });

    it('trims whitespace from tag name', async () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const addButton = screen.getByText('Add Tag');
      await user.click(addButton);

      const input = await screen.findByPlaceholderText('Tag name');
      await user.type(input, '  Spaced Tag  ');

      const createButton = screen.getByText('Create Tag');
      await user.click(createButton);

      expect(createTagMutate).toHaveBeenCalledWith({
        name: 'Spaced Tag',
        color: '#ef4444',
      });
    });

    it('does not create tag when name is empty', async () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const addButton = screen.getByText('Add Tag');
      await user.click(addButton);

      const createButton = await screen.findByText('Create Tag');
      await user.click(createButton);

      expect(createTagMutate).not.toHaveBeenCalled();
    });

    it('does not create tag when name is only whitespace', async () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const addButton = screen.getByText('Add Tag');
      await user.click(addButton);

      const input = await screen.findByPlaceholderText('Tag name');
      await user.type(input, '   ');

      const createButton = screen.getByText('Create Tag');
      await user.click(createButton);

      expect(createTagMutate).not.toHaveBeenCalled();
    });

    it('disables create button when tag name is empty', async () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const addButton = screen.getByText('Add Tag');
      await user.click(addButton);

      const createButton = await screen.findByText('Create Tag');
      expect(createButton).toBeDisabled();
    });

    it('disables create button during pending state', async () => {
      mockUseCreateTag.mockImplementation((options) => ({
        mutate: createTagMutate,
        isPending: true,
        ...options,
      }));

      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const addButton = screen.getByText('Add Tag');
      await user.click(addButton);

      const input = await screen.findByPlaceholderText('Tag name');
      await user.type(input, 'New Tag');

      const createButton = screen.getByText('Creating...');
      expect(createButton).toBeDisabled();
    });

    it('shows "Creating..." text during pending state', async () => {
      mockUseCreateTag.mockImplementation((options) => ({
        mutate: createTagMutate,
        isPending: true,
        ...options,
      }));

      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const addButton = screen.getByText('Add Tag');
      await user.click(addButton);

      const input = await screen.findByPlaceholderText('Tag name');
      await user.type(input, 'New Tag');

      expect(screen.getByText('Creating...')).toBeTruthy();
    });

    it('shows toast error when create fails', async () => {
      const errorMessage = 'Tag name already exists';
      let capturedOnError: ((error: Error) => void) | undefined;

      mockUseCreateTag.mockImplementation((options) => {
        capturedOnError = options?.onError;
        return {
          mutate: createTagMutate,
          isPending: false,
          ...options,
        };
      });

      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      // Trigger the error callback
      expect(capturedOnError).toBeDefined();
      capturedOnError!(new Error(errorMessage));

      await waitFor(() => {
        expect(mockToastError).toHaveBeenCalledWith('Failed to create tag', {
          description: errorMessage,
        });
      });
    });

    it('automatically assigns newly created tag to session', async () => {
      const newTag: ChatTag = {
        id: 'tag-new',
        tenant_id: 'tenant-1',
        name: 'New Tag',
        color: '#ef4444',
        created_at: '2025-01-01T00:00:00Z',
      };

      let capturedOnSuccess: ((tag: ChatTag) => void) | undefined;

      mockUseCreateTag.mockImplementation((options) => {
        capturedOnSuccess = options?.onSuccess;
        return {
          mutate: createTagMutate,
          isPending: false,
          ...options,
        };
      });

      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      // Trigger success callback
      expect(capturedOnSuccess).toBeDefined();
      capturedOnSuccess!(newTag);

      await waitFor(() => {
        expect(assignTagsMutate).toHaveBeenCalledWith({
          sessionId,
          tagIds: ['tag-new'],
        });
      });
    });
  });

  describe('Assign Existing Tag Functionality', () => {
    it('shows available tags in popover', async () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const addButton = screen.getByText('Add Tag');
      await user.click(addButton);

      await waitFor(() => {
        expect(screen.getByText('Existing Tags')).toBeTruthy();
        // Should show only the unassigned tag
        expect(screen.getByText('Question')).toBeTruthy();
      });
    });

    it('does not show already assigned tags in available tags list', async () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const addButton = screen.getByText('Add Tag');
      await user.click(addButton);

      await waitFor(() => {
        // Should not show already assigned tags in the popover
        const existingTagsSection = screen.getByText('Existing Tags').parentElement;
        const bugTag = existingTagsSection?.querySelector('[data-tag-id="tag-1"]');
        expect(bugTag).toBeNull();
      });
    });

    it('assigns tag when existing tag is clicked', async () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const addButton = screen.getByText('Add Tag');
      await user.click(addButton);

      await waitFor(() => {
        expect(screen.getByText('Existing Tags')).toBeTruthy();
      });

      // Click on the Question tag in the existing tags section
      const existingTagsSection = screen.getByText('Existing Tags').parentElement;
      const questionTag = existingTagsSection?.querySelector('button');
      expect(questionTag).toBeTruthy();

      if (questionTag) {
        await user.click(questionTag);
      }

      expect(assignTagsMutate).toHaveBeenCalledWith({
        sessionId,
        tagIds: ['tag-3'],
      });
    });

    it('disables existing tag buttons during pending state', async () => {
      mockUseAssignTagsToSession.mockImplementation((options) => ({
        mutate: assignTagsMutate,
        isPending: true,
        ...options,
      }));

      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const addButton = screen.getByText('Add Tag');
      await user.click(addButton);

      await waitFor(() => {
        const existingTagsSection = screen.getByText('Existing Tags').parentElement;
        const tagButton = existingTagsSection?.querySelector('button');
        expect(tagButton).toBeDisabled();
      });
    });

    it('shows toast error when assign fails', async () => {
      const errorMessage = 'Failed to assign tag to session';
      let capturedOnError: ((error: Error) => void) | undefined;

      mockUseAssignTagsToSession.mockImplementation((options) => {
        capturedOnError = options?.onError;
        return {
          mutate: assignTagsMutate,
          isPending: false,
          ...options,
        };
      });

      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      // Trigger the error callback
      expect(capturedOnError).toBeDefined();
      capturedOnError!(new Error(errorMessage));

      await waitFor(() => {
        expect(mockToastError).toHaveBeenCalledWith('Failed to assign tag', {
          description: errorMessage,
        });
      });
    });

    it('does not show "Existing Tags" section when all tags are assigned', async () => {
      // All tags are already assigned to the session
      mockUseSessionTags.mockReturnValue({
        data: mockTags,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const addButton = screen.getByText('Add Tag');
      await user.click(addButton);

      await waitFor(() => {
        expect(screen.queryByText('Existing Tags')).toBeNull();
      });
    });
  });

  describe('Edge Cases', () => {
    it('renders correctly with no session tags', () => {
      mockUseSessionTags.mockReturnValue({
        data: [],
        isLoading: false,
      });

      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      // Should not render any tag badges
      expect(screen.queryByText('Bug')).toBeNull();
      expect(screen.queryByText('Feature')).toBeNull();
      // Should still show Add Tag button
      expect(screen.getByText('Add Tag')).toBeTruthy();
    });

    it('renders correctly with no available tags', () => {
      mockUseChatTags.mockReturnValue({
        data: [],
        isLoading: false,
      });
      mockUseSessionTags.mockReturnValue({
        data: [],
        isLoading: false,
      });

      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      expect(screen.getByText('Add Tag')).toBeTruthy();
    });

    it('handles tags without colors', () => {
      const tagsWithoutColor: ChatTag[] = [
        {
          id: 'tag-no-color',
          tenant_id: 'tenant-1',
          name: 'No Color',
          created_at: '2025-01-01T00:00:00Z',
        },
      ];

      mockUseSessionTags.mockReturnValue({
        data: tagsWithoutColor,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      expect(screen.getByText('No Color')).toBeTruthy();
    });

    it('handles undefined data from hooks', () => {
      mockUseChatTags.mockReturnValue({
        data: undefined,
        isLoading: false,
      });
      mockUseSessionTags.mockReturnValue({
        data: undefined,
        isLoading: false,
      });

      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      // Should render without crashing (empty arrays are used as defaults)
      expect(screen.getByText('Add Tag')).toBeTruthy();
    });

    it('prevents form submission with Enter key on color buttons', async () => {
      render(
        <TestWrapper>
          <ChatTagsManager sessionId={sessionId} />
        </TestWrapper>
      );

      const user = userEvent.setup();
      const addButton = screen.getByText('Add Tag');
      await user.click(addButton);

      const input = await screen.findByPlaceholderText('Tag name');
      await user.type(input, 'Test');

      const colorButton = screen.getByLabelText('Select color #10b981');
      await user.click(colorButton);

      // Should not have submitted the form (color buttons have type="button")
      expect(createTagMutate).not.toHaveBeenCalled();
    });
  });
});
