import { AdapterHealthFlag } from '@/api/adapter-types';
import { TrustState } from '@/api/training-types';

export type LineageDirection = 'upstream' | 'downstream' | 'both';

export type LineageNodeType =
  | 'document'
  | 'document_collection'
  | 'dataset'
  | 'dataset_version'
  | 'training_job'
  | 'adapter_repo'
  | 'adapter_version'
  | 'evidence';

export interface LineageBadge {
  label: string;
  tone?: 'info' | 'success' | 'warning' | 'danger';
}

export interface LineageNode {
  id: string;
  type: LineageNodeType;
  label: string;
  subtitle?: string;
  href?: string;
  trust_state?: TrustState;
  adapter_health?: AdapterHealthFlag;
  badges?: LineageBadge[];
  metadata?: Record<string, unknown>;
}

export interface LineageLevel {
  type: LineageNodeType;
  label?: string;
  nodes: LineageNode[];
  total?: number;
  has_more?: boolean;
  next_cursor?: string;
}

export interface LineageGraphResponse {
  schema_version?: string;
  root: LineageNode;
  upstream?: LineageLevel[];
  downstream?: LineageLevel[];
  evidence?: LineageNode[];
  summary?: {
    upstream_count?: number;
    downstream_count?: number;
  };
}

export interface LineageQueryParams {
  direction?: LineageDirection;
  include_evidence?: boolean;
  limit_per_level?: number;
  cursors?: Record<string, string>;
}

export type LineageEntityKind = 'dataset_version' | 'adapter_version';
