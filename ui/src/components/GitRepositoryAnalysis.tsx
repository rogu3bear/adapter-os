import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from './ui/card';
import { Button } from './ui/button';
import { Badge } from './ui/badge';
import { Progress } from './ui/progress';
import { Alert, AlertDescription } from './ui/alert';
import { 
  GitBranch, 
  Code, 
  Database, 
  Target, 
  FileText, 
  Users, 
  Clock,
  CheckCircle,
  AlertTriangle,
  Brain,
  Layers,
  Zap,
  Eye,
  Download,
  Upload,
  Shield,
  Activity
} from 'lucide-react';
import apiClient from '@/api/client';

interface GitRepositoryAnalysisProps {
  repositoryId: string;
  onTrainingStart?: (config: Record<string, unknown>) => void;
}

interface RepositoryAnalysis {
  repo_id: string;
  languages: LanguageInfo[];
  frameworks: FrameworkInfo[];
  security_scan: SecurityScanResult;
  git_info: GitInfo;
  evidence_spans: EvidenceSpan[];
}

interface LanguageInfo {
  name: string;
  files: number;
  lines: number;
  percentage: number;
}

interface FrameworkInfo {
  name: string;
  version?: string;
  confidence: number;
  files: string[];
}

interface SecurityScanResult {
  violations: SecurityViolation[];
  scan_timestamp: string;
  status: string;
}

interface SecurityViolation {
  file_path: string;
  pattern: string;
  line_number?: number;
  severity: string;
}

interface GitInfo {
  branch: string;
  commit_count: number;
  last_commit: string;
  authors: string[];
}

interface EvidenceSpan {
  span_id: string;
  evidence_type: string;
  file_path: string;
  line_range: [number, number];
  relevance_score: number;
  content: string;
}

export function GitRepositoryAnalysis({ repositoryId, onTrainingStart }: GitRepositoryAnalysisProps) {
  const [analysis, setAnalysis] = useState<RepositoryAnalysis | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchAnalysis = async () => {
      try {
        // Evidence: ui/src/components/CodeIntelligence.tsx:42-58
        // Pattern: Data fetching with error handling
        const data = await apiClient.getRepositoryAnalysis(repositoryId);
        setAnalysis(data as RepositoryAnalysis);
      } catch (err) {
        const errorMessage = err instanceof Error ? err.message : 'Failed to fetch repository analysis';
        setError(errorMessage);
      } finally {
        setLoading(false);
      }
    };

    fetchAnalysis();
  }, [repositoryId]);

  if (loading) {
    return (
      <div className="text-center p-8">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary mx-auto mb-4"></div>
        <p className="text-muted-foreground">Analyzing repository...</p>
      </div>
    );
  }

  if (error) {
    return (
      <Alert variant="destructive">
        <AlertTriangle className="h-4 w-4" />
        <AlertDescription>{error}</AlertDescription>
      </Alert>
    );
  }

  if (!analysis) {
    return (
      <Alert>
        <AlertTriangle className="h-4 w-4" />
        <AlertDescription>No analysis data available</AlertDescription>
      </Alert>
    );
  }

  return (
    <div className="space-y-6">
      {/* Repository Header */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <GitBranch className="mr-2 h-5 w-5" />
            Repository Analysis: {analysis.repo_id}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 gap-4">
            <div>
              <div className="text-sm font-medium text-muted-foreground">Branch</div>
              <div className="flex items-center space-x-2">
                <GitBranch className="h-4 w-4" />
                <Badge variant="outline">{analysis.git_info.branch}</Badge>
              </div>
            </div>
            <div>
              <div className="text-sm font-medium text-muted-foreground">Commits</div>
              <div className="text-lg font-semibold">{analysis.git_info.commit_count.toLocaleString()}</div>
            </div>
            <div>
              <div className="text-sm font-medium text-muted-foreground">Last Commit</div>
              <div className="text-sm">{new Date(analysis.git_info.last_commit).toLocaleString()}</div>
            </div>
            <div>
              <div className="text-sm font-medium text-muted-foreground">Authors</div>
              <div className="text-sm">{analysis.git_info.authors.length} contributors</div>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Languages Detection */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <Code className="mr-2 h-5 w-5" />
            Languages Detected
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            {analysis.languages.map((lang) => (
              <div key={lang.name} className="space-y-2">
                <div className="flex justify-between items-center">
                  <div className="flex items-center space-x-2">
                    <Code className="h-4 w-4" />
                    <span className="font-medium">{lang.name}</span>
                  </div>
                  <div className="text-sm text-muted-foreground">
                    {lang.files} files, {lang.lines.toLocaleString()} lines
                  </div>
                </div>
                <Progress value={lang.percentage} className="h-2" />
                <div className="text-xs text-muted-foreground">
                  {lang.percentage.toFixed(1)}% of codebase
                </div>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      {/* Frameworks Detection */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <Layers className="mr-2 h-5 w-5" />
            Frameworks Detected
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {analysis.frameworks.map((framework) => (
              <div key={framework.name} className="border rounded-lg p-4">
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center space-x-2">
                    <Database className="h-4 w-4" />
                    <span className="font-medium">{framework.name}</span>
                    {framework.version && (
                      <Badge variant="outline">{framework.version}</Badge>
                    )}
                  </div>
                  <div className="text-sm text-muted-foreground">
                    {(framework.confidence * 100).toFixed(0)}% confidence
                  </div>
                </div>
                <div className="text-sm text-muted-foreground">
                  {framework.files.length} files detected
                </div>
                <Progress value={framework.confidence * 100} className="h-1 mt-2" />
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      {/* Security Scan Results */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <Shield className="mr-2 h-5 w-5" />
            Security Scan Results
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            <div className="flex items-center space-x-2">
              <Badge variant={analysis.security_scan.violations.length === 0 ? "default" : "destructive"}>
                {analysis.security_scan.status}
              </Badge>
              <span className="text-sm text-muted-foreground">
                {analysis.security_scan.violations.length} violations found
              </span>
            </div>
            
            {analysis.security_scan.violations.length > 0 && (
              <div className="space-y-2">
                <div className="text-sm font-medium">Security Violations:</div>
                {analysis.security_scan.violations.map((violation, index) => (
                  <div key={index} className="border rounded p-3 bg-red-50">
                    <div className="flex items-center justify-between">
                      <div className="font-medium text-sm">{violation.file_path}</div>
                      <Badge variant="destructive" className="text-xs">
                        {violation.severity}
                      </Badge>
                    </div>
                    <div className="text-xs text-muted-foreground mt-1">
                      Pattern: {violation.pattern}
                      {violation.line_number && ` (Line ${violation.line_number})`}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Evidence Spans */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <Brain className="mr-2 h-5 w-5" />
            Evidence Spans ({analysis.evidence_spans.length})
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-3">
            {analysis.evidence_spans.slice(0, 5).map((span) => (
              <div key={span.span_id} className="border rounded p-3">
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center space-x-2">
                    <FileText className="h-4 w-4" />
                    <span className="font-medium text-sm">{span.file_path}</span>
                  </div>
                  <div className="text-xs text-muted-foreground">
                    Lines {span.line_range[0]}-{span.line_range[1]}
                  </div>
                </div>
                <div className="text-sm text-muted-foreground mb-2">
                  {span.content.substring(0, 200)}...
                </div>
                <div className="flex items-center justify-between">
                  <Badge variant="outline" className="text-xs">
                    {span.evidence_type}
                  </Badge>
                  <div className="text-xs text-muted-foreground">
                    {(span.relevance_score * 100).toFixed(0)}% relevance
                  </div>
                </div>
              </div>
            ))}
            {analysis.evidence_spans.length > 5 && (
              <div className="text-center text-sm text-muted-foreground">
                ... and {analysis.evidence_spans.length - 5} more evidence spans
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Training Actions */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center">
            <Zap className="mr-2 h-5 w-5" />
            Adapter Training
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-4">
            <Alert>
              <CheckCircle className="h-4 w-4" />
              <AlertDescription>
                This repository is ready for adapter training. The system will create codebase-specific 
                adapters that understand your project's patterns, conventions, and architecture.
              </AlertDescription>
            </Alert>
            
            <div className="flex space-x-2">
              <Button 
                onClick={() => onTrainingStart?.({
                  category: 'codebase',
                  scope: 'repo',
                  rank: 24,
                  alpha: 48,
                  epochs: 3,
                  learning_rate: 0.001,
                  batch_size: 32,
                  targets: ['q_proj', 'k_proj', 'v_proj', 'o_proj', 'gate_proj', 'up_proj', 'down_proj']
                })}
                className="flex-1"
              >
                <Zap className="mr-2 h-4 w-4" />
                Start Training
              </Button>
              <Button variant="outline">
                <Eye className="mr-2 h-4 w-4" />
                View Details
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
