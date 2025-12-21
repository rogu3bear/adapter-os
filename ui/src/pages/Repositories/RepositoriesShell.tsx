import React from 'react';
import { useParams } from 'react-router-dom';
import RepositoriesPage from './RepositoriesPage';
import RepoDetailPage from './RepoDetailPage';
import RepoVersionPage from './RepoVersionPage';

export default function RepositoriesShell() {
  const { repoId, versionId } = useParams<{ repoId?: string; versionId?: string }>();

  if (repoId && versionId) {
    return <RepoVersionPage />;
  }
  if (repoId) {
    return <RepoDetailPage />;
  }
  return <RepositoriesPage />;
}
