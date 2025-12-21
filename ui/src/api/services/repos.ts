/**
 * ReposService
 *
 * Handles adapter repository management operations including:
 * - Repository listing and CRUD
 * - Version management (list, promote, rollback, tag)
 * - Timeline and training job tracking
 */

import { BaseService } from './base';
import type {
  RepoSummary,
  RepoDetail,
  RepoVersionSummary,
  RepoVersionDetail,
  RepoTimelineEvent,
  RepoTrainingJobLink,
  CreateRepoRequest,
  UpdateRepoRequest,
  PromoteVersionRequest,
  RollbackVersionRequest,
  TagVersionRequest,
  StartTrainingFromVersionRequest,
} from '@/api/repo-types';

export class ReposService extends BaseService {
  // ============================================================================
  // Core Repository Operations
  // ============================================================================

  /**
   * List all repositories
   *
   * GET /v1/repos
   */
  async listRepos(): Promise<RepoSummary[]> {
    return this.requestList<RepoSummary>('/v1/repos');
  }

  /**
   * Get repository by ID
   *
   * GET /v1/repos/:repoId
   */
  async getRepo(repoId: string): Promise<RepoDetail> {
    return this.request<RepoDetail>(`/v1/repos/${repoId}`);
  }

  /**
   * Create a new repository
   *
   * POST /v1/repos
   */
  async createRepo(data: CreateRepoRequest): Promise<RepoDetail> {
    return this.request<RepoDetail>('/v1/repos', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  /**
   * Update repository
   *
   * PATCH /v1/repos/:repoId
   */
  async updateRepo(repoId: string, data: UpdateRepoRequest): Promise<RepoDetail> {
    return this.request<RepoDetail>(`/v1/repos/${repoId}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    });
  }

  // ============================================================================
  // Version Operations
  // ============================================================================

  /**
   * List versions for a repository
   *
   * GET /v1/repos/:repoId/versions
   */
  async listRepoVersions(repoId: string): Promise<RepoVersionSummary[]> {
    return this.requestList<RepoVersionSummary>(`/v1/repos/${repoId}/versions`);
  }

  /**
   * Get specific version details
   *
   * GET /v1/repos/:repoId/versions/:versionId
   */
  async getRepoVersion(repoId: string, versionId: string): Promise<RepoVersionDetail> {
    return this.request<RepoVersionDetail>(`/v1/repos/${repoId}/versions/${versionId}`);
  }

  /**
   * Promote a version to active
   *
   * POST /v1/repos/:repoId/versions/:versionId/promote
   */
  async promoteRepoVersion(
    repoId: string,
    versionId: string,
    data?: PromoteVersionRequest
  ): Promise<RepoVersionDetail> {
    return this.request<RepoVersionDetail>(`/v1/repos/${repoId}/versions/${versionId}/promote`, {
      method: 'POST',
      body: JSON.stringify(data ?? {}),
    });
  }

  /**
   * Rollback to a previous version
   *
   * POST /v1/repos/:repoId/versions/:versionId/rollback
   */
  async rollbackRepoVersion(
    repoId: string,
    versionId: string,
    data?: RollbackVersionRequest
  ): Promise<RepoVersionDetail> {
    return this.request<RepoVersionDetail>(`/v1/repos/${repoId}/versions/${versionId}/rollback`, {
      method: 'POST',
      body: JSON.stringify(data ?? {}),
    });
  }

  /**
   * Tag a version
   *
   * POST /v1/repos/:repoId/versions/:versionId/tag
   */
  async tagRepoVersion(
    repoId: string,
    versionId: string,
    data: TagVersionRequest
  ): Promise<RepoVersionDetail> {
    return this.request<RepoVersionDetail>(`/v1/repos/${repoId}/versions/${versionId}/tag`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  // ============================================================================
  // Training Operations
  // ============================================================================

  /**
   * Start training from a specific version
   *
   * POST /v1/repos/:repoId/versions/:versionId/train
   */
  async startTrainingFromVersion(
    repoId: string,
    versionId: string,
    data?: StartTrainingFromVersionRequest
  ): Promise<RepoTrainingJobLink> {
    return this.request<RepoTrainingJobLink>(`/v1/repos/${repoId}/versions/${versionId}/train`, {
      method: 'POST',
      body: JSON.stringify(data ?? {}),
    });
  }

  /**
   * List training jobs for a repository
   *
   * GET /v1/repos/:repoId/training-jobs
   */
  async listRepoTrainingJobs(repoId: string): Promise<RepoTrainingJobLink[]> {
    return this.requestList<RepoTrainingJobLink>(`/v1/repos/${repoId}/training-jobs`);
  }

  // ============================================================================
  // Timeline
  // ============================================================================

  /**
   * Get repository timeline events
   *
   * GET /v1/repos/:repoId/timeline
   */
  async getRepoTimeline(repoId: string): Promise<RepoTimelineEvent[]> {
    return this.requestList<RepoTimelineEvent>(`/v1/repos/${repoId}/timeline`);
  }
}
