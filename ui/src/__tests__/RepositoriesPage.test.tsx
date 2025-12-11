import { describe, expect, it } from 'vitest';
import { groupReposByBaseModel } from '@/pages/Repositories/RepositoriesPage';
import type { RepoSummary } from '@/api/repo-types';

const baseRepo = {
  default_branch: 'main',
  branches: [],
  tags: [],
  created_at: '2025-01-01T00:00:00Z',
  status: 'healthy' as const,
};

describe('groupReposByBaseModel', () => {
  it('groups repositories by base model', () => {
    const repos: RepoSummary[] = [
      { id: 'a', name: 'Repo A', base_model: 'qwen', ...baseRepo },
      { id: 'b', name: 'Repo B', base_model: 'llama', ...baseRepo },
      { id: 'c', name: 'Repo C', base_model: 'qwen', ...baseRepo },
    ];

    const grouped = groupReposByBaseModel(repos);

    expect(grouped).toHaveLength(2);
    const qwenGroup = grouped.find(g => g.baseModel === 'qwen');
    expect(qwenGroup?.repos.map(r => r.id)).toEqual(['a', 'c']);
  });

  it('returns empty array when input missing', () => {
    expect(groupReposByBaseModel()).toEqual([]);
  });
});
