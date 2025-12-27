import { useEffect, useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { apiClient } from '@/api/services';
import { logger, toError } from '@/utils/logger';
import type { components } from '@/api/generated';
import type {
  PredictedPathNode,
  TopologyAdapter,
  TopologyCluster,
  TopologyGraph,
  TopologyLink,
  TopologyNodePosition,
} from '@/types/topology';

type ApiTopologyGraph = components['schemas']['TopologyGraph'];
type ApiAdapterTopology = components['schemas']['AdapterTopology'];
type ApiAdjacencyEdge = components['schemas']['AdjacencyEdge'];
type ApiClusterDefinition = components['schemas']['ClusterDefinition'];
type ApiPredictedPathNode = components['schemas']['PredictedPathNode'];

interface UseTopologyOptions {
  enabled?: boolean;
  previewText?: string;
  debounceMs?: number;
}

const FALLBACK_GRAPH: TopologyGraph = {
  clusters: [
    { id: 'core', name: 'Core', adapterIds: ['planner-1', 'reasoner-1'], position: { x: -80, y: -20 }, radius: 48 },
    { id: 'config', name: 'Config', adapterIds: ['config-checker'], position: { x: 90, y: 10 }, radius: 44 },
    { id: 'memory', name: 'Memory', adapterIds: ['retriever-1', 'semantic-cache'], position: { x: 10, y: 110 }, radius: 52 },
  ],
  adapters: [
    { id: 'planner-1', name: 'Planner', clusterId: 'core', score: 0.62 },
    { id: 'reasoner-1', name: 'Reasoner', clusterId: 'core', score: 0.54 },
    { id: 'config-checker', name: 'Config Cluster', clusterId: 'config', score: 0.4 },
    { id: 'retriever-1', name: 'Retriever', clusterId: 'memory', score: 0.33 },
    { id: 'semantic-cache', name: 'Semantic Cache', clusterId: 'memory', score: 0.28 },
  ],
  links: [
    { source: 'planner-1', target: 'reasoner-1', weight: 0.4 },
    { source: 'planner-1', target: 'retriever-1', weight: 0.3 },
    { source: 'retriever-1', target: 'semantic-cache', weight: 0.6 },
    { source: 'reasoner-1', target: 'config-checker', weight: 0.2 },
  ],
  startingClusterId: 'core',
  version: 'fallback',
  clustersVersion: 'fallback',
  predictedPath: [],
};

const isRecord = (value: unknown): value is Record<string, unknown> => (
  Boolean(value) && typeof value === 'object' && !Array.isArray(value)
);

const normalizePosition = (value: unknown): TopologyNodePosition | undefined => {
  if (!value || typeof value !== 'object') return undefined;
  const raw = value as Record<string, unknown>;
  const x = Number(raw.x ?? raw.X ?? raw.lon ?? raw.lng);
  const y = Number(raw.y ?? raw.Y ?? raw.lat ?? raw.latitude);
  const zCandidate = raw.z ?? raw.Z;
  const hasX = Number.isFinite(x);
  const hasY = Number.isFinite(y);
  if (!hasX || !hasY) return undefined;
  const position: TopologyNodePosition = { x, y };
  const z = Number(zCandidate);
  if (Number.isFinite(z)) {
    position.z = z;
  }
  return position;
};

const safeId = (value: unknown): string | null => {
  if (typeof value === 'string' && value.trim()) return value;
  if (typeof value === 'number') return String(value);
  return null;
};

const normalizeCluster = (raw: unknown): TopologyCluster | null => {
  if (!raw || typeof raw !== 'object') return null;
  const input = raw as Record<string, unknown>;
  const id = safeId(input.id ?? input.cluster_id ?? input.key ?? input.name);
  if (!id) return null;

  const adapterIds = Array.isArray(input.adapters)
    ? input.adapters
    : Array.isArray(input.adapter_ids)
      ? input.adapter_ids
      : [];

  return {
    id,
    name: typeof input.name === 'string' && input.name.trim()
      ? input.name
      : typeof input.description === 'string' && input.description.trim()
        ? input.description
        : id,
    kind: typeof input.kind === 'string' ? input.kind : undefined,
    adapterIds: adapterIds.map((a) => String(a)),
    position: normalizePosition(input.position ?? input.pos),
    radius: typeof input.radius === 'number' ? input.radius : undefined,
    metadata: input.metadata && typeof input.metadata === 'object' ? (input.metadata as Record<string, unknown>) : undefined,
    description: typeof input.description === 'string' ? input.description : undefined,
    version: typeof input.version === 'string' ? input.version : undefined,
    defaultAdapterId: safeId((input as ApiClusterDefinition).default_adapter_id ?? (input as any).defaultAdapterId) ?? null,
  };
};

const normalizeAdapter = (raw: unknown): TopologyAdapter | null => {
  if (!raw || typeof raw !== 'object') return null;
  const input = raw as Record<string, unknown>;
  const id = safeId(input.id ?? input.adapter_id ?? input.name);
  if (!id) return null;
  const clusterIdsRaw = Array.isArray((input as ApiAdapterTopology).cluster_ids)
    ? (input as ApiAdapterTopology).cluster_ids
    : Array.isArray((input as any).clusterIds)
      ? (input as any).clusterIds
      : undefined;
  const clusterIds = clusterIdsRaw?.map((value: string | number) => safeId(value) ?? '').filter(Boolean) as string[] | undefined;
  const clusterId = safeId(input.clusterId ?? input.cluster_id ?? input.cluster ?? clusterIds?.[0]);
  if (!clusterId && !(clusterIds?.length)) return null;

  const transitionSource = (input as ApiAdapterTopology).transition_probabilities ?? (input as any).transitionProbabilities;
  const transitionProbabilities = isRecord(transitionSource)
    ? Object.entries(transitionSource as Record<string, unknown>).reduce<Record<string, number>>((acc, [key, value]) => {
        const weight = Number(value);
        if (Number.isFinite(weight)) acc[key] = weight;
        return acc;
      }, {})
    : undefined;

  return {
    id,
    name: typeof input.name === 'string' && input.name.trim() ? input.name : id,
    clusterId: clusterId ?? (clusterIds?.[0] as string),
    clusterIds,
    score: typeof input.score === 'number' ? input.score : typeof input.reasoning_score === 'number' ? input.reasoning_score : undefined,
    status: typeof input.status === 'string' ? input.status : undefined,
    position: normalizePosition(input.position ?? input.pos),
    metadata: input.metadata && typeof input.metadata === 'object' ? (input.metadata as Record<string, unknown>) : undefined,
    transitionProbabilities,
  };
};

const normalizeLink = (raw: unknown): TopologyLink | null => {
  if (!raw || typeof raw !== 'object') return null;
  const input = raw as Record<string, unknown>;
  const source = safeId(input.source ?? input.from ?? input.src);
  const target = safeId(input.target ?? input.to ?? input.dst ?? input.destination);
  if (!source || !target) return null;

  return {
    source,
    target,
    weight: typeof input.weight === 'number' ? input.weight : typeof input.strength === 'number' ? input.strength : undefined,
    kind: typeof input.kind === 'string' ? input.kind : undefined,
  };
};

const normalizePredictedPath = (raw: unknown): PredictedPathNode[] => {
  const items = Array.isArray(raw)
    ? raw
    : Array.isArray((raw as any)?.nodes)
      ? (raw as any).nodes
      : [];

  if (!items.length) return [];

  const normalized: PredictedPathNode[] = [];

  for (const item of items as Array<Partial<ApiPredictedPathNode> & Record<string, unknown>>) {
    if (!item || typeof item !== 'object') continue;
    const id = safeId(item.id ?? item.adapter_id ?? item.node_id ?? item.name);
    if (!id) continue;
    const adapterId = safeId(item.adapter_id ?? (item as any).adapterId ?? item.node_id ?? item.id) ?? id;
    const clusterId = safeId(item.cluster_id ?? (item as any).clusterId);
    const confidenceValue = item.confidence ?? (item as any).score ?? (item as any).gate ?? (item as any).gate_value ?? (item as any).gateValue;
    const confidence = typeof confidenceValue === 'number'
      ? confidenceValue
      : Number.isFinite(Number(confidenceValue))
        ? Number(confidenceValue)
        : undefined;

    normalized.push({
      id,
      adapterId,
      clusterId: clusterId ?? undefined,
      confidence,
      kind: typeof item.kind === 'string' ? item.kind : adapterId ? 'adapter' : undefined,
    });
  }

  return normalized;
};

const normalizeAdjacencyLinks = (raw: unknown): TopologyLink[] => {
  if (!raw || typeof raw !== 'object') return [];
  const adjacency = raw as Record<string, unknown>;
  const links: TopologyLink[] = [];
  Object.entries(adjacency).forEach(([key, value]) => {
    const source = safeId(key);
    if (!source) return;
    const edges = Array.isArray(value) ? value : [];
    edges.forEach((edge) => {
      if (!edge || typeof edge !== 'object') return;
      const edgeRecord = edge as Partial<ApiAdjacencyEdge> & Record<string, unknown>;
      const target = safeId(edgeRecord.to_cluster_id ?? (edgeRecord as any).toClusterId ?? edgeRecord.target ?? edgeRecord.id);
      if (!target) return;
      const weight = typeof edgeRecord.probability === 'number'
        ? edgeRecord.probability
        : typeof edgeRecord.weight === 'number'
          ? edgeRecord.weight
          : undefined;
      links.push({ source, target, weight, kind: 'cluster' });
    });
  });
  return links;
};

const normalizeFromNodes = (raw: Record<string, unknown>): TopologyGraph | null => {
  const nodes = Array.isArray(raw.nodes) ? raw.nodes : [];
  const edges = Array.isArray(raw.links) ? raw.links : Array.isArray(raw.edges) ? raw.edges : [];

  if (!nodes.length) return null;

  const clusters: TopologyCluster[] = [];
  const adapters: TopologyAdapter[] = [];

  nodes.forEach((node) => {
    const obj = (typeof node === 'object' && node) ? (node as Record<string, unknown>) : null;
    if (!obj) return;
    const type = typeof obj.type === 'string' ? obj.type.toLowerCase() : undefined;
    if (type === 'cluster') {
      const cluster = normalizeCluster(obj);
      if (cluster) clusters.push(cluster);
    } else {
      const adapter = normalizeAdapter({ ...obj, clusterId: obj.clusterId ?? obj.cluster_id ?? obj.cluster ?? obj.parent });
      if (adapter) adapters.push(adapter);
    }
  });

  const links: TopologyLink[] = edges
    .map(normalizeLink)
    .filter((edge): edge is TopologyLink => Boolean(edge));

  const startingClusterId = safeId(
    raw.start_cluster ?? raw.starting_cluster ?? raw.origin ?? clusters[0]?.id
  );

  return { clusters, adapters, links, startingClusterId };
};

const normalizeTopologyResponse = (raw: unknown): TopologyGraph => {
  if (!raw || typeof raw !== 'object') {
    logger.warn('Topology endpoint returned no data, using fallback', { component: 'useTopologyData' });
    return FALLBACK_GRAPH;
  }

  const input = raw as Record<string, unknown>;
  const clustersRaw = Array.isArray(input.clusters) ? input.clusters : null;
  const adaptersRaw = Array.isArray(input.adapters) ? input.adapters : null;
  const linksRaw = Array.isArray(input.links) ? input.links : Array.isArray(input.edges) ? input.edges : null;
  const adjacencyLinks = normalizeAdjacencyLinks((input as ApiTopologyGraph).adjacency ?? (input as any).adjacency);

  if (clustersRaw || adaptersRaw) {
    const clusters = (clustersRaw ?? []).map(normalizeCluster).filter((c): c is TopologyCluster => Boolean(c));
    const adapters = (adaptersRaw ?? []).map(normalizeAdapter).filter((a): a is TopologyAdapter => Boolean(a));
    const links = [
      ...adjacencyLinks,
      ...(linksRaw ?? []).map(normalizeLink).filter((l): l is TopologyLink => Boolean(l)),
    ];
    const startingClusterId = safeId(
      input.start_cluster ?? input.starting_cluster ?? input.origin_cluster ?? clusters[0]?.id
    );

    // If adapters reference clusters not present, create lightweight placeholders
    const missingClusters = new Set(
      adapters
        .flatMap((a) => [a.clusterId, ...(a.clusterIds ?? [])])
        .filter((id) => id && !clusters.find((c) => c.id === id))
    );
    if (missingClusters.size > 0) {
      missingClusters.forEach((id) => {
        clusters.push({ id, name: id, adapterIds: [], position: undefined });
      });
    }

    // Fill adapterIds on clusters
    clusters.forEach((cluster) => {
      cluster.adapterIds = adapters
        .filter((a) => a.clusterId === cluster.id || (a.clusterIds ?? []).includes(cluster.id))
        .map((a) => a.id);
    });

    if (!clusters.length && !adapters.length) {
      logger.warn('Topology endpoint returned an empty graph, using fallback', { component: 'useTopologyData' });
      return FALLBACK_GRAPH;
    }

    return {
      clusters,
      adapters,
      links,
      startingClusterId: startingClusterId ?? null,
      version: typeof input.version === 'string' ? input.version : undefined,
      clustersVersion: typeof (input as ApiTopologyGraph).clusters_version === 'string' ? (input as ApiTopologyGraph).clusters_version : undefined,
      predictedPath: normalizePredictedPath(input.predicted_path ?? input.predictedPath),
    };
  }

  const nodeGraph = normalizeFromNodes(input);
  if (nodeGraph) {
    return nodeGraph;
  }

  logger.warn('Topology endpoint format was unknown, using fallback graph', { component: 'useTopologyData' });
  return FALLBACK_GRAPH;
};

export function useTopologyData(options: UseTopologyOptions = {}) {
  const { enabled = true, previewText, debounceMs = 500 } = options;

  const normalizedPreview = previewText?.trim() ?? '';
  const [debouncedPreview, setDebouncedPreview] = useState<string>('');

  useEffect(() => {
    if (!normalizedPreview) {
      setDebouncedPreview('');
      return;
    }
    const handle = window.setTimeout(() => setDebouncedPreview(normalizedPreview), debounceMs);
    return () => window.clearTimeout(handle);
  }, [debounceMs, normalizedPreview]);

  const previewKey = normalizedPreview ? debouncedPreview : '';
  const queryEnabled = enabled && (!normalizedPreview || debouncedPreview === normalizedPreview);

  const queryResult = useQuery({
    queryKey: ['topology', previewKey || 'base'],
    enabled: queryEnabled,
    staleTime: previewKey ? 10_000 : 30_000,
    refetchInterval: previewKey ? undefined : 60_000,
    queryFn: async (): Promise<TopologyGraph> => {
      const query = previewKey ? `?preview_text=${encodeURIComponent(previewKey)}` : '';
      const response = await apiClient.request<unknown>(`/v1/topology${query}`);
      return normalizeTopologyResponse(response);
    },
  });

  const data = useMemo<TopologyGraph | undefined>(() => {
    if (queryResult.data) return queryResult.data;
    if (queryResult.isFetching) return undefined;
    return FALLBACK_GRAPH;
  }, [queryResult.data, queryResult.isFetching]);

  if (queryResult.error) {
    logger.error('Failed to load topology', { component: 'useTopologyData' }, toError(queryResult.error));
  }

  return {
    ...queryResult,
    data,
  };
}
