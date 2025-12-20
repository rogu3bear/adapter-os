import React, { useState, useCallback, useMemo } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Progress } from '@/components/ui/progress';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible';
import {
  CheckCircle2,
  AlertCircle,
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  Play,
  Loader2,
  FileText,
  Layers,
  Zap,
  Shield,
  TrendingUp,
  Info,
  X,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { apiClient } from '@/api/services';
import { Adapter } from '@/api/types';
import { formatBytes } from '@/lib/formatters';

interface StackAdapter {
  adapter: Adapter;
  order: number;
  enabled: boolean;
}

interface StackPreviewProps {
  adapters: StackAdapter[];
  stackName?: string;
  stackId?: string;
  onValidation?: (report: ValidationReport) => void;
  onTestInference?: (result: InferenceTestResult) => void;
}

interface ValidationIssue {
  level: 'error' | 'warning' | 'info';
  category: string;
  message: string;
  adapter?: string;
  suggestion?: string;
}

interface ValidationReport {
  isValid: boolean;
  issues: ValidationIssue[];
  summary: {
    totalAdapters: number;
    enabledAdapters: number;
    totalParameters: number;
    totalMemory: number;
    estimatedLatency: number;
    compatibilityScore: number;
  };
}

interface InferenceTestResult {
  success: boolean;
  prompt: string;
  output: string;
  latency: number;
  adaptersApplied: string[];
  error?: string;
}

// Validation rule sets
const validationRules = {
  frameworkCompatibility: (adapters: StackAdapter[]): ValidationIssue[] => {
    const issues: ValidationIssue[] = [];
    const frameworks = new Set<string>();

    adapters.forEach((item) => {
      if (item.adapter.framework) {
        frameworks.add(item.adapter.framework);
      }
    });

    if (frameworks.size > 1) {
      issues.push({
        level: 'warning',
        category: 'Framework Compatibility',
        message: `Stack uses multiple frameworks: ${Array.from(frameworks).join(', ')}`,
        suggestion:
          'Adapters trained on different frameworks may have reduced effectiveness when combined',
      });
    }

    return issues;
  },

  rankCompatibility: (adapters: StackAdapter[]): ValidationIssue[] => {
    const issues: ValidationIssue[] = [];
    const ranks = adapters
      .filter((item) => item.enabled)
      .map((item) => item.adapter.rank);

    if (ranks.length === 0) return issues;

    const minRank = Math.min(...ranks);
    const maxRank = Math.max(...ranks);
    const rankDiff = maxRank - minRank;

    if (rankDiff > 16) {
      issues.push({
        level: 'warning',
        category: 'Rank Compatibility',
        message: `Rank variance is ${rankDiff} (min: ${minRank}, max: ${maxRank})`,
        suggestion: 'Consider using adapters with similar ranks for better compatibility',
      });
    }

    return issues;
  },

  tierAlignment: (adapters: StackAdapter[]): ValidationIssue[] => {
    const issues: ValidationIssue[] = [];
    const tiers = adapters
      .filter((item) => item.enabled)
      .map((item) => item.adapter.tier)
      .filter((tier) => tier !== undefined) as string[];

    if (tiers.length === 0) return issues;

    const uniqueTiers = new Set(tiers);

    if (uniqueTiers.size > 1) {
      issues.push({
        level: 'info',
        category: 'Tier Alignment',
        message: `Stack contains adapters from different tiers (${Array.from(uniqueTiers).join(', ')})`,
        suggestion: 'Mixing storage tiers is allowed but may affect memory efficiency',
      });
    }

    return issues;
  },

  semanticNaming: (adapters: StackAdapter[], stackName?: string): ValidationIssue[] => {
    const issues: ValidationIssue[] = [];

    // Stack naming: should follow format and not use reserved words
    const reservedTenants = [
      'system',
      'admin',
      'root',
      'default',
      'test',
    ];
    const reservedDomains = ['core', 'internal', 'deprecated'];

    if (stackName) {
      const nameParts = stackName.split('/');

      if (nameParts.length > 0 && reservedTenants.includes(nameParts[0])) {
        issues.push({
          level: 'error',
          category: 'Semantic Naming',
          message: `Stack name uses reserved tenant: "${nameParts[0]}"`,
          suggestion: `Use a valid tenant name instead of: ${reservedTenants.join(', ')}`,
        });
      }

      if (
        nameParts.length > 1 &&
        reservedDomains.includes(nameParts[1])
      ) {
        issues.push({
          level: 'error',
          category: 'Semantic Naming',
          message: `Stack name uses reserved domain: "${nameParts[1]}"`,
          suggestion: `Use a domain name instead of: ${reservedDomains.join(', ')}`,
        });
      }

      // Validate adapter semantic naming
      adapters.forEach((item) => {
        const adapterNameParts = item.adapter.name.split('/');
        if (adapterNameParts.length < 4) {
          issues.push({
            level: 'warning',
            category: 'Semantic Naming',
            message: `Adapter "${item.adapter.name}" doesn't follow semantic naming format`,
            adapter: item.adapter.name,
            suggestion:
              'Use format: {tenant}/{domain}/{purpose}/{revision} e.g., tenant-a/engineering/code-review/r001',
          });
        }
      });
    }

    return issues;
  },

  routerCompliance: (adapters: StackAdapter[]): ValidationIssue[] => {
    const issues: ValidationIssue[] = [];

    const enabledAdapters = adapters.filter((item) => item.enabled);

    if (enabledAdapters.length > 10) {
      issues.push({
        level: 'warning',
        category: 'Router Compliance',
        message: `Stack has ${enabledAdapters.length} adapters (recommended max: 10 for K-sparse routing)`,
        suggestion: 'Consider reducing stack size for better router performance',
      });
    }

    if (enabledAdapters.length === 0) {
      issues.push({
        level: 'error',
        category: 'Router Compliance',
        message: 'No adapters are enabled in the stack',
        suggestion: 'Enable at least one adapter to create a valid stack',
      });
    }

    return issues;
  },

  policyCompliance: (adapters: StackAdapter[]): ValidationIssue[] => {
    const issues: ValidationIssue[] = [];

    // Check for adapter activation percentages
    adapters.forEach((item) => {
      if (item.adapter.activation_count === 0) {
        issues.push({
          level: 'info',
          category: 'Policy Compliance',
          message: `Adapter "${item.adapter.name}" has no activation history`,
          adapter: item.adapter.name,
          suggestion:
            'Consider testing the adapter before adding it to production stacks',
        });
      }
    });

    // Check for deprecated adapters
    adapters.forEach((item) => {
      if (item.adapter.lifecycle_state === 'deprecated') {
        issues.push({
          level: 'warning',
          category: 'Policy Compliance',
          message: `Adapter "${item.adapter.name}" is marked as deprecated`,
          adapter: item.adapter.name,
          suggestion: 'Use an active adapter instead of deprecated ones',
        });
      }
    });

    // Check for retired adapters
    adapters.forEach((item) => {
      if (item.adapter.lifecycle_state === 'retired') {
        issues.push({
          level: 'error',
          category: 'Policy Compliance',
          message: `Adapter "${item.adapter.name}" is retired and cannot be used`,
          adapter: item.adapter.name,
          suggestion: 'Remove this adapter from the stack',
        });
      }
    });

    return issues;
  },
};

const calculateStackMetrics = (adapters: StackAdapter[]) => {
  const enabledAdapters = adapters.filter((item) => item.enabled);

  const totalParameters = enabledAdapters.reduce(
    (sum, item) => sum + (item.adapter.rank || 0) * 1000,
    0
  ); // Rough estimation
  const totalMemory = enabledAdapters.reduce(
    (sum, item) => sum + (item.adapter.memory_bytes || 0),
    0
  );
  const estimatedLatency = enabledAdapters.length * 2.5 + 5; // ms per adapter + base

  // Calculate compatibility score (0-100)
  let compatScore = 100;
  if (enabledAdapters.length === 0) compatScore = 0;
  if (
    enabledAdapters.some((item) =>
      item.adapter.lifecycle_state?.includes('retired')
    )
  )
    compatScore -= 50;
  if (
    enabledAdapters.some((item) =>
      item.adapter.lifecycle_state?.includes('deprecated')
    )
  )
    compatScore -= 20;
  if (enabledAdapters.length > 10) compatScore -= 15;

  return {
    totalAdapters: adapters.length,
    enabledAdapters: enabledAdapters.length,
    totalParameters: Math.floor(totalParameters),
    totalMemory: Math.floor(totalMemory),
    estimatedLatency: Math.round(estimatedLatency * 10) / 10,
    compatibilityScore: Math.max(0, Math.min(100, compatScore)),
  };
};

const ValidationIssueCard: React.FC<{
  issue: ValidationIssue;
  onDismiss?: () => void;
}> = ({ issue, onDismiss }) => {
  const Icon =
    issue.level === 'error'
      ? AlertCircle
      : issue.level === 'warning'
        ? AlertTriangle
        : Info;

  const bgColor =
    issue.level === 'error'
      ? 'bg-red-500/10 border-red-500/20'
      : issue.level === 'warning'
        ? 'bg-yellow-500/10 border-yellow-500/20'
        : 'bg-blue-500/10 border-blue-500/20';

  const textColor =
    issue.level === 'error'
      ? 'text-red-700'
      : issue.level === 'warning'
        ? 'text-yellow-700'
        : 'text-blue-700';

  return (
    <div className={cn('border rounded-lg p-4', bgColor)}>
      <div className="flex gap-3">
        <Icon className={cn('h-5 w-5 flex-shrink-0 mt-0.5', textColor)} />
        <div className="flex-1">
          <div className="flex items-start justify-between gap-2">
            <div>
              <p className={cn('font-medium', textColor)}>
                {issue.category}
              </p>
              <p className="text-sm text-muted-foreground mt-1">
                {issue.message}
              </p>
              {issue.suggestion && (
                <p className="text-xs text-muted-foreground mt-2 p-2 bg-white/50 rounded">
                  Suggestion: {issue.suggestion}
                </p>
              )}
            </div>
            {onDismiss && (
              <button
                onClick={onDismiss}
                className="text-muted-foreground hover:text-foreground"
              >
                <X className="h-4 w-4" />
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export const StackPreview: React.FC<StackPreviewProps> = ({
  adapters,
  stackName,
  stackId,
  onValidation,
  onTestInference,
}) => {
  const [expandedSections, setExpandedSections] = useState<
    Record<string, boolean>
  >({
    compatibility: true,
    policies: true,
    metrics: false,
    testing: false,
  });

  const [testPrompt, setTestPrompt] = useState('');
  const [isTestingInference, setIsTestingInference] = useState(false);
  const [testResult, setTestResult] = useState<InferenceTestResult | null>(
    null
  );
  const [dismissedIssues, setDismissedIssues] = useState<Set<string>>(
    new Set()
  );

  // Run all validations
  const validationReport = useMemo(() => {
    const allIssues: ValidationIssue[] = [];

    allIssues.push(
      ...validationRules.frameworkCompatibility(adapters)
    );
    allIssues.push(...validationRules.rankCompatibility(adapters));
    allIssues.push(...validationRules.tierAlignment(adapters));
    allIssues.push(
      ...validationRules.semanticNaming(adapters, stackName)
    );
    allIssues.push(...validationRules.routerCompliance(adapters));
    allIssues.push(...validationRules.policyCompliance(adapters));

    const filteredIssues = allIssues.filter(
      (issue, idx) => !dismissedIssues.has(`${idx}-${issue.message}`)
    );

    const errorCount = filteredIssues.filter(
      (i) => i.level === 'error'
    ).length;

    const report: ValidationReport = {
      isValid: errorCount === 0,
      issues: filteredIssues,
      summary: calculateStackMetrics(adapters),
    };

    return report;
  }, [adapters, stackName, dismissedIssues]);

  // Notify parent of validation changes
  React.useEffect(() => {
    if (onValidation) {
      onValidation(validationReport);
    }
  }, [validationReport, onValidation]);

  const toggleSection = (section: string) => {
    setExpandedSections((prev) => ({
      ...prev,
      [section]: !prev[section],
    }));
  };

  const dismissIssue = (issue: ValidationIssue) => {
    const key = `${issue.category}-${issue.message}`;
    setDismissedIssues((prev) => new Set(prev).add(key));
  };

  const handleTestInference = async () => {
    if (!testPrompt.trim()) return;

    setIsTestingInference(true);
    try {
      const response = await apiClient.request<{ data: { output?: string; latency_ms?: number; adapters_applied?: string[] } }>(
        '/api/inference/test',
        {
          method: 'POST',
          body: JSON.stringify({
            prompt: testPrompt,
            adapter_ids: adapters
              .filter((item) => item.enabled)
              .map((item) => item.adapter.adapter_id),
            stack_id: stackId,
          }),
        }
      );

      const result: InferenceTestResult = {
        success: true,
        prompt: testPrompt,
        output: response.data.output || '',
        latency: response.data.latency_ms || 0,
        adaptersApplied: response.data.adapters_applied || [],
      };

      setTestResult(result);

      if (onTestInference) {
        onTestInference(result);
      }
    } catch (error: unknown) {
      const result: InferenceTestResult = {
        success: false,
        prompt: testPrompt,
        output: '',
        latency: 0,
        adaptersApplied: [],
        error: error instanceof Error ? error.message : 'Inference test failed',
      };

      setTestResult(result);
    } finally {
      setIsTestingInference(false);
    }
  };

  const errorIssues = validationReport.issues.filter(
    (i) => i.level === 'error'
  );
  const warningIssues = validationReport.issues.filter(
    (i) => i.level === 'warning'
  );
  const infoIssues = validationReport.issues.filter(
    (i) => i.level === 'info'
  );

  return (
    <div className="space-y-4">
      {/* Status Header */}
      <Card className="border-0 bg-gradient-to-r from-blue-50 to-indigo-50">
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              {validationReport.isValid ? (
                <CheckCircle2 className="h-6 w-6 text-green-600" />
              ) : (
                <AlertCircle className="h-6 w-6 text-red-600" />
              )}
              <div>
                <CardTitle className="text-lg">
                  {validationReport.isValid
                    ? 'Stack is Valid'
                    : 'Stack has Issues'}
                </CardTitle>
                <CardDescription>
                  {validationReport.summary.enabledAdapters} adapters enabled
                  {errorIssues.length > 0 &&
                    ` • ${errorIssues.length} error(s)`}
                  {warningIssues.length > 0 &&
                    ` • ${warningIssues.length} warning(s)`}
                </CardDescription>
              </div>
            </div>

            <div className="text-right">
              <div className="text-2xl font-bold text-indigo-600">
                {validationReport.summary.compatibilityScore}%
              </div>
              <div className="text-xs text-muted-foreground">
                Compatibility
              </div>
            </div>
          </div>
        </CardHeader>
      </Card>

      {/* Error Issues (if any) - Always expanded */}
      {errorIssues.length > 0 && (
        <Card className="border-red-200 bg-red-50/50">
          <CardHeader className="pb-3">
            <CardTitle className="text-base flex items-center gap-2 text-red-700">
              <AlertCircle className="h-5 w-5" />
              Blocking Issues
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            {errorIssues.map((issue, idx) => (
              <ValidationIssueCard
                key={`error-${idx}`}
                issue={issue}
                onDismiss={() => dismissIssue(issue)}
              />
            ))}
          </CardContent>
        </Card>
      )}

      {/* Compatibility Validation */}
      <Collapsible
        open={expandedSections.compatibility}
        onOpenChange={() => toggleSection('compatibility')}
      >
        <Card>
          <CollapsibleTrigger className="w-full">
            <CardHeader className="pb-3 hover:bg-muted/50 transition-colors">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  {warningIssues.length > 0 ? (
                    <AlertTriangle className="h-5 w-5 text-yellow-600" />
                  ) : (
                    <CheckCircle2 className="h-5 w-5 text-green-600" />
                  )}
                  <CardTitle className="text-base">
                    Compatibility Checks
                  </CardTitle>
                  {warningIssues.length > 0 && (
                    <Badge variant="outline" className="ml-2">
                      {warningIssues.length} warning(s)
                    </Badge>
                  )}
                </div>
                {expandedSections.compatibility ? (
                  <ChevronDown className="h-4 w-4" />
                ) : (
                  <ChevronRight className="h-4 w-4" />
                )}
              </div>
            </CardHeader>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <CardContent className="space-y-3 pt-0">
              {warningIssues.length === 0 ? (
                <div className="flex items-center gap-2 text-sm text-green-700 bg-green-50 p-3 rounded">
                  <CheckCircle2 className="h-4 w-4" />
                  All compatibility checks passed
                </div>
              ) : (
                warningIssues.map((issue, idx) => (
                  <ValidationIssueCard
                    key={`warning-${idx}`}
                    issue={issue}
                    onDismiss={() => dismissIssue(issue)}
                  />
                ))
              )}
            </CardContent>
          </CollapsibleContent>
        </Card>
      </Collapsible>

      {/* Policy Validation */}
      <Collapsible
        open={expandedSections.policies}
        onOpenChange={() => toggleSection('policies')}
      >
        <Card>
          <CollapsibleTrigger className="w-full">
            <CardHeader className="pb-3 hover:bg-muted/50 transition-colors">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <Shield className="h-5 w-5 text-blue-600" />
                  <CardTitle className="text-base">
                    Policy Validation
                  </CardTitle>
                  {infoIssues.length > 0 && (
                    <Badge variant="outline" className="ml-2">
                      {infoIssues.length} info
                    </Badge>
                  )}
                </div>
                {expandedSections.policies ? (
                  <ChevronDown className="h-4 w-4" />
                ) : (
                  <ChevronRight className="h-4 w-4" />
                )}
              </div>
            </CardHeader>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <CardContent className="space-y-3 pt-0">
              {infoIssues.length === 0 ? (
                <div className="flex items-center gap-2 text-sm text-blue-700 bg-blue-50 p-3 rounded">
                  <CheckCircle2 className="h-4 w-4" />
                  All policies are compliant
                </div>
              ) : (
                infoIssues.map((issue, idx) => (
                  <ValidationIssueCard
                    key={`info-${idx}`}
                    issue={issue}
                    onDismiss={() => dismissIssue(issue)}
                  />
                ))
              )}
            </CardContent>
          </CollapsibleContent>
        </Card>
      </Collapsible>

      {/* Stack Metrics */}
      <Collapsible
        open={expandedSections.metrics}
        onOpenChange={() => toggleSection('metrics')}
      >
        <Card>
          <CollapsibleTrigger className="w-full">
            <CardHeader className="pb-3 hover:bg-muted/50 transition-colors">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <TrendingUp className="h-5 w-5 text-purple-600" />
                  <CardTitle className="text-base">Stack Metrics</CardTitle>
                </div>
                {expandedSections.metrics ? (
                  <ChevronDown className="h-4 w-4" />
                ) : (
                  <ChevronRight className="h-4 w-4" />
                )}
              </div>
            </CardHeader>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <CardContent className="space-y-4 pt-0">
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label className="text-xs text-muted-foreground">
                    Total Adapters
                  </Label>
                  <p className="text-2xl font-bold">
                    {validationReport.summary.totalAdapters}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    {validationReport.summary.enabledAdapters} enabled
                  </p>
                </div>

                <div>
                  <Label className="text-xs text-muted-foreground">
                    Total Memory
                  </Label>
                  <p className="text-2xl font-bold">
                    {formatBytes(
                      validationReport.summary.totalMemory
                    )}
                  </p>
                </div>

                <div>
                  <Label className="text-xs text-muted-foreground">
                    Est. Parameters
                  </Label>
                  <p className="text-2xl font-bold">
                    {validationReport.summary.totalParameters.toLocaleString()}
                  </p>
                </div>

                <div>
                  <Label className="text-xs text-muted-foreground">
                    Est. Latency
                  </Label>
                  <p className="text-2xl font-bold">
                    {validationReport.summary.estimatedLatency}ms
                  </p>
                </div>
              </div>

              <div>
                <Label className="text-xs text-muted-foreground">
                  Adapter Execution Order
                </Label>
                <div className="mt-2 space-y-2">
                  {adapters
                    .filter((item) => item.enabled)
                    .sort((a, b) => a.order - b.order)
                    .map((item, idx) => (
                      <div
                        key={item.adapter.adapter_id}
                        className="flex items-center gap-2 text-sm p-2 bg-muted/50 rounded"
                      >
                        <span className="font-bold text-muted-foreground">
                          {idx + 1}
                        </span>
                        <span className="font-medium">
                          {item.adapter.name}
                        </span>
                        {item.adapter.version && (
                          <Badge variant="outline" className="text-[10px]">
                            v{item.adapter.version}
                          </Badge>
                        )}
                        {item.adapter.hash_b3 && (
                          <Badge variant="secondary" className="text-[10px]">
                            b3 {item.adapter.hash_b3.slice(0, 8)}…
                          </Badge>
                        )}
                        <span className="text-xs text-muted-foreground ml-auto">
                          rank:{item.adapter.rank} tier:{item.adapter.tier}
                        </span>
                      </div>
                    ))}
                </div>
              </div>
            </CardContent>
          </CollapsibleContent>
        </Card>
      </Collapsible>

      {/* Testing Interface */}
      <Collapsible
        open={expandedSections.testing}
        onOpenChange={() => toggleSection('testing')}
      >
        <Card>
          <CollapsibleTrigger className="w-full">
            <CardHeader className="pb-3 hover:bg-muted/50 transition-colors">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <Zap className="h-5 w-5 text-orange-600" />
                  <CardTitle className="text-base">
                    Test Inference
                  </CardTitle>
                </div>
                {expandedSections.testing ? (
                  <ChevronDown className="h-4 w-4" />
                ) : (
                  <ChevronRight className="h-4 w-4" />
                )}
              </div>
            </CardHeader>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <CardContent className="space-y-4 pt-0">
              <div>
                <Label htmlFor="test-prompt" className="text-sm">
                  Test Prompt
                </Label>
                <textarea
                  id="test-prompt"
                  placeholder="Enter a test prompt to validate stack inference..."
                  value={testPrompt}
                  onChange={(e) => setTestPrompt(e.target.value)}
                  className="w-full p-3 border rounded-md bg-background text-sm font-mono resize-none"
                  rows={4}
                  disabled={isTestingInference}
                />
              </div>

              <Button
                onClick={handleTestInference}
                disabled={
                  !testPrompt.trim() ||
                  !validationReport.isValid ||
                  isTestingInference
                }
                className="w-full"
              >
                {isTestingInference && (
                  <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                )}
                {isTestingInference
                  ? 'Running Inference...'
                  : 'Test with Stack'}
              </Button>

              {testResult && (
                <div
                  className={cn(
                    'border rounded-lg p-4',
                    testResult.success
                      ? 'bg-green-50 border-green-200'
                      : 'bg-red-50 border-red-200'
                  )}
                >
                  {testResult.success ? (
                    <>
                      <div className="flex items-center gap-2 mb-3">
                        <CheckCircle2 className="h-5 w-5 text-green-600" />
                        <p className="font-medium text-green-700">
                          Inference successful
                        </p>
                      </div>

                      <div className="space-y-3 text-sm">
                        <div>
                          <p className="text-xs text-muted-foreground font-medium">
                            Output:
                          </p>
                          <p className="mt-1 p-2 bg-white rounded font-mono text-xs">
                            {testResult.output}
                          </p>
                        </div>

                        <div className="grid grid-cols-2 gap-2 text-xs">
                          <div className="p-2 bg-white rounded">
                            <p className="text-muted-foreground">Latency</p>
                            <p className="font-bold">
                              {testResult.latency}ms
                            </p>
                          </div>
                          <div className="p-2 bg-white rounded">
                            <p className="text-muted-foreground">
                              Adapters Applied
                            </p>
                            <p className="font-bold">
                              {testResult.adaptersApplied.length}
                            </p>
                          </div>
                        </div>
                      </div>
                    </>
                  ) : (
                    <>
                      <div className="flex items-center gap-2 mb-3">
                        <AlertCircle className="h-5 w-5 text-red-600" />
                        <p className="font-medium text-red-700">
                          Inference failed
                        </p>
                      </div>
                      <p className="text-sm text-red-600">
                        {testResult.error}
                      </p>
                    </>
                  )}
                </div>
              )}
            </CardContent>
          </CollapsibleContent>
        </Card>
      </Collapsible>

      {/* Adapter List */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base flex items-center gap-2">
            <Layers className="h-5 w-5" />
            Stack Composition
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-2">
            {adapters.length === 0 ? (
              <p className="text-sm text-muted-foreground text-center py-4">
                No adapters in stack
              </p>
            ) : (
              adapters.map((item) => (
                <div
                  key={item.adapter.adapter_id}
                  className={cn(
                    'flex items-center justify-between p-3 border rounded-lg',
                    !item.enabled && 'opacity-50 bg-muted/50'
                  )}
                >
                  <div className="flex-1">
                    <p className="font-medium text-sm">
                      {item.order}. {item.adapter.name}
                    </p>
                    <div className="flex flex-wrap gap-2 text-[11px] text-muted-foreground mt-1">
                      {item.adapter.version && (
                        <Badge variant="outline">v{item.adapter.version}</Badge>
                      )}
                      {item.adapter.hash_b3 && (
                        <Badge variant="secondary">b3 {item.adapter.hash_b3.slice(0, 8)}…</Badge>
                      )}
                      <span className="truncate max-w-[220px]">{item.adapter.adapter_id}</span>
                    </div>
                    <p className="text-xs text-muted-foreground mt-1">
                      Rank: {item.adapter.rank} | Tier:{' '}
                      {item.adapter.tier} | State:{' '}
                      {item.adapter.current_state || 'unknown'}
                    </p>
                  </div>
                  {!item.enabled && (
                    <Badge variant="secondary">Disabled</Badge>
                  )}
                </div>
              ))
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
};

export type { ValidationReport, ValidationIssue, InferenceTestResult };
