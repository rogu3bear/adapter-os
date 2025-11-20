// General API response and utility types
// Extracted from types.ts for better organization
//
// 【2025-01-20†rectification†api_types】

export interface OpenAIModelInfo {
  id: string;
  object: string;
  created: number;
  owned_by: string;
}

export interface OpenAIModelsListResponse {
  object: string;
  data: OpenAIModelInfo[];
}

export interface UpdateMonitoringRuleRequest {
  name?: string;
  description?: string;
  enabled?: boolean;
  conditions?: any;
  actions?: any;
  severity?: 'low' | 'medium' | 'high' | 'critical';
}

export interface ErrorResponse {
  error: string;
  code?: string;
  details?: string;
  timestamp?: string;
}

export interface SystemMetrics {
  cpu_usage?: number;
  memory_usage?: number;
  disk_usage?: number;
  network_rx?: number;
  network_tx?: number;
  timestamp?: string;
}
