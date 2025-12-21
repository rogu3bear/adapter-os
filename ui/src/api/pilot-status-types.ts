/**
 * Pilot Status API Types
 *
 * Type definitions for the pilot readiness endpoint:
 * `GET /v1/system/pilot-status`
 */

export interface PilotTrainingJobSummary {
  id: string;
  status: string;
  started_at: string;
  completed_at?: string;
  adapter_name?: string;
  repo_id: string;
}

export interface PilotStatusResponse {
  schema_version: string;
  tenant_id: string;
  api_ready: boolean;
  db_ready: boolean;
  db_error?: string;
  worker_registered: boolean;
  workers_total: number;
  worker_status_counts: Record<string, number>;
  workers_error?: string;
  models_seeded: boolean;
  models_total: number;
  model_names: string[];
  models_error?: string;
  last_training_job?: PilotTrainingJobSummary;
  training_error?: string;
  timestamp: number;
}

