import React, { useEffect, useMemo, useRef, useState } from 'react';
import mermaid from 'mermaid';

type MermaidConfig = Parameters<typeof mermaid.initialize>[0];

interface MermaidProps {
  chart: string;
  config?: MermaidConfig;
  className?: string;
  onRender?: () => void;
  onError?: (error: unknown) => void;
}

let initialized = false;

function ensureMermaidConfig(config?: MermaidConfig) {
  if (initialized) {
    if (config) {
      mermaid.initialize({ ...config, startOnLoad: false });
    }
    return;
  }
  initialized = true;
  mermaid.initialize({
    startOnLoad: false,
    securityLevel: 'loose',
    theme: 'neutral',
    ...(config ?? {}),
  });
}

const Mermaid: React.FC<MermaidProps> = ({ chart, config, className, onRender, onError }) => {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [error, setError] = useState<string | null>(null);
  const renderKey = useMemo(() => {
    const chartSignature = chart?.length ?? 0;
    return `mermaid-${chartSignature}-${Math.random().toString(36).slice(2, 9)}`;
  }, [chart]);

  useEffect(() => {
    const element = containerRef.current;
    if (!element || typeof window === 'undefined') {
      return;
    }

    if (!chart) {
      element.innerHTML = '';
      setError(null);
      return;
    }

    ensureMermaidConfig(config);

    let cancelled = false;
    async function render() {
      try {
        const { svg } = await mermaid.render(renderKey, chart, element ?? undefined);
        if (!cancelled && element) {
          element.innerHTML = svg;
          setError(null);
          onRender?.();
        }
      } catch (err) {
        if (!cancelled && element) {
          const message = err instanceof Error ? err.message : 'Unable to render diagram';
          setError(message);
          onError?.(err);
          element.innerHTML = '';
        }
      }
    }

    void render();

    return () => {
      cancelled = true;
      if (element) {
        element.innerHTML = '';
      }
    };
  }, [chart, config, onRender, onError, renderKey]);

  if (error) {
    return (
      <div
        className={className}
        data-mermaid-error
        role="status"
        aria-live="polite"
      >
        Failed to render mermaid chart: {error}
      </div>
    );
  }

  return <div ref={containerRef} className={className} data-mermaid-container />;
};

export default Mermaid;
