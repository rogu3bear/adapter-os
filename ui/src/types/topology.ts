export interface TopologyNodePosition {
  x: number;
  y: number;
  z?: number;
}

export interface TopologyCluster {
  id: string;
  name: string;
  kind?: string;
  adapterIds: string[];
  position?: TopologyNodePosition;
  radius?: number;
  metadata?: Record<string, unknown>;
}

export interface TopologyAdapter {
  id: string;
  name?: string;
  clusterId: string;
  score?: number;
  status?: string;
  position?: TopologyNodePosition;
  metadata?: Record<string, unknown>;
}

export interface TopologyLink {
  source: string;
  target: string;
  weight?: number;
  kind?: 'intra' | 'inter' | string;
}

export interface PredictedPathNode {
  id: string;
  adapterId?: string;
  clusterId?: string;
  confidence?: number;
  kind?: string;
}

export interface TopologyGraph {
  clusters: TopologyCluster[];
  adapters: TopologyAdapter[];
  links: TopologyLink[];
  startingClusterId?: string | null;
  version?: string;
  predictedPath?: PredictedPathNode[];
}

export interface RouterEventStep {
  id: string;
  adapterId?: string | null;
  clusterId?: string | null;
  score?: number | null;
  reason?: string | null;
  timestamp: string;
  drift?: number | null;
}

export interface ReasoningSwapEvent {
  id: string;
  fromClusterId?: string | null;
  toClusterId?: string | null;
  fromAdapterId?: string | null;
  toAdapterId?: string | null;
  reason?: string | null;
  traceId?: string | null;
  timestamp: string;
}

export interface RouterRealtimeState {
  activeAdapterId: string | null;
  activeClusterId: string | null;
  reasoningScore: number | null;
  startingClusterId: string | null;
  driftDistance: number | null;
  lastUpdated?: string;
  trail: string[];
}
