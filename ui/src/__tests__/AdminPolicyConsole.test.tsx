import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import AdminPolicyConsole from '@/pages/Admin/AdminPolicyConsole';
import type { AdapterRepositorySummary } from '@/api/repo-types';
import type { Dataset } from '@/api/training-types';

const mockListAdapterRepositories = vi.fn();
const mockUpdateAdapterRepositoryPolicy = vi.fn();
const mockListDatasets = vi.fn();
const mockApplyDatasetTrustOverride = vi.fn();
const mockGetTenantStorageUsage = vi.fn();

vi.mock('@/api/client', () => ({
  __esModule: true,
  default: {
    listAdapterRepositories: mockListAdapterRepositories,
    updateAdapterRepositoryPolicy: mockUpdateAdapterRepositoryPolicy,
    listDatasets: mockListDatasets,
    applyDatasetTrustOverride: mockApplyDatasetTrustOverride,
    getTenantStorageUsage: mockGetTenantStorageUsage,
  },
}));

vi.mock('@/components/ui/use-toast', () => ({
  useToast: () => ({ toast: vi.fn() }),
}));

const repoFixture: AdapterRepositorySummary = {
  id: 'repo-1',
  tenant_id: 'tenant-1',
  name: 'Repo One',
  base_model_id: 'qwen2',
  default_branch: 'main',
  archived: false,
  created_at: 'now',
  training_policy: {
    repo_id: 'repo-1',
    coreml_mode: 'coreml_preferred',
    repo_tier: 'normal',
    auto_rollback_on_trust_regress: false,
    coreml_allowed: true,
    coreml_required: false,
    autopromote_coreml: false,
    preferred_backends: ['coreml'],
    created_at: 'now',
  },
};

const datasetFixture: Dataset = {
  id: 'ds-1',
  dataset_version_id: 'v1',
  name: 'Dataset One',
  hash_b3: 'hash',
  source_type: 'uploaded_files',
  file_count: 1,
  total_size_bytes: 10,
  total_tokens: 0,
  validation_status: 'validated',
  created_at: 'now',
  updated_at: 'now',
  trust_state: 'blocked',
};

beforeEach(() => {
  mockListAdapterRepositories.mockResolvedValue([repoFixture]);
  mockUpdateAdapterRepositoryPolicy.mockResolvedValue({
    ...repoFixture.training_policy!,
    coreml_mode: 'coreml_strict',
    repo_tier: 'high_assurance',
    auto_rollback_on_trust_regress: true,
  });
  mockListDatasets.mockResolvedValue({ datasets: [datasetFixture], schema_version: '1.0', total: 1, page: 1, page_size: 1 });
  mockApplyDatasetTrustOverride.mockResolvedValue({
    dataset_id: datasetFixture.id,
    dataset_version_id: datasetFixture.dataset_version_id!,
    effective_trust_state: 'allowed_with_warning',
  });
  mockGetTenantStorageUsage.mockResolvedValue({
    tenant_id: 'tenant-1',
    dataset_bytes: 1024,
    artifact_bytes: 2048,
    dataset_versions: 2,
    adapter_versions: 3,
    soft_limit_bytes: 10 * 1024,
    hard_limit_bytes: 20 * 1024,
    soft_exceeded: false,
    hard_exceeded: false,
  });
});

describe('AdminPolicyConsole', () => {
  it('lets admin review and save repository policy changes', async () => {
    render(<AdminPolicyConsole />);

    await screen.findByText('Repo One');

    await userEvent.click(screen.getByRole('button', { name: /edit/i }));
    await userEvent.click(screen.getByRole('button', { name: /review & save/i }));
    await userEvent.click(screen.getByRole('button', { name: /confirm/i }));

    await waitFor(() => {
      expect(mockUpdateAdapterRepositoryPolicy).toHaveBeenCalledWith('repo-1', {
        coreml_mode: 'coreml_preferred',
        repo_tier: 'normal',
        auto_rollback_on_trust_regress: false,
      });
    });
  });

  it('requires justification and warns when unblocking datasets', async () => {
    render(<AdminPolicyConsole />);
    await userEvent.click(screen.getByRole('tab', { name: /dataset trust overrides/i }));
    await screen.findByText('Dataset One');
    await userEvent.click(screen.getByRole('button', { name: /override/i }));

    await userEvent.click(screen.getByText(/blocked/i));
    await userEvent.click(await screen.findByText(/allowed/i));

    expect(screen.getByText(/moving from blocked/)).toBeInTheDocument();

    const justification = screen.getByPlaceholderText(/audit-ready justification/i);
    await userEvent.type(justification, 'Allow for controlled test');
    await userEvent.click(screen.getByRole('button', { name: /apply override/i }));

    await waitFor(() => {
      expect(mockApplyDatasetTrustOverride).toHaveBeenCalledWith('ds-1', {
        override_state: 'allowed',
        reason: 'Allow for controlled test',
      });
    });
  });

  it('shows storage usage and limits', async () => {
    render(<AdminPolicyConsole />);
    await userEvent.click(screen.getByRole('tab', { name: /storage & quotas/i }));
    await screen.findByText(/Dataset storage/i);
    expect(screen.getByText(/Soft limit/i)).toBeInTheDocument();
  });
});
