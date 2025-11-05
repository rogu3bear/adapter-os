import { render, screen, waitFor } from '@testing-library/react';
import { GitIntegrationPage } from '@/components/GitIntegrationPage';
import apiClient from '@/api/client';

// Mock dependencies
jest.mock('@/api/client');

const mockedApiClient = apiClient as jest.Mocked<typeof apiClient>;

describe('GitIntegrationPage', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  describe('Repository URL formatting', () => {
    it('displays proper URLs as-is', async () => {
      const mockRepos = [
        {
          id: 'repo1',
          url: 'https://github.com/user/repo.git',
          branch: 'main',
          commit_count: 42,
          last_scan: '2024-01-15T10:00:00Z',
        },
        {
          id: 'repo2',
          url: 'git@github.com:user/repo.git',
          branch: 'main',
          commit_count: 24,
          last_scan: '2024-01-14T10:00:00Z',
        },
      ];

      mockedApiClient.listRepositories.mockResolvedValue(mockRepos);

      render(<GitIntegrationPage selectedTenant="default" />);

      await waitFor(() => {
        expect(screen.getByText('https://github.com/user/repo.git')).toBeInTheDocument();
        expect(screen.getByText('git@github.com:user/repo.git')).toBeInTheDocument();
      });
    });

    it('formats repo_id fallbacks with "Repository:" prefix', async () => {
      const mockRepos = [
        {
          id: 'repo1',
          url: 'github.com/user/repo', // This is a repo_id fallback, not a real URL
          url_is_fallback: true,
          branch: 'main',
          commit_count: 42,
          last_scan: '2024-01-15T10:00:00Z',
        },
      ];

      mockedApiClient.listRepositories.mockResolvedValue(mockRepos);

      render(<GitIntegrationPage selectedTenant="default" />);

      await waitFor(() => {
        expect(screen.getByText('Repository: github.com/user/repo')).toBeInTheDocument();
      });

      // Verify the title attribute shows the raw URL
      const repoElement = screen.getByText('Repository: github.com/user/repo');
      expect(repoElement).toHaveAttribute('title', 'github.com/user/repo');
    });

    it('handles mixed repository sources correctly', async () => {
      const mockRepos = [
        {
          id: 'repo1',
          url: 'https://github.com/user/repo1.git', // Real URL
          url_is_fallback: false,
          branch: 'main',
          commit_count: 42,
          last_scan: '2024-01-15T10:00:00Z',
        },
        {
          id: 'repo2',
          url: 'github.com/user/repo2', // Repo ID fallback
          url_is_fallback: true,
          branch: 'develop',
          commit_count: 24,
          last_scan: '2024-01-14T10:00:00Z',
        },
        {
          id: 'repo3',
          url: 'https://gitlab.com/user/repo3.git', // Real URL
          url_is_fallback: false,
          branch: 'master',
          commit_count: 18,
          last_scan: '2024-01-13T10:00:00Z',
        },
      ];

      mockedApiClient.listRepositories.mockResolvedValue(mockRepos);

      render(<GitIntegrationPage selectedTenant="default" />);

      await waitFor(() => {
        // Real URLs should be displayed as-is
        expect(screen.getByText('https://github.com/user/repo1.git')).toBeInTheDocument();
        expect(screen.getByText('https://gitlab.com/user/repo3.git')).toBeInTheDocument();

        // Repo ID fallbacks should be prefixed
        expect(screen.getByText('Repository: github.com/user/repo2')).toBeInTheDocument();
      });
    });

    it('handles empty repository list gracefully', async () => {
      mockedApiClient.listRepositories.mockResolvedValue([]);

      render(<GitIntegrationPage selectedTenant="default" />);

      await waitFor(() => {
        expect(screen.getByText('No repositories registered yet')).toBeInTheDocument();
      });
    });

    it('handles API errors gracefully', async () => {
      mockedApiClient.listRepositories.mockRejectedValue(new Error('API Error'));

      render(<GitIntegrationPage selectedTenant="default" />);

      await waitFor(() => {
        expect(screen.getByText('Failed to load repositories')).toBeInTheDocument();
      });
    });
  });
});
