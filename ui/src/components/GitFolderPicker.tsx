import React, { useState, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { Alert, AlertDescription } from './ui/alert';
import { Badge } from './ui/badge';
import { 
  FolderOpen, 
  GitBranch, 
  CheckCircle, 
  AlertTriangle, 
  Code, 
  FileText,
  Users,
  Clock,
  Database
} from 'lucide-react';
import { toast } from 'sonner';
import apiClient from '../api/client';
import * as types from '../api/types';

interface GitFolderPickerProps {
  onFolderSelect: (folderPath: string, repoInfo: GitRepositoryInfo) => void;
  onCancel: () => void;
}

interface GitRepositoryInfo {
  path: string;
  name: string;
  branch: string;
  commitCount: number;
  languages: string[];
  frameworks: string[];
  lastCommit: string; // Commit message, not timestamp
  isValid: boolean;
}

/**
 * Generate a deterministic repo_id from the path
 * 
 * Note: The backend uses this repo_id as the repository identifier.
 * While the backend generates a UUID for the database record, it uses
 * the client-provided repo_id for the actual repository identification.
 * This ensures deterministic, path-based identification.
 */
function generateRepoId(path: string): string {
  const normalizedPath = path.trim().replace(/\/$/, ''); // Remove trailing slash
  const parts = normalizedPath.split('/').filter(Boolean);
  const repoName = parts[parts.length - 1] || 'repository';
  // Use a hash-like approach for determinism (simple hash of path)
  const pathHash = normalizedPath.split('').reduce((acc, char) => {
    return ((acc << 5) - acc) + char.charCodeAt(0);
  }, 0);
  return `${repoName}-${Math.abs(pathHash).toString(36)}`;
}

/**
 * Validate repository path format and basic git repository indicators
 * 
 * Note: Actual filesystem validation (path existence, .git directory check)
 * happens on the backend. This provides client-side heuristics to catch
 * obvious issues before making the API call.
 */
function validatePathFormat(path: string): { valid: boolean; error?: string } {
  const trimmed = path.trim();
  
  if (!trimmed) {
    return { valid: false, error: 'Path cannot be empty' };
  }
  
  // Basic format validation
  if (trimmed.length < 2) {
    return { valid: false, error: 'Path is too short' };
  }
  
  // Check for common invalid characters (basic check)
  if (/[<>:"|?*\x00-\x1f]/.test(trimmed)) {
    return { valid: false, error: 'Path contains invalid characters' };
  }
  
  // Heuristic: Check if path might be a git repository
  // Note: This is a hint only - actual validation happens on backend
  // Backend will verify: 1) path exists, 2) path contains .git directory (line 186 in git_repository.rs)
  // We allow any path format here - backend handles actual filesystem validation
  
  return { valid: true };
}

export function GitFolderPicker({ onFolderSelect, onCancel }: GitFolderPickerProps) {
  const [folderPath, setFolderPath] = useState('');
  const [repoInfo, setRepoInfo] = useState<GitRepositoryInfo | null>(null);
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const analyzeRepository = useCallback(async (path: string) => {
    setIsAnalyzing(true);
    setError(null);
    
    try {
      // Validate path format before making API call
      const pathValidation = validatePathFormat(path);
      if (!pathValidation.valid) {
        throw new Error(pathValidation.error || 'Invalid path format');
      }
      
      const trimmedPath = path.trim();
      
      // Generate deterministic repo_id from path
      const repoId = generateRepoId(trimmedPath);
      const repoName = trimmedPath.split('/').filter(Boolean).pop() || 'repository';
      
      // Call the backend API to register and analyze the repository
      const response: types.RegisterGitRepositoryResponse = await apiClient.registerGitRepository({
        repo_id: repoId,
        path: trimmedPath,
      });
      
      // Validate response structure
      if (!response.analysis) {
        throw new Error('Invalid response: missing analysis data');
      }
      
      if (!response.analysis.git_info) {
        throw new Error('Invalid response: missing git_info data');
      }
      
      // Backend returns the repo_id we sent (it uses this as the repository identifier)
      // The backend generates a separate UUID for the database record (line 260 in git_repository.rs),
      // but uses our repo_id for the actual repository identification (line 309)
      const analysis = response.analysis;
      const gitInfo = analysis.git_info;
      
      // Validate required fields exist
      if (!gitInfo.branch) {
        throw new Error('Repository analysis incomplete: missing branch information');
      }
      
      if (typeof gitInfo.commit_count !== 'number') {
        throw new Error('Repository analysis incomplete: missing commit count');
      }
      
      if (!gitInfo.last_commit) {
        throw new Error('Repository analysis incomplete: missing last commit information');
      }
      
      if (!Array.isArray(analysis.languages)) {
        throw new Error('Repository analysis incomplete: missing language information');
      }
      
      if (!Array.isArray(analysis.frameworks)) {
        throw new Error('Repository analysis incomplete: missing framework information');
      }
      
      // Map the API response to GitRepositoryInfo (no fallbacks - use real data)
      const repoInfo: GitRepositoryInfo = {
        path: trimmedPath,
        name: repoName,
        branch: gitInfo.branch as string,
        commitCount: gitInfo.commit_count,
        languages: analysis.languages.map((lang: string) => lang),
        frameworks: analysis.frameworks.map((fw: string) => fw),
        // Note: last_commit is the commit message/summary, not a timestamp
        // The backend GitInfo struct (git_repository.rs:92-97) only provides
        // the commit message. If timestamp is needed, backend would need to be
        // updated to include last_commit_timestamp field.
        lastCommit: gitInfo.last_commit as string,
        isValid: response.status === 'synced',
      };
      
      setRepoInfo(repoInfo);
      toast.success('Repository analysis complete');
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to analyze repository';
      setError(errorMessage);
      toast.error(errorMessage);
    } finally {
      setIsAnalyzing(false);
    }
  }, []);

  const handlePathChange = (path: string) => {
    setFolderPath(path);
    setRepoInfo(null);
    setError(null);
  };

  const handleBrowseFolder = async () => {
    // Note: Browser security restrictions prevent direct access to file system paths.
    // The File System Access API provides directory handles but not full paths.
    // Users must enter the full path manually.
    const input = document.getElementById('folder-path') as HTMLInputElement;
    if (input) {
      input.focus();
      toast.info('Please enter the full path to your Git repository folder (e.g., /Users/username/projects/my-repo)');
    }
  };

  const handleAnalyze = () => {
    if (folderPath.trim()) {
      analyzeRepository(folderPath.trim());
    }
  };

  const handleConfirm = () => {
    if (repoInfo) {
      onFolderSelect(repoInfo.path, repoInfo);
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center space-x-2">
        <GitBranch className="h-6 w-6 text-primary" />
        <h2 className="text-xl font-semibold">Select Git Repository</h2>
      </div>

      <Alert>
        <CheckCircle className="h-4 w-4" />
        <AlertDescription>
          Select a local folder containing a Git repository (.git folder) to train codebase adapters.
          The system will analyze your codebase structure, languages, and frameworks.
        </AlertDescription>
      </Alert>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <FolderOpen className="mr-2 h-5 w-5" />
            Repository Path
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="folder-path">Folder Path</Label>
            <div className="flex space-x-2">
              <Input
                id="folder-path"
                value={folderPath}
                onChange={(e) => handlePathChange(e.target.value)}
                placeholder="/path/to/your/git/repository"
                className="flex-1"
              />
              <Button onClick={handleBrowseFolder} variant="outline">
                Browse
              </Button>
            </div>
          </div>

          <div className="flex space-x-2">
            <Button 
              onClick={handleAnalyze} 
              disabled={!folderPath.trim() || isAnalyzing}
              className="flex-1"
            >
              {isAnalyzing ? 'Analyzing...' : 'Analyze Repository'}
            </Button>
          </div>

          {error && (
            <Alert variant="destructive">
              <AlertTriangle className="h-4 w-4" />
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
        </CardContent>
      </Card>

      {repoInfo && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center">
              <CheckCircle className="mr-2 h-5 w-5 text-green-600" />
              Repository Analysis
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="grid grid-cols-2 gap-4">
              <div>
                <Label className="text-sm font-medium text-muted-foreground">Repository Name</Label>
                <p className="text-lg font-semibold">{repoInfo.name}</p>
              </div>
              <div>
                <Label className="text-sm font-medium text-muted-foreground">Branch</Label>
                <div className="flex items-center space-x-2">
                  <GitBranch className="h-4 w-4" />
                  <Badge variant="outline">{repoInfo.branch}</Badge>
                </div>
              </div>
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div>
                <Label className="text-sm font-medium text-muted-foreground">Commits</Label>
                <p className="text-lg">{repoInfo.commitCount.toLocaleString()}</p>
              </div>
              <div>
                <Label className="text-sm font-medium text-muted-foreground">Last Commit Message</Label>
                <p className="text-sm" title={repoInfo.lastCommit}>
                  {repoInfo.lastCommit.length > 50 
                    ? `${repoInfo.lastCommit.substring(0, 50)}...` 
                    : repoInfo.lastCommit}
                </p>
                <p className="text-xs text-muted-foreground mt-1">
                  Note: Commit timestamp not available from backend
                </p>
              </div>
            </div>

            <div>
              <Label className="text-sm font-medium text-muted-foreground">Languages Detected</Label>
              <div className="flex flex-wrap gap-2 mt-2">
                {repoInfo.languages.map((lang) => (
                  <Badge key={lang} variant="secondary" className="flex items-center space-x-1">
                    <Code className="h-3 w-3" />
                    <span>{lang}</span>
                  </Badge>
                ))}
              </div>
            </div>

            <div>
              <Label className="text-sm font-medium text-muted-foreground">Frameworks Detected</Label>
              <div className="flex flex-wrap gap-2 mt-2">
                {repoInfo.frameworks.map((framework) => (
                  <Badge key={framework} variant="outline" className="flex items-center space-x-1">
                    <Database className="h-3 w-3" />
                    <span>{framework}</span>
                  </Badge>
                ))}
              </div>
            </div>

            <Alert>
              <CheckCircle className="h-4 w-4" />
              <AlertDescription>
                This repository is ready for adapter training. The system will create codebase-specific 
                adapters that understand your project's patterns, conventions, and architecture.
              </AlertDescription>
            </Alert>
          </CardContent>
        </Card>
      )}

      <div className="flex justify-end space-x-2">
        <Button variant="outline" onClick={onCancel}>
          Cancel
        </Button>
        <Button 
          onClick={handleConfirm} 
          disabled={!repoInfo}
        >
          Use This Repository
        </Button>
      </div>
    </div>
  );
}
