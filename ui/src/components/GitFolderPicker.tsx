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
  lastCommit: string;
  isValid: boolean;
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
      // Simulate repository analysis - in real implementation, this would call the backend
      await new Promise(resolve => setTimeout(resolve, 1500));
      
      // Mock analysis results
      const mockRepoInfo: GitRepositoryInfo = {
        path,
        name: path.split('/').pop() || 'unknown',
        branch: 'main',
        commitCount: Math.floor(Math.random() * 1000) + 100,
        languages: ['Rust', 'TypeScript', 'Python'],
        frameworks: ['React', 'Axum', 'Tokio'],
        lastCommit: new Date().toISOString(),
        isValid: true
      };
      
      setRepoInfo(mockRepoInfo);
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
    try {
      // In a real implementation, this would use the File System Access API or similar
      // For now, we'll simulate folder selection
      const mockPath = '/Users/star/Dev/my-awesome-project';
      setFolderPath(mockPath);
      await analyzeRepository(mockPath);
    } catch (err) {
      toast.error('Failed to browse folder');
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
                <Label className="text-sm font-medium text-muted-foreground">Last Commit</Label>
                <p className="text-sm">{new Date(repoInfo.lastCommit).toLocaleString()}</p>
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
