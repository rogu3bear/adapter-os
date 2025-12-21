import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { ChatMessage } from '@/components/chat/ChatMessage';
import type { ExportMetadata } from '@/utils/export/renderMarkdown';

// Create mock functions at top level for hoisting
const mockText = vi.fn();
const mockOutput = vi.fn();
const mockSetFontSize = vi.fn();

// Mock modules before imports
vi.mock('jspdf', () => {
  // Create these inside the factory to avoid hoisting issues
  const text = vi.fn();
  const output = vi.fn();
  const setFontSize = vi.fn();

  return {
    __esModule: true,
    default: class MockJsPDF {
      text = text;
      output = output;
      setFontSize = setFontSize;
      lastAutoTable: any = null;

      constructor() {
        // Capture calls in outer scope for assertions
        this.text = (...args: any[]) => {
          mockText(...args);
          return text(...args);
        };
        this.output = (...args: any[]) => {
          const result = output(...args) || new Blob(['mock-pdf'], { type: 'application/pdf' });
          mockOutput(...args);
          return result;
        };
        this.setFontSize = (...args: any[]) => {
          mockSetFontSize(...args);
          return setFontSize(...args);
        };
      }
    },
  };
});

vi.mock('jspdf-autotable', () => ({
  __esModule: true,
  default: vi.fn((doc, options) => {
    (doc as any).lastAutoTable = { finalY: options.startY ? options.startY + 20 : 60 };
  }),
}));

// Import after mocking
import { generateChatSessionPdf, downloadPdfFile } from '@/utils/export/generatePdf';
import autoTable from 'jspdf-autotable';

const mockAutoTable = autoTable as unknown as ReturnType<typeof vi.fn>;

describe('generateChatSessionPdf', () => {
  const mockMetadata: ExportMetadata = {
    exportId: 'export-123',
    exportTimestamp: '2025-12-12T10:00:00.000Z',
    entityType: 'chat_session',
    entityId: 'session-456',
    entityName: 'Test Session',
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('generates a PDF blob', async () => {
    const messages: ChatMessage[] = [
      {
        id: 'msg-1',
        role: 'user',
        content: 'Hello',
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
      },
    ];

    const blob = await generateChatSessionPdf('My Session', messages, mockMetadata);

    expect(blob).toBeInstanceOf(Blob);
    expect(mockOutput).toHaveBeenCalledWith('blob');
  });

  it('sets title with correct font size', async () => {
    const messages: ChatMessage[] = [];

    await generateChatSessionPdf('Test Session', messages, mockMetadata);

    expect(mockSetFontSize).toHaveBeenCalledWith(20);
    expect(mockText).toHaveBeenCalledWith('Chat Session: Test Session', 14, 22);
  });

  it('creates metadata table', async () => {
    const messages: ChatMessage[] = [];

    await generateChatSessionPdf('Test', messages, mockMetadata);

    expect(mockAutoTable).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({
        head: [['Field', 'Value']],
        body: expect.arrayContaining([
          ['Export Date', '2025-12-12T10:00:00.000Z'],
          ['Session ID', 'session-456'],
          ['Export ID', 'export-123'],
        ]),
      })
    );
  });

  it('creates messages table', async () => {
    const messages: ChatMessage[] = [
      {
        id: 'msg-1',
        role: 'user',
        content: 'Question',
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
      },
    ];

    await generateChatSessionPdf('Test', messages, mockMetadata);

    expect(mockAutoTable).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({
        head: [['Time', 'Role', 'Message']],
      })
    );
  });

  it('truncates long messages', async () => {
    const longContent = 'A'.repeat(300);
    const messages: ChatMessage[] = [
      {
        id: 'msg-1',
        role: 'assistant',
        content: longContent,
        timestamp: new Date('2025-12-12T09:00:00.000Z'),
      },
    ];

    await generateChatSessionPdf('Test', messages, mockMetadata);

    const calls = mockAutoTable.mock.calls;
    const messagesCall = calls.find((call) => call[1].head?.[0]?.includes('Message'));
    expect(messagesCall).toBeDefined();

    const messageContent = messagesCall![1].body[0][2];
    expect(messageContent.length).toBe(203); // 200 + '...'
    expect(messageContent).toContain('...');
  });

  it('handles messages without timestamps', async () => {
    const messages: ChatMessage[] = [
      {
        id: 'msg-1',
        role: 'user',
        content: 'Hello',
        timestamp: undefined as unknown as Date,
      },
    ];

    await generateChatSessionPdf('Test', messages, mockMetadata);

    const calls = mockAutoTable.mock.calls;
    const messagesCall = calls.find((call) => call[1].head?.[0]?.includes('Time'));
    expect(messagesCall![1].body[0][0]).toBe('');
  });
});

describe('downloadPdfFile', () => {
  let mockLink: HTMLAnchorElement;
  let mockCreateObjectURL: ReturnType<typeof vi.fn>;
  let mockRevokeObjectURL: ReturnType<typeof vi.fn>;
  let mockClick: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockClick = vi.fn();
    mockLink = {
      href: '',
      download: '',
      click: mockClick,
    } as unknown as HTMLAnchorElement;

    vi.spyOn(document, 'createElement').mockReturnValue(mockLink);

    mockCreateObjectURL = vi.fn().mockReturnValue('blob:mock-url');
    mockRevokeObjectURL = vi.fn();
    global.URL.createObjectURL = mockCreateObjectURL;
    global.URL.revokeObjectURL = mockRevokeObjectURL;

    vi.spyOn(document.body, 'appendChild').mockImplementation(() => mockLink);
    vi.spyOn(document.body, 'removeChild').mockImplementation(() => mockLink);
  });

  it('creates download link with correct attributes', () => {
    const blob = new Blob(['test'], { type: 'application/pdf' });
    downloadPdfFile(blob, 'test.pdf');

    expect(mockLink.href).toBe('blob:mock-url');
    expect(mockLink.download).toBe('test.pdf');
  });

  it('adds .pdf extension if missing', () => {
    const blob = new Blob(['test'], { type: 'application/pdf' });
    downloadPdfFile(blob, 'test');

    expect(mockLink.download).toBe('test.pdf');
  });

  it('does not add duplicate .pdf extension', () => {
    const blob = new Blob(['test'], { type: 'application/pdf' });
    downloadPdfFile(blob, 'test.pdf');

    expect(mockLink.download).toBe('test.pdf');
  });

  it('triggers download', () => {
    const blob = new Blob(['test'], { type: 'application/pdf' });
    downloadPdfFile(blob, 'test.pdf');

    expect(mockClick).toHaveBeenCalled();
  });

  it('cleans up object URL', () => {
    const blob = new Blob(['test'], { type: 'application/pdf' });
    downloadPdfFile(blob, 'test.pdf');

    expect(mockRevokeObjectURL).toHaveBeenCalledWith('blob:mock-url');
  });
});
