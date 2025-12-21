/**
 * CodeService
 *
 * Handles git repository and source code operations including:
 * - Repository registration and management
 * - Repository scanning
 * - Commit listing and diffs
 * - Repository analysis and reports
 */

import { BaseService } from './base';
import type {
  Repository,
  Commit,
  CommitDiff,
  TriggerScanResponse,
  RegisterGitRepositoryResponse,
  RegisterRepositoryRequest,
  RepositoryReportResponse,
} from '@/api/api-types';

export class CodeService extends BaseService {
  // ============================================================================
  // Repository Operations
  // ============================================================================

  /**
   * List all registered git repositories
   *
   * GET /v1/code/repositories
   */
  async listRepositories(): Promise<Repository[]> {
    return this.requestList<Repository>('/v1/code/repositories');
  }

  /**
   * Register a git repository
   *
   * POST /v1/code/register-repo
   */
  async registerRepository(data: RegisterRepositoryRequest): Promise<RegisterGitRepositoryResponse> {
    return this.request<RegisterGitRepositoryResponse>('/v1/code/register-repo', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  /**
   * Register a git repository (alias for GitFolderPicker compatibility)
   *
   * POST /v1/code/register-repo
   */
  async registerGitRepository(data: { repo_id: string; path: string }): Promise<RegisterGitRepositoryResponse> {
    return this.registerRepository(data);
  }

  /**
   * Trigger a repository scan
   *
   * POST /v1/code/scan
   */
  async triggerRepositoryScan(repositoryId: string): Promise<TriggerScanResponse> {
    return this.request<TriggerScanResponse>('/v1/code/scan', {
      method: 'POST',
      body: JSON.stringify({ repository_id: repositoryId }),
    });
  }

  /**
   * Unregister a repository
   *
   * DELETE /v1/code/repositories/:repositoryId
   */
  async unregisterRepository(repositoryId: string): Promise<void> {
    await this.request<void>(`/v1/code/repositories/${repositoryId}`, {
      method: 'DELETE',
    });
  }

  /**
   * Get repository details/report
   *
   * GET /v1/code/repositories/:repositoryId
   */
  async getRepositoryReport(repositoryId: string): Promise<RepositoryReportResponse> {
    return this.request<RepositoryReportResponse>(`/v1/code/repositories/${repositoryId}`);
  }

  /**
   * Get repository analysis (alias for training workflows)
   *
   * GET /v1/code/repositories/:repositoryId
   */
  async getRepositoryAnalysis(repositoryId: string): Promise<RepositoryReportResponse> {
    return this.getRepositoryReport(repositoryId);
  }

  // ============================================================================
  // Commit Operations
  // ============================================================================

  /**
   * List commits, optionally filtered by repository
   *
   * GET /v1/commits
   */
  async listCommits(repositoryId?: string): Promise<Commit[]> {
    const qs = new URLSearchParams();
    if (repositoryId) {
      qs.append('repository_id', repositoryId);
    }
    const query = qs.toString() ? `?${qs.toString()}` : '';
    return this.requestList<Commit>(`/v1/commits${query}`);
  }

  /**
   * Get commit diff
   *
   * GET /v1/commits/:sha/diff
   */
  async getCommitDiff(repositoryId: string, sha: string): Promise<CommitDiff> {
    const qs = new URLSearchParams();
    qs.append('repository_id', repositoryId);
    return this.request<CommitDiff>(`/v1/commits/${sha}/diff?${qs.toString()}`);
  }
}
