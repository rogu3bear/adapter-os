import { useState, useRef, useEffect } from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Badge } from '@/components/ui/badge';
import { Send, Bot, User, Terminal, ExternalLink, Loader2 } from 'lucide-react';
import { apiClient } from '@/api/client';

interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  suggested_cli?: string;
  relevant_links?: string[];
  timestamp: Date;
}

interface SystemChatWidgetProps {
  systemOverview?: object;
  adapters?: object[];
}

const WELCOME_MESSAGE: ChatMessage = {
  id: 'welcome',
  role: 'assistant',
  content: `Hello! I'm your AdapterOS assistant. I can help you with:

• Understanding system status and metrics
• Managing adapters and stacks
• Training and dataset operations
• Navigating to specific features
• Running CLI commands

Ask me anything about your AdapterOS instance!`,
  timestamp: new Date(),
};

export function SystemChatWidget({ systemOverview, adapters }: SystemChatWidgetProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([WELCOME_MESSAGE]);
  const [input, setInput] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const scrollAreaRef = useRef<HTMLDivElement>(null);
  const navigate = useNavigate();
  const location = useLocation();

  useEffect(() => {
    if (scrollAreaRef.current) {
      const scrollContainer = scrollAreaRef.current.querySelector('[data-radix-scroll-area-viewport]');
      if (scrollContainer) {
        scrollContainer.scrollTop = scrollContainer.scrollHeight;
      }
    }
  }, [messages]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || isLoading) return;

    const userMessage: ChatMessage = {
      id: `user-${Date.now()}`,
      role: 'user',
      content: input.trim(),
      timestamp: new Date(),
    };

    setMessages(prev => [...prev, userMessage]);
    setInput('');
    setIsLoading(true);

    try {
      // Build conversation history for API
      const conversationMessages = messages
        .filter(msg => msg.id !== 'welcome') // Exclude welcome message
        .map(msg => ({
          role: msg.role,
          content: msg.content,
        }))
        .concat([{ role: 'user', content: userMessage.content }]);

      // Build context object
      const context = {
        route: location.pathname,
        metrics_snapshot: systemOverview,
        user_role: 'owner', // This component is specifically for owner role
      };

      // Call real API
      const response = await apiClient.sendOwnerChatMessage(conversationMessages, context);

      const assistantMessage: ChatMessage = {
        id: `assistant-${Date.now()}`,
        role: 'assistant',
        content: response.response,
        suggested_cli: response.suggested_cli,
        relevant_links: response.relevant_links,
        timestamp: new Date(),
      };

      setMessages(prev => [...prev, assistantMessage]);
    } catch (error) {
      console.error('Chat API error:', error);

      // Fallback to mock response for development/resilience
      try {
        const mockResponse = await mockChatAPI(userMessage.content, { systemOverview, adapters });

        const assistantMessage: ChatMessage = {
          id: `assistant-${Date.now()}`,
          role: 'assistant',
          content: mockResponse.content + '\n\n_Note: Using fallback responses. API may be unavailable._',
          suggested_cli: mockResponse.suggested_cli,
          relevant_links: mockResponse.relevant_links,
          timestamp: new Date(),
        };

        setMessages(prev => [...prev, assistantMessage]);
      } catch (fallbackError) {
        // Both real and fallback failed
        const errorMessage: ChatMessage = {
          id: `error-${Date.now()}`,
          role: 'assistant',
          content: 'Sorry, I encountered an error processing your request. Please try again later.',
          timestamp: new Date(),
        };
        setMessages(prev => [...prev, errorMessage]);
      }
    } finally {
      setIsLoading(false);
    }
  };

  const handleLinkClick = (link: string) => {
    if (link.startsWith('/')) {
      navigate(link);
    } else {
      window.open(link, '_blank', 'noopener,noreferrer');
    }
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
  };

  return (
    <div
      className="flex flex-col h-full bg-white rounded-lg border border-slate-200 shadow-sm"
      role="region"
      aria-label="System Assistant chat interface"
    >
      <div className="flex items-center gap-2 px-4 py-3 border-b border-slate-200 bg-slate-50">
        <Bot className="w-5 h-5 text-blue-600" aria-hidden="true" />
        <h3 className="font-semibold text-slate-900">System Assistant</h3>
        <Badge variant="secondary" className="ml-auto text-xs">
          AI Chat
        </Badge>
      </div>

      <ScrollArea ref={scrollAreaRef} className="flex-1 p-4">
        <div className="space-y-4" role="log" aria-live="polite" aria-label="Chat messages">
          {messages.map((message) => (
            <div
              key={message.id}
              className={`flex gap-3 ${message.role === 'user' ? 'justify-end' : 'justify-start'}`}
            >
              {message.role === 'assistant' && (
                <div className="flex-shrink-0 w-8 h-8 rounded-full bg-blue-100 flex items-center justify-center">
                  <Bot className="w-5 h-5 text-blue-600" />
                </div>
              )}

              <div
                className={`flex flex-col gap-2 max-w-[80%] ${
                  message.role === 'user' ? 'items-end' : 'items-start'
                }`}
              >
                <div
                  className={`rounded-lg px-4 py-2 ${
                    message.role === 'user'
                      ? 'bg-blue-600 text-white'
                      : 'bg-slate-100 text-slate-900'
                  }`}
                >
                  <p className="text-sm whitespace-pre-wrap">{message.content}</p>
                </div>

                {message.suggested_cli && (
                  <div className="w-full bg-slate-900 rounded-lg p-3 border border-slate-700">
                    <div className="flex items-center gap-2 mb-2">
                      <Terminal className="w-4 h-4 text-green-400" />
                      <span className="text-xs font-medium text-slate-300">Suggested CLI</span>
                    </div>
                    <code className="text-sm text-green-400 font-mono block">
                      {message.suggested_cli}
                    </code>
                    <Button
                      size="sm"
                      variant="ghost"
                      className="mt-2 h-7 text-xs text-slate-300 hover:text-white"
                      onClick={() => copyToClipboard(message.suggested_cli!)}
                    >
                      Copy to clipboard
                    </Button>
                  </div>
                )}

                {message.relevant_links && message.relevant_links.length > 0 && (
                  <div className="w-full bg-blue-50 rounded-lg p-3 border border-blue-200">
                    <div className="flex items-center gap-2 mb-2">
                      <ExternalLink className="w-4 h-4 text-blue-600" />
                      <span className="text-xs font-medium text-blue-900">Relevant Links</span>
                    </div>
                    <div className="space-y-1">
                      {message.relevant_links.map((link, idx) => (
                        <button
                          key={idx}
                          onClick={() => handleLinkClick(link)}
                          className="block text-sm text-blue-600 hover:text-blue-800 hover:underline"
                        >
                          {formatLinkText(link)}
                        </button>
                      ))}
                    </div>
                  </div>
                )}

                <span className="text-xs text-slate-400">
                  {message.timestamp.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                </span>
              </div>

              {message.role === 'user' && (
                <div className="flex-shrink-0 w-8 h-8 rounded-full bg-slate-200 flex items-center justify-center">
                  <User className="w-5 h-5 text-slate-600" />
                </div>
              )}
            </div>
          ))}

          {isLoading && (
            <div className="flex gap-3 justify-start">
              <div className="flex-shrink-0 w-8 h-8 rounded-full bg-blue-100 flex items-center justify-center">
                <Bot className="w-5 h-5 text-blue-600" />
              </div>
              <div className="bg-slate-100 rounded-lg px-4 py-2">
                <Loader2 className="w-5 h-5 text-slate-400 animate-spin" />
              </div>
            </div>
          )}
        </div>
      </ScrollArea>

      <form onSubmit={handleSubmit} className="p-4 border-t border-slate-200" role="search">
        <label htmlFor="chat-input" className="sr-only">Ask the system assistant</label>
        <div className="flex gap-2">
          <Input
            id="chat-input"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="Ask about system status, adapters, training..."
            disabled={isLoading}
            className="flex-1"
            aria-describedby="chat-status"
          />
          <span id="chat-status" className="sr-only" aria-live="assertive">
            {isLoading ? 'Thinking...' : 'Ready'}
          </span>
          <Button
            type="submit"
            disabled={isLoading || !input.trim()}
            size="icon"
            aria-label="Send message"
          >
            <Send className="w-4 h-4" aria-hidden="true" />
          </Button>
        </div>
      </form>
    </div>
  );
}

function formatLinkText(link: string): string {
  if (link.startsWith('/')) {
    const parts = link.split('/').filter(Boolean);
    return parts.map(part => part.charAt(0).toUpperCase() + part.slice(1)).join(' > ');
  }
  return link;
}

async function mockChatAPI(
  userInput: string,
  context: { systemOverview?: object; adapters?: object[] }
): Promise<{
  content: string;
  suggested_cli?: string;
  relevant_links?: string[];
}> {
  await new Promise(resolve => setTimeout(resolve, 800));

  const input = userInput.toLowerCase();

  if (input.includes('adapter') && input.includes('list')) {
    return {
      content: `You can list all adapters using the CLI or the web interface. The Adapters page shows all registered adapters with their status, tier, and activation metrics.`,
      suggested_cli: 'aosctl adapter list',
      relevant_links: ['/adapters'],
    };
  }

  if (input.includes('train')) {
    return {
      content: `To train a new adapter, you'll need to:
1. Upload a training dataset
2. Configure hyperparameters (rank, alpha, epochs)
3. Start the training job

The Training page provides a wizard to guide you through this process.`,
      suggested_cli: 'aosctl train --dataset-id <id> --output adapters/my-adapter.aos --rank 16 --epochs 3',
      relevant_links: ['/training', '/training/datasets'],
    };
  }

  if (input.includes('status') || input.includes('health')) {
    const adapterCount = context.adapters?.length || 0;
    return {
      content: `System Status:
• ${adapterCount} adapters registered
• Backend: MLX (operational)
• Database: Connected
• All services healthy

You can view detailed metrics on the System Overview page.`,
      relevant_links: ['/system/overview'],
    };
  }

  if (input.includes('stack')) {
    return {
      content: `Adapter stacks let you combine multiple adapters for specialized workflows. You can create and manage stacks from the Adapter Stacks page.`,
      suggested_cli: 'aosctl stack create --name my-stack --adapters adapter1,adapter2',
      relevant_links: ['/stacks'],
    };
  }

  if (input.includes('dataset')) {
    return {
      content: `Datasets are stored in JSONL format with input/target pairs. You can upload datasets via the web interface or CLI, then use them for training.`,
      suggested_cli: 'aosctl dataset upload --name my-dataset --file training.jsonl',
      relevant_links: ['/training/datasets'],
    };
  }

  return {
    content: `I can help you with:
• Adapter management (list, register, load/unload)
• Training workflows and datasets
• System status and monitoring
• CLI commands and navigation

What would you like to know more about?`,
  };
}
