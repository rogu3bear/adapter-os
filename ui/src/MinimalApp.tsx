import React, { useState, useEffect } from 'react';

interface Adapter {
  id: string;
  name: string;
  current_state: string;
  description?: string;
}

interface SystemInfo {
  memory_used_gb: number;
  memory_total_gb: number;
  adapters_loaded: number;
}

export default function MinimalApp() {
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [activeAdapter, setActiveAdapter] = useState<string | null>(null);
  const [prompt, setPrompt] = useState('');
  const [output, setOutput] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [systemInfo, setSystemInfo] = useState<SystemInfo | null>(null);

  // Fetch adapters on mount
  useEffect(() => {
    fetchAdapters();
    fetchSystemInfo();
    const interval = setInterval(fetchSystemInfo, 5000); // Update every 5s
    return () => clearInterval(interval);
  }, []);

  const fetchAdapters = async () => {
    try {
      const response = await fetch('/v1/adapters');
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const data = await response.json();
      setAdapters(data.adapters || []);
      setError(null);
    } catch (err: any) {
      setError(`Failed to fetch adapters: ${err.message}`);
      console.error('Fetch adapters error:', err);
    }
  };

  const fetchSystemInfo = async () => {
    try {
      const response = await fetch('/v1/system/info');
      if (!response.ok) return; // Don't show error for system info
      const data = await response.json();
      setSystemInfo(data);
    } catch (err) {
      console.error('System info error:', err);
    }
  };

  const loadAdapter = async (adapterId: string) => {
    setLoading(true);
    setError(null);
    try {
      const response = await fetch(`/v1/adapters/${adapterId}/load`, {
        method: 'POST',
      });
      if (!response.ok) {
        const errorData = await response.json();
        throw new Error(errorData.error || `HTTP ${response.status}`);
      }
      await fetchAdapters(); // Refresh list
      setActiveAdapter(adapterId);
    } catch (err: any) {
      setError(`Failed to load adapter: ${err.message}`);
    } finally {
      setLoading(false);
    }
  };

  const swapAdapter = async (adapterId: string) => {
    setLoading(true);
    setError(null);
    try {
      const response = await fetch(`/v1/adapters/${adapterId}/swap`, {
        method: 'POST',
      });
      if (!response.ok) {
        const errorData = await response.json();
        throw new Error(errorData.error || `HTTP ${response.status}`);
      }
      setActiveAdapter(adapterId);
    } catch (err: any) {
      setError(`Failed to swap adapter: ${err.message}`);
    } finally {
      setLoading(false);
    }
  };

  const generate = async () => {
    if (!prompt.trim()) {
      setError('Please enter a prompt');
      return;
    }

    setLoading(true);
    setError(null);
    setOutput('');

    try {
      const response = await fetch('/v1/generate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          prompt: prompt,
          max_tokens: 100,
          adapter_id: activeAdapter,
        }),
      });

      if (!response.ok) {
        const errorData = await response.json();
        throw new Error(errorData.error || `HTTP ${response.status}`);
      }

      const data = await response.json();
      setOutput(data.text || data.output || JSON.stringify(data));
    } catch (err: any) {
      setError(`Generation failed: ${err.message}`);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{
      fontFamily: 'system-ui, -apple-system, sans-serif',
      maxWidth: '1200px',
      margin: '0 auto',
      padding: '20px',
    }}>
      <header style={{ borderBottom: '2px solid #eee', paddingBottom: '20px', marginBottom: '20px' }}>
        <h1 style={{ margin: 0 }}>AdapterOS MVP</h1>
        <p style={{ color: '#666', margin: '5px 0 0 0' }}>
          Hot-swappable LoRA adapters on Apple Silicon
        </p>
      </header>

      {error && (
        <div style={{
          background: '#fee',
          border: '1px solid #fcc',
          padding: '15px',
          borderRadius: '4px',
          marginBottom: '20px',
          color: '#c00',
        }}>
          ⚠️ {error}
        </div>
      )}

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 2fr', gap: '20px' }}>
        {/* Left Column: Adapters */}
        <div>
          <h2>Available Adapters</h2>
          {adapters.length === 0 ? (
            <p style={{ color: '#999' }}>No adapters loaded</p>
          ) : (
            <div>
              {adapters.map(adapter => (
                <div
                  key={adapter.id}
                  style={{
                    padding: '15px',
                    marginBottom: '10px',
                    background: activeAdapter === adapter.id ? '#e8f5e9' : '#f5f5f5',
                    border: activeAdapter === adapter.id ? '2px solid #4CAF50' : '1px solid #ddd',
                    borderRadius: '4px',
                    cursor: 'pointer',
                  }}
                  onClick={() => adapter.current_state === 'Hot' ? swapAdapter(adapter.id) : loadAdapter(adapter.id)}
                >
                  <strong>{adapter.name || adapter.id}</strong>
                  <div style={{ fontSize: '12px', color: '#666', marginTop: '5px' }}>
                    State: {adapter.current_state}
                  </div>
                  {adapter.description && (
                    <div style={{ fontSize: '12px', color: '#999', marginTop: '5px' }}>
                      {adapter.description}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}

          <button
            onClick={fetchAdapters}
            style={{
              marginTop: '10px',
              padding: '10px',
              width: '100%',
              cursor: 'pointer',
            }}
          >
            🔄 Refresh Adapters
          </button>
        </div>

        {/* Right Column: Inference */}
        <div>
          <h2>Inference Playground</h2>

          <div style={{ marginBottom: '20px' }}>
            <label style={{ display: 'block', marginBottom: '5px', fontWeight: 'bold' }}>
              Active Adapter:
            </label>
            <div style={{
              padding: '10px',
              background: activeAdapter ? '#e3f2fd' : '#f5f5f5',
              borderRadius: '4px',
            }}>
              {activeAdapter || 'None selected'}
            </div>
          </div>

          <div style={{ marginBottom: '20px' }}>
            <label style={{ display: 'block', marginBottom: '5px', fontWeight: 'bold' }}>
              Prompt:
            </label>
            <textarea
              value={prompt}
              onChange={e => setPrompt(e.target.value)}
              rows={4}
              style={{
                width: '100%',
                padding: '10px',
                fontSize: '14px',
                fontFamily: 'monospace',
                border: '1px solid #ddd',
                borderRadius: '4px',
              }}
              placeholder="Enter your prompt here..."
            />
          </div>

          <button
            onClick={generate}
            disabled={loading || !activeAdapter}
            style={{
              padding: '12px 24px',
              fontSize: '16px',
              background: loading ? '#ccc' : '#2196F3',
              color: 'white',
              border: 'none',
              borderRadius: '4px',
              cursor: loading ? 'not-allowed' : 'pointer',
              marginBottom: '20px',
            }}
          >
            {loading ? '⏳ Generating...' : '✨ Generate'}
          </button>

          <div>
            <label style={{ display: 'block', marginBottom: '5px', fontWeight: 'bold' }}>
              Output:
            </label>
            <pre style={{
              padding: '15px',
              background: '#f5f5f5',
              border: '1px solid #ddd',
              borderRadius: '4px',
              whiteSpace: 'pre-wrap',
              wordWrap: 'break-word',
              minHeight: '150px',
              fontFamily: 'monospace',
              fontSize: '14px',
            }}>
              {output || 'Output will appear here...'}
            </pre>
          </div>
        </div>
      </div>

      {/* System Info Footer */}
      {systemInfo && (
        <footer style={{
          marginTop: '40px',
          padding: '15px',
          background: '#f5f5f5',
          borderRadius: '4px',
          fontSize: '12px',
          color: '#666',
        }}>
          <strong>System Status:</strong> {' '}
          Memory: {systemInfo.memory_used_gb.toFixed(1)}GB / {systemInfo.memory_total_gb.toFixed(1)}GB {' '}
          | Adapters Loaded: {systemInfo.adapters_loaded}
        </footer>
      )}
    </div>
  );
}