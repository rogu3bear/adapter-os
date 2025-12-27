import { useEffect, useMemo, useRef, type MouseEvent } from 'react';
import type { ReasoningSwapEvent, TopologyAdapter, TopologyCluster, TopologyLink } from '@/types/topology';
import { cn } from '@/lib/utils';
import { useSSE } from '@/hooks/realtime/useSSE';

type NodeKind = 'cluster' | 'adapter';

interface NodeState {
  id: string;
  type: NodeKind;
  clusterId?: string;
  x: number;
  y: number;
  vx: number;
  vy: number;
  radius: number;
}

interface NodeTarget {
  id: string;
  type: NodeKind;
  clusterId?: string;
  x: number;
  y: number;
  radius: number;
}

interface GalaxyCanvasProps {
  clusters: TopologyCluster[];
  adapters: TopologyAdapter[];
  links: TopologyLink[];
  activeClusterId?: string | null;
  activeAdapterId?: string | null;
  highlightClusterId?: string | null;
  driftWarning?: boolean;
  ghostPath?: string[];
  trail?: string[];
  reasoningSwaps?: ReasoningSwapEvent[];
  className?: string;
  onClusterClick?: (clusterId: string) => void;
  onPositionsUpdate?: (payload: {
    nodes: Record<string, { x: number; y: number; type: NodeKind; clusterId?: string }>;
    viewport: { width: number; height: number };
  }) => void;
}

const palette = ['#7dd3fc', '#c084fc', '#f472b6', '#fdba74', '#34d399', '#a5b4fc', '#38bdf8', '#fbbf24'];
const TRACE_RECEIPT_STREAM_ENDPOINT = '/v1/stream/trace-receipts';
const SWAP_ARC_DURATION_MS = 3400;
const SWAP_BUBBLE_DURATION_MS = 2600;
const ACTIVE_OVERRIDE_DURATION_MS = 5200;

const hashToUnit = (value: string): number => {
  let hash = 0;
  for (let i = 0; i < value.length; i += 1) {
    hash = (hash << 5) - hash + value.charCodeAt(i);
    hash |= 0; // force int32
  }
  return Math.abs(hash % 1000) / 1000;
};

const buildTargets = (
  clusters: TopologyCluster[],
  adapters: TopologyAdapter[],
  width: number,
  height: number,
): Record<string, NodeTarget> => {
  const nodes: Record<string, NodeTarget> = {};
  const centerX = width / 2;
  const centerY = height / 2;
  const ringRadius = Math.max(Math.min(width, height) / 2 - 60, 80);
  const safeClusterCount = Math.max(clusters.length, 1);

  clusters.forEach((cluster, idx) => {
    const angle = (idx / safeClusterCount) * Math.PI * 2;
    const x = centerX + Math.cos(angle) * ringRadius;
    const y = centerY + Math.sin(angle) * ringRadius;
    nodes[cluster.id] = {
      id: cluster.id,
      type: 'cluster',
      clusterId: cluster.id,
      x: cluster.position?.x ?? x,
      y: cluster.position?.y ?? y,
      radius: cluster.radius ?? 46,
    };
  });

  adapters.forEach((adapter) => {
    const clusterTarget = nodes[adapter.clusterId];
    const baseX = clusterTarget?.x ?? centerX;
    const baseY = clusterTarget?.y ?? centerY;
    const angle = hashToUnit(`${adapter.id}-${adapter.clusterId}`) * Math.PI * 2;
    const distance = (clusterTarget?.radius ?? 42) + 18 + hashToUnit(adapter.id) * 22;
    const x = baseX + Math.cos(angle) * distance;
    const y = baseY + Math.sin(angle) * distance;
    nodes[adapter.id] = {
      id: adapter.id,
      type: 'adapter',
      clusterId: adapter.clusterId,
      x: adapter.position?.x ?? x,
      y: adapter.position?.y ?? y,
      radius: 8 + Math.min(Math.max((adapter.score ?? 0) * 12, 2), 10),
    };
  });

  return nodes;
};

const coerceString = (value: unknown): string | null => {
  if (typeof value === 'string' && value.trim()) return value;
  if (typeof value === 'number') return String(value);
  return null;
};

const normalizeReasoningSwap = (payload: unknown): ReasoningSwapEvent | null => {
  if (!payload || typeof payload !== 'object') return null;
  const raw = payload as Record<string, unknown>;
  const envelope = typeof raw.reasoning_swap === 'object' && raw.reasoning_swap
    ? (raw.reasoning_swap as Record<string, unknown>)
    : raw;
  const eventType = coerceString(envelope.event_type ?? envelope.type ?? envelope.kind);

  const toAdapterId = coerceString(
    envelope.to_adapter_id ?? envelope.to_adapter ?? envelope.target_adapter ?? envelope.active_adapter ?? envelope.adapter_id
  );
  const fromAdapterId = coerceString(
    envelope.from_adapter_id ?? envelope.from_adapter ?? envelope.source_adapter ?? envelope.previous_adapter
  );
  const toClusterId = coerceString(
    envelope.to_cluster_id ?? envelope.to_cluster ?? envelope.target_cluster ?? envelope.cluster_id ?? envelope.cluster
  );
  const fromClusterId = coerceString(
    envelope.from_cluster_id ?? envelope.from_cluster ?? envelope.source_cluster ?? envelope.previous_cluster
  );
  const looksLikeSwap =
    (eventType && eventType.toLowerCase().includes('swap')) || Boolean(toAdapterId || toClusterId);
  if (!looksLikeSwap) return null;

  const timestamp = coerceString(envelope.timestamp ?? raw.timestamp) ?? new Date().toISOString();
  const id = coerceString(envelope.event_id ?? envelope.id ?? raw.id ?? raw.event_id) ?? `${timestamp}-${toAdapterId ?? toClusterId ?? 'swap'}`;
  const reason =
    coerceString(envelope.reason ?? envelope.rationale ?? envelope.reason_hash ?? envelope.summary ?? raw.reason_hash) ?? null;
  const traceId = coerceString(envelope.trace_id ?? raw.trace_id ?? envelope.request_id);

  return {
    id,
    fromClusterId,
    toClusterId,
    fromAdapterId,
    toAdapterId,
    reason,
    traceId,
    timestamp,
  };
};

export function GalaxyCanvas({
  clusters,
  adapters,
  links,
  activeClusterId,
  activeAdapterId,
  highlightClusterId,
  driftWarning = false,
  ghostPath = [],
  trail = [],
  reasoningSwaps = [],
  className,
  onClusterClick,
  onPositionsUpdate,
}: GalaxyCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const nodesRef = useRef<Record<string, NodeState>>({});
  const targetsRef = useRef<Record<string, NodeTarget>>({});
  const animationRef = useRef<number | null>(null);
  const lastActiveChangeRef = useRef<number>(performance.now());
  const swapEventsRef = useRef<
    Array<{ id: string; fromId?: string | null; toId?: string | null; reason?: string | null; startedAt: number }>
  >([]);
  const seenSwapIdsRef = useRef<Set<string>>(new Set());
  const adapterClusterRef = useRef<Map<string, string>>(new Map());
  const activeIdsRef = useRef<{ adapterId: string | null; clusterId: string | null }>({
    adapterId: activeAdapterId ?? null,
    clusterId: activeClusterId ?? null,
  });
  const activeOverrideRef = useRef<{ adapterId: string | null; clusterId: string | null; expiresAt: number; reason?: string | null } | null>(null);

  const registerSwapEvent = (swap: ReasoningSwapEvent, eventTimestamp?: number) => {
    if (!swap?.id || seenSwapIdsRef.current.has(swap.id)) return;
    const nowTs = eventTimestamp ?? performance.now();
    seenSwapIdsRef.current.add(swap.id);

    const resolvedToCluster = swap.toClusterId ?? (swap.toAdapterId ? adapterClusterRef.current.get(swap.toAdapterId) ?? null : null);
    const resolvedFromCluster = swap.fromClusterId ?? (swap.fromAdapterId ? adapterClusterRef.current.get(swap.fromAdapterId) ?? null : null);

    swapEventsRef.current.push({
      id: swap.id,
      fromId: swap.fromAdapterId ?? resolvedFromCluster ?? swap.fromClusterId ?? null,
      toId: swap.toAdapterId ?? resolvedToCluster ?? swap.toClusterId ?? null,
      reason: swap.reason ?? swap.traceId ?? null,
      startedAt: nowTs,
    });

    if (swapEventsRef.current.length > 32) {
      swapEventsRef.current = swapEventsRef.current.slice(-32);
    }

    activeOverrideRef.current = {
      adapterId: swap.toAdapterId ?? null,
      clusterId: resolvedToCluster ?? null,
      expiresAt: nowTs + ACTIVE_OVERRIDE_DURATION_MS,
      reason: swap.reason ?? swap.traceId ?? null,
    };
    lastActiveChangeRef.current = nowTs;
  };

  const clusterColors = useMemo(() => {
    const map: Record<string, string> = {};
    clusters.forEach((cluster, idx) => {
      map[cluster.id] = palette[idx % palette.length];
    });
    return map;
  }, [clusters]);

  useEffect(() => {
    activeIdsRef.current = {
      adapterId: activeAdapterId ?? null,
      clusterId: activeClusterId ?? null,
    };
    lastActiveChangeRef.current = performance.now();
  }, [activeAdapterId, activeClusterId]);

  useEffect(() => {
    const map = new Map<string, string>();
    adapters.forEach((adapter) => map.set(adapter.id, adapter.clusterId));
    adapterClusterRef.current = map;
  }, [adapters]);

  useSSE<unknown>(TRACE_RECEIPT_STREAM_ENDPOINT, {
    enabled: true,
    onMessage: (payload) => {
      const swap = normalizeReasoningSwap(payload);
      if (swap) {
        registerSwapEvent(swap);
      }
    },
  });

  useEffect(() => {
    if (!reasoningSwaps.length) return;
    const now = performance.now();
    reasoningSwaps.forEach((swap) => {
      registerSwapEvent(swap, now);
    });
  }, [reasoningSwaps]);

  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const resize = () => {
      const rect = container.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      canvas.width = rect.width * dpr;
      canvas.height = rect.height * dpr;
      canvas.style.width = `${rect.width}px`;
      canvas.style.height = `${rect.height}px`;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

      targetsRef.current = buildTargets(clusters, adapters, rect.width, rect.height);

      // Initialize node states with previous positions when available
      const nextNodes: Record<string, NodeState> = {};
      Object.values(targetsRef.current).forEach((target) => {
        const prev = nodesRef.current[target.id];
        nextNodes[target.id] = prev
          ? { ...prev, radius: target.radius }
          : {
              id: target.id,
              type: target.type,
              clusterId: target.clusterId,
              x: target.x,
              y: target.y,
              vx: 0,
              vy: 0,
              radius: target.radius,
            };
      });
      nodesRef.current = nextNodes;

      if (onPositionsUpdate) {
        const snapshot: Record<string, { x: number; y: number; type: NodeKind; clusterId?: string }> = {};
        Object.values(targetsRef.current).forEach((target) => {
          snapshot[target.id] = { x: target.x, y: target.y, type: target.type, clusterId: target.clusterId };
        });
        onPositionsUpdate({ nodes: snapshot, viewport: { width: rect.width, height: rect.height } });
      }
    };

    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(container);

    const step = () => {
      const rect = container.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) {
        animationRef.current = requestAnimationFrame(step);
        return;
      }

      const nodes = nodesRef.current;
      const targets = targetsRef.current;

      // Physics step: spring toward targets and light repulsion between clusters
      const clusterNodes = Object.values(nodes).filter((n) => n.type === 'cluster');
      for (let i = 0; i < clusterNodes.length; i += 1) {
        for (let j = i + 1; j < clusterNodes.length; j += 1) {
          const a = clusterNodes[i];
          const b = clusterNodes[j];
          const dx = a.x - b.x;
          const dy = a.y - b.y;
          const distSq = dx * dx + dy * dy;
          if (distSq === 0) continue;
          const minDist = a.radius + b.radius + 40;
          if (distSq < minDist * minDist) {
            const dist = Math.sqrt(distSq) || 1;
            const force = (minDist - dist) * 0.002;
            const nx = dx / dist;
            const ny = dy / dist;
            a.vx += nx * force;
            a.vy += ny * force;
            b.vx -= nx * force;
            b.vy -= ny * force;
          }
        }
      }

      Object.values(nodes).forEach((node) => {
        const target = targets[node.id];
        if (!target) return;
        const stiffness = node.type === 'cluster' ? 0.06 : 0.08;
        node.vx += (target.x - node.x) * stiffness;
        node.vy += (target.y - node.y) * stiffness;
        node.vx *= 0.88;
        node.vy *= 0.88;
        node.x += node.vx;
        node.y += node.vy;
        const margin = 12;
        node.x = Math.max(margin, Math.min(rect.width - margin, node.x));
        node.y = Math.max(margin, Math.min(rect.height - margin, node.y));
      });

      // Render
      ctx.clearRect(0, 0, rect.width, rect.height);

      if (ghostPath.length > 1) {
        ctx.save();
        ctx.setLineDash([8, 10]);
        ctx.lineWidth = 2;
        ctx.strokeStyle = 'rgba(168,85,247,0.55)';
        ctx.shadowBlur = 10;
        ctx.shadowColor = 'rgba(168,85,247,0.35)';
        for (let i = 0; i < ghostPath.length - 1; i += 1) {
          const from = nodes[ghostPath[i]];
          const to = nodes[ghostPath[i + 1]];
          if (!from || !to) continue;
          ctx.beginPath();
          ctx.moveTo(from.x, from.y);
          ctx.lineTo(to.x, to.y);
          ctx.stroke();
        }
        ctx.restore();
      }

      // Path tracing glow
      if (trail.length > 1) {
        ctx.save();
        ctx.lineWidth = 3;
        ctx.shadowBlur = 12;
        ctx.shadowColor = '#22d3ee';
        ctx.strokeStyle = 'rgba(45,212,191,0.7)';
        for (let i = 0; i < trail.length - 1; i += 1) {
          const from = nodes[trail[i]];
          const to = nodes[trail[i + 1]];
          if (!from || !to) continue;
          ctx.beginPath();
          ctx.moveTo(from.x, from.y);
          ctx.lineTo(to.x, to.y);
          ctx.stroke();
        }
        ctx.restore();
      }

      // Links
      ctx.save();
      ctx.lineWidth = 1.25;
      ctx.strokeStyle = 'rgba(255,255,255,0.08)';
      links.forEach((link) => {
        const source = nodes[link.source];
        const target = nodes[link.target];
        if (!source || !target) return;
        ctx.beginPath();
        ctx.moveTo(source.x, source.y);
        ctx.lineTo(target.x, target.y);
        ctx.stroke();
      });
      ctx.restore();

      const nowTs = performance.now();
      if (activeOverrideRef.current && activeOverrideRef.current.expiresAt < nowTs) {
        activeOverrideRef.current = null;
      }
      const effectiveActiveAdapterId = activeOverrideRef.current?.adapterId ?? activeIdsRef.current.adapterId;
      const effectiveActiveClusterId =
        activeOverrideRef.current?.clusterId ??
        activeIdsRef.current.clusterId ??
        (effectiveActiveAdapterId ? adapterClusterRef.current.get(effectiveActiveAdapterId) ?? null : null);

      const swapAnnotations: Array<{ target: NodeState; reason?: string | null; age: number }> = [];
      const activeSwaps = swapEventsRef.current.filter((event) => nowTs - event.startedAt < SWAP_ARC_DURATION_MS);
      swapEventsRef.current = activeSwaps;

      if (activeSwaps.length) {
        ctx.save();
        activeSwaps.forEach((event) => {
          const to = (event.toId && nodes[event.toId]) || null;
          const from = (event.fromId && nodes[event.fromId]) || null;
          const target = to ?? from;
          const start = from ?? to;
          if (!target || !start) return;
          const age = nowTs - event.startedAt;
          const t = Math.min(age / SWAP_ARC_DURATION_MS, 1);
          const fade = Math.max(0, 1 - t);
          const dx = target.x - start.x;
          const dy = target.y - start.y;
          const dist = Math.max(1, Math.hypot(dx, dy));
          const midX = start.x + dx * 0.5;
          const midY = start.y + dy * 0.5;
          const nx = -dy / dist;
          const ny = dx / dist;
          const curve = 32 + Math.min(110, dist * 0.28);
          const cx = midX + nx * curve;
          const cy = midY + ny * curve;

          ctx.lineWidth = 2.4 + fade * 1.6;
          const gradient = ctx.createLinearGradient(start.x, start.y, target.x, target.y);
          gradient.addColorStop(0, 'rgba(56,189,248,0.08)');
          gradient.addColorStop(0.25, 'rgba(56,189,248,0.55)');
          gradient.addColorStop(0.75, 'rgba(74,222,128,0.95)');
          gradient.addColorStop(1, 'rgba(74,222,128,0.15)');
          ctx.strokeStyle = gradient;
          ctx.shadowBlur = 16 + fade * 10;
          ctx.shadowColor = 'rgba(56,189,248,0.7)';
          ctx.beginPath();
          ctx.moveTo(start.x, start.y);
          ctx.quadraticCurveTo(cx, cy, target.x, target.y);
          ctx.stroke();

          const sparkT = Math.min(1, t * 1.05);
          const inv = 1 - sparkT;
          const sparkX = inv * inv * start.x + 2 * inv * sparkT * cx + sparkT * sparkT * target.x;
          const sparkY = inv * inv * start.y + 2 * inv * sparkT * cy + sparkT * sparkT * target.y;
          ctx.fillStyle = `rgba(125,211,252,${0.75 * fade + 0.2})`;
          ctx.beginPath();
          ctx.arc(sparkX, sparkY, 3.5 + fade * 2.1, 0, Math.PI * 2);
          ctx.fill();

          ctx.fillStyle = `rgba(94,234,212,${0.28 * fade})`;
          ctx.beginPath();
          ctx.arc(target.x, target.y, (target.radius ?? 10) + 12 * (1 - t * 0.65), 0, Math.PI * 2);
          ctx.fill();

          if (age < SWAP_BUBBLE_DURATION_MS && to) {
            swapAnnotations.push({ target: to, reason: event.reason, age });
          }
        });
        ctx.restore();
      }

      // Clusters (nebulae)
      Object.values(nodes)
        .filter((node) => node.type === 'cluster')
        .forEach((node) => {
          const color = clusterColors[node.id] ?? palette[0];
          const gradient = ctx.createRadialGradient(node.x, node.y, node.radius * 0.2, node.x, node.y, node.radius * 1.6);
          gradient.addColorStop(0, `${color}88`);
          gradient.addColorStop(1, `${color}08`);

          ctx.save();
          ctx.fillStyle = gradient;
          ctx.beginPath();
          ctx.arc(node.x, node.y, node.radius * 1.6, 0, Math.PI * 2);
          ctx.fill();

          // Outline
          ctx.lineWidth = 2;
          ctx.strokeStyle = `${color}55`;
          ctx.beginPath();
          ctx.arc(node.x, node.y, node.radius, 0, Math.PI * 2);
          ctx.stroke();

          const isActiveCluster = effectiveActiveClusterId && node.id === effectiveActiveClusterId;
          const isHighlighted = highlightClusterId && node.id === highlightClusterId;

          if (isHighlighted) {
            const pulse = 4 + 3 * Math.sin(nowTs / 300);
            ctx.lineWidth = 2;
            ctx.strokeStyle = '#f97316aa';
            ctx.beginPath();
            ctx.arc(node.x, node.y, node.radius + pulse, 0, Math.PI * 2);
            ctx.stroke();
          }

          if (isActiveCluster) {
            const t = (nowTs - lastActiveChangeRef.current) / 250;
            const pulse = 6 + Math.sin(t) * 3;
            ctx.lineWidth = 3;
            ctx.strokeStyle = driftWarning ? '#ef4444dd' : '#22d3eedb';
            ctx.shadowBlur = 18;
            ctx.shadowColor = driftWarning ? '#ef4444aa' : '#22d3eeaa';
            ctx.beginPath();
            ctx.arc(node.x, node.y, node.radius + pulse, 0, Math.PI * 2);
            ctx.stroke();
          }

          ctx.restore();
        });

      // Adapters (stars)
      Object.values(nodes)
        .filter((node) => node.type === 'adapter')
        .forEach((node) => {
          const clusterColor = clusterColors[node.clusterId ?? ''] ?? '#e2e8f0';
          ctx.save();
          ctx.fillStyle = clusterColor;
          ctx.shadowBlur = 10;
          ctx.shadowColor = clusterColor;
          ctx.beginPath();
          ctx.arc(node.x, node.y, node.radius, 0, Math.PI * 2);
          ctx.fill();

          if (node.id === effectiveActiveAdapterId) {
            const t = (nowTs - lastActiveChangeRef.current) / 200;
            const pulse = 4 + Math.sin(t) * 2;
            ctx.lineWidth = 2;
            ctx.strokeStyle = '#22d3eef0';
            ctx.beginPath();
            ctx.arc(node.x, node.y, node.radius + pulse, 0, Math.PI * 2);
            ctx.stroke();
          }

          ctx.restore();
        });

      if (swapAnnotations.length) {
        ctx.save();
        ctx.font = '12px "Satoshi", "Inter", system-ui, -apple-system, sans-serif';
        swapAnnotations.forEach((annotation) => {
          const to = annotation.target;
          const alpha = Math.max(0, 1 - annotation.age / SWAP_BUBBLE_DURATION_MS);
          const reasonText = (annotation.reason && annotation.reason.length > 46)
            ? `${annotation.reason.slice(0, 46)}…`
            : annotation.reason ?? 'Reasoning swap';
          const padding = 8;
          const textWidth = ctx.measureText(reasonText).width;
          const bubbleWidth = textWidth + padding * 2;
          const bubbleHeight = 26;
          const bubbleX = Math.min(rect.width - bubbleWidth - 8, Math.max(8, to.x + 14));
          const bubbleY = Math.max(12, Math.min(rect.height - bubbleHeight - 8, to.y - bubbleHeight - 6));

          ctx.globalAlpha = alpha;
          ctx.fillStyle = 'rgba(15,23,42,0.88)';
          ctx.strokeStyle = 'rgba(94,234,212,0.6)';
          ctx.lineWidth = 1;
          const roundRect = (ctx as CanvasRenderingContext2D & { roundRect?: (x: number, y: number, w: number, h: number, r: number) => void }).roundRect;
          if (roundRect) {
            ctx.beginPath();
            roundRect.call(ctx, bubbleX, bubbleY, bubbleWidth, bubbleHeight, 8);
            ctx.fill();
            ctx.stroke();
          } else {
            ctx.beginPath();
            ctx.rect(bubbleX, bubbleY, bubbleWidth, bubbleHeight);
            ctx.fill();
            ctx.stroke();
          }
          ctx.fillStyle = '#e0f2fe';
          ctx.fillText(reasonText, bubbleX + padding, bubbleY + bubbleHeight / 2 + 4);
        });
        ctx.restore();
      }

      animationRef.current = requestAnimationFrame(step);
    };

    step();

    return () => {
      observer.disconnect();
      if (animationRef.current) cancelAnimationFrame(animationRef.current);
    };
  }, [adapters, clusters, links, trail, ghostPath, activeClusterId, activeAdapterId, highlightClusterId, driftWarning, onPositionsUpdate, clusterColors]);

  const handleClick = (event: MouseEvent<HTMLCanvasElement>) => {
    if (!canvasRef.current || !onClusterClick) return;
    const rect = canvasRef.current.getBoundingClientRect();
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;
    let nearestCluster: NodeState | null = null;
    let nearestDistance = Number.POSITIVE_INFINITY;
    Object.values(nodesRef.current)
      .filter((n): n is NodeState => n.type === 'cluster')
      .forEach((node) => {
        const dist = Math.hypot(node.x - x, node.y - y);
        if (dist < node.radius + 18 && dist < nearestDistance) {
          nearestCluster = node;
          nearestDistance = dist;
        }
      });
    const clusterId = nearestCluster?.id;
    if (clusterId) {
      onClusterClick(clusterId);
    }
  };

  return (
    <div ref={containerRef} className={cn('relative h-[280px] w-full overflow-hidden rounded-xl border border-border/60 bg-gradient-to-br from-slate-900/50 via-slate-900/40 to-slate-900/30', className)}>
      <canvas ref={canvasRef} className="h-full w-full" onClick={handleClick} />
      <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_center,rgba(255,255,255,0.05)_0,transparent_45%)]" />
    </div>
  );
}
