# InferenceStreaming Component

Real-time streaming inference UI component that consumes the `/v1/infer/stream` SSE endpoint. Displays tokens as they arrive with connection status, timing metadata, and comprehensive error handling.

## Features

- **Token-by-token streaming**: Real-time display of generated tokens as they arrive from the server
- **Connection management**: Visual indicators for streaming state, connection status, and errors
- **Timing metrics**: Displays tokens/second, total latency, and token count
- **OpenAI-compatible format**: Handles OpenAI chat completion chunk format with proper parsing
- **Stop sequences**: Supports stop sequences and [DONE] terminator detection
- **Error handling**: Graceful error states with retry capability
- **Auto-scroll**: Automatically scrolls to show new tokens as they arrive
- **Progress tracking**: Visual progress indicator for token generation
- **Responsive UI**: Clean, modern interface that adapts to different screen sizes

## Installation

The component is already integrated into the codebase and can be imported directly:

```tsx
import { InferenceStreaming } from '@/components';
// or
import InferenceStreaming from '@/components/InferenceStreaming';
```

## Basic Usage

### Minimal Example

```tsx
import { InferenceStreaming } from '@/components';

function MyComponent() {
  return (
    <InferenceStreaming
      prompt="Explain what LoRA adapters are."
      maxTokens={100}
    />
  );
}
```

### With Adapters

```tsx
<InferenceStreaming
  prompt="Write a Python function to sort a list."
  adapters={['code-assistant', 'python-expert']}
  maxTokens={200}
  temperature={0.5}
/>
```

### With Callbacks

```tsx
<InferenceStreaming
  prompt="What is machine learning?"
  maxTokens={150}
  onComplete={(text) => {
    console.log('Generation complete:', text);
  }}
  onError={(error) => {
    console.error('Error:', error);
  }}
/>
```

### Auto-start

```tsx
<InferenceStreaming
  prompt="Describe SSE streaming."
  maxTokens={200}
  autoStart={true}
/>
```

## Props

| Prop | Type | Default | Description |
|------|------|---------|-------------|
| `prompt` | `string` | *required* | The prompt to send for inference |
| `model` | `string` | `undefined` | Model identifier (optional) |
| `adapters` | `string[]` | `undefined` | Array of adapter IDs to use |
| `maxTokens` | `number` | `512` | Maximum number of tokens to generate |
| `temperature` | `number` | `0.7` | Sampling temperature (0.0 - 2.0) |
| `topP` | `number` | `undefined` | Top-p (nucleus) sampling parameter |
| `stopSequences` | `string[]` | `undefined` | Stop sequences to terminate generation |
| `onComplete` | `(text: string) => void` | `undefined` | Callback when inference completes |
| `onError` | `(error: Error) => void` | `undefined` | Callback on error |
| `autoStart` | `boolean` | `false` | Auto-start streaming on mount |
| `showTiming` | `boolean` | `true` | Show timing metadata display |
| `showStatus` | `boolean` | `true` | Show connection status badge |
| `className` | `string` | `undefined` | Custom CSS class |

## Hook API

The component uses the `useInferenceStream` hook internally, which can also be used directly for custom implementations:

```tsx
import { useInferenceStream } from '@/hooks/streaming';

function MyCustomComponent() {
  const {
    text,
    tokens,
    isStreaming,
    connected,
    error,
    start,
    stop,
    reset,
    tokensPerSecond,
    latencyMs,
  } = useInferenceStream({
    prompt: 'Your prompt here',
    maxTokens: 100,
    temperature: 0.7,
  });

  return (
    <div>
      <button onClick={start}>Start</button>
      <button onClick={stop}>Stop</button>
      <button onClick={reset}>Reset</button>
      <p>Status: {isStreaming ? 'Streaming' : 'Idle'}</p>
      <p>Tokens: {tokens.length}</p>
      <p>Speed: {tokensPerSecond.toFixed(1)} tokens/sec</p>
      <pre>{text}</pre>
    </div>
  );
}
```

### Hook Options

```typescript
interface InferenceStreamOptions {
  prompt: string;
  model?: string;
  adapters?: string[];
  stackId?: string;
  maxTokens?: number;
  temperature?: number;
  topP?: number;
  topK?: number;
  stopSequences?: string[];
  seed?: number;
  enabled?: boolean;
  onComplete?: (text: string) => void;
  onError?: (error: Error) => void;
  onToken?: (token: StreamToken) => void;
}
```

### Hook Result

```typescript
interface InferenceStreamResult {
  text: string;                     // Accumulated response text
  tokens: StreamToken[];            // Array of individual tokens
  isStreaming: boolean;             // Streaming active
  connected: boolean;               // SSE connection established
  error: Error | null;              // Error if failed
  start: () => void;                // Start streaming
  stop: () => void;                 // Stop streaming
  reset: () => void;                // Reset state
  latencyMs: number;                // Total latency in ms
  tokensPerSecond: number;          // Tokens per second
  finishReason: 'stop' | 'length' | 'error' | null;
  responseId: string | null;        // Response ID from stream
}
```

## Examples

See `InferenceStreaming.example.tsx` for comprehensive examples including:

1. **Basic Usage**: Simple streaming with default settings
2. **With Adapters**: Using specific adapters for domain-specific inference
3. **Interactive Prompt**: User input with dynamic prompt updates
4. **With Callbacks**: Handling completion and error events
5. **Auto-start**: Automatically begin streaming on mount
6. **Custom Styling**: Override default styles

## Architecture

### Component Structure

```
InferenceStreaming/
├── InferenceStreaming.tsx          # Main component
├── InferenceStreaming.example.tsx  # Usage examples
├── InferenceStreaming.README.md    # This file
└── useInferenceStream.ts           # Custom hook
```

### Data Flow

1. **Component Mount**: Initialize state and optionally auto-start
2. **User Action**: User clicks "Start Streaming" button
3. **Hook Activation**: `useInferenceStream` hook initiates SSE connection
4. **POST Request**: Sends inference config to `/v1/infer/stream`
5. **SSE Stream**: Server responds with token chunks via SSE
6. **Token Processing**: Each chunk is parsed and accumulated
7. **UI Update**: Component displays tokens in real-time
8. **Completion**: [DONE] signal or finish_reason triggers completion
9. **Cleanup**: Connection closed, metrics finalized

### SSE Event Format

The component expects OpenAI-compatible chat completion chunks:

```json
data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"delta":{"content":"Hi"},"finish_reason":null}]}
data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"delta":{"content":" there"},"finish_reason":null}]}
data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"delta":{},"finish_reason":"stop"}]}
data: [DONE]
```

## Error Handling

The component handles several error scenarios:

1. **Network Errors**: Connection failures, timeouts
2. **HTTP Errors**: 4xx/5xx status codes from server
3. **Parse Errors**: Malformed JSON chunks
4. **Aborted Streams**: User cancellation via Stop button
5. **Stream Errors**: Server-side streaming errors

All errors are displayed in the UI with:
- Red error badge in status indicator
- Alert message with error details
- Retry button to restart inference

## Best Practices

### Performance

- Use `autoStart={false}` (default) to avoid unnecessary API calls
- Set appropriate `maxTokens` to prevent runaway generation
- Consider using `stopSequences` for structured output

### User Experience

- Always show timing metrics (`showTiming={true}`) for transparency
- Display connection status (`showStatus={true}`) for debugging
- Provide completion callbacks for integration with parent components

### Error Handling

```tsx
<InferenceStreaming
  prompt={prompt}
  onError={(error) => {
    // Log to monitoring service
    analytics.track('inference_error', { error: error.message });

    // Show user-friendly toast
    toast.error('Failed to generate response. Please try again.');
  }}
  onComplete={(text) => {
    // Save to database
    saveResponse(text);

    // Update UI
    setGeneratedText(text);
  }}
/>
```

### Integration

```tsx
import { InferenceStreaming } from '@/components';
import { useSession } from '@/hooks/chat';

function ChatInterface() {
  const { session } = useSession();

  return (
    <InferenceStreaming
      prompt={session.currentPrompt}
      adapters={session.selectedAdapters}
      maxTokens={session.settings.maxTokens}
      temperature={session.settings.temperature}
      onComplete={(text) => {
        session.addMessage({
          role: 'assistant',
          content: text,
        });
      }}
    />
  );
}
```

## Testing

The component can be tested using React Testing Library:

```tsx
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { InferenceStreaming } from './InferenceStreaming';

test('displays streaming tokens', async () => {
  render(
    <InferenceStreaming
      prompt="Test prompt"
      autoStart={true}
    />
  );

  // Wait for streaming to start
  await waitFor(() => {
    expect(screen.getByText(/streaming/i)).toBeInTheDocument();
  });

  // Verify tokens appear
  await waitFor(() => {
    expect(screen.getByRole('textbox')).toHaveTextContent(/\w+/);
  });
});
```

## Related Components

- **StreamingIntegration**: Demonstrates all SSE streaming endpoints
- **ChatInterface**: Chat UI with message history and streaming
- **InferencePlayground**: Interactive inference testing interface

## API Reference

### Backend Endpoint

**POST** `/v1/infer/stream`

Request body:
```json
{
  "prompt": "Your prompt here",
  "model": "model-id",
  "adapters": ["adapter-1", "adapter-2"],
  "max_tokens": 500,
  "temperature": 0.7,
  "top_p": 0.9,
  "stop": ["STOP", "END"],
  "stream": true
}
```

Response: SSE stream with OpenAI-compatible chunks

See `crates/adapteros-server-api/src/handlers/streaming_infer.rs` for full backend implementation.

## Troubleshooting

### Tokens not appearing

- Check browser console for SSE connection errors
- Verify `/v1/infer/stream` endpoint is accessible
- Ensure proper authentication (session cookie)

### Slow streaming

- Check `tokensPerSecond` metric (should be > 5 tokens/sec)
- Verify backend worker is not overloaded
- Consider using lighter adapters or lower temperature

### Connection errors

- Check network tab for failed SSE requests
- Verify CORS settings for SSE endpoints
- Check backend logs for errors

## Contributing

When extending this component:

1. Follow existing TypeScript patterns
2. Add prop types to `InferenceStreamingProps`
3. Update this README with new features
4. Add examples to `InferenceStreaming.example.tsx`
5. Maintain backward compatibility

## License

Part of AdapterOS - see project LICENSE file.
