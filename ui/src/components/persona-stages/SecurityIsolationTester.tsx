import React, { useState, useCallback } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Shield,
  Play,
  CheckCircle,
  XCircle,
  Clock,
  Lock,
  Network,
  HardDrive,
  Users,
  RefreshCw,
  AlertTriangle,
} from 'lucide-react';
import { apiClient } from '@/api/services';
import { IsolationTestResult, IsolationTestScenario } from '@/api/types';
import { toast } from 'sonner';
import { logger } from '@/utils/logger';

interface SecurityIsolationTesterProps {
  tenantId?: string;
}

// Predefined test scenarios
const testScenarios: IsolationTestScenario[] = [
  {
    id: 'tenant-boundary',
    name: 'Organization Boundary Test',
    description: 'Verify that organization data and resources are properly isolated',
    category: 'tenant',
  },
  {
    id: 'cross-tenant-access',
    name: 'Cross-Tenant Access Test',
    description: 'Attempt to access another tenant\'s adapters and data',
    category: 'tenant',
  },
  {
    id: 'memory-isolation',
    name: 'Memory Isolation Test',
    description: 'Verify GPU memory isolation between tenants',
    category: 'memory',
  },
  {
    id: 'memory-overflow',
    name: 'Memory Overflow Test',
    description: 'Test that memory overflow does not affect other tenants',
    category: 'memory',
  },
  {
    id: 'network-egress',
    name: 'Network Egress Test',
    description: 'Verify that network egress is blocked in production mode',
    category: 'network',
  },
  {
    id: 'uds-only',
    name: 'UDS-Only Communication Test',
    description: 'Verify all communication uses Unix domain sockets',
    category: 'network',
  },
  {
    id: 'filesystem-isolation',
    name: 'Filesystem Isolation Test',
    description: 'Verify tenant filesystem boundaries are enforced',
    category: 'filesystem',
  },
  {
    id: 'uid-gid-enforcement',
    name: 'UID/GID Enforcement Test',
    description: 'Verify process-level isolation using UID/GID',
    category: 'filesystem',
  },
];

export default function SecurityIsolationTester({ tenantId = 'default' }: SecurityIsolationTesterProps) {
  const [selectedScenario, setSelectedScenario] = useState<string>('');
  const [isRunning, setIsRunning] = useState(false);
  const [results, setResults] = useState<IsolationTestResult[]>([]);
  const [policyCompliance, setPolicyCompliance] = useState<{
    egress: boolean;
    determinism: boolean;
    isolation: boolean;
  }>({
    egress: true,
    determinism: true,
    isolation: true,
  });

  const runTest = useCallback(async () => {
    if (!selectedScenario) {
      toast.error('Please select a test scenario');
      return;
    }

    setIsRunning(true);
    try {
      logger.info('Running isolation test', {
        component: 'SecurityIsolationTester',
        operation: 'runTest',
        scenarioId: selectedScenario,
        tenantId,
      });

      const result = await apiClient.runIsolationTest(selectedScenario, tenantId);

      setResults((prev) => [result, ...prev]);

      // Update policy compliance based on test results
      const scenario = testScenarios.find((s) => s.id === selectedScenario);
      if (scenario) {
        if (scenario.category === 'network' && !result.passed) {
          setPolicyCompliance((prev) => ({ ...prev, egress: false }));
        }
        if (scenario.category === 'tenant' && !result.passed) {
          setPolicyCompliance((prev) => ({ ...prev, isolation: false }));
        }
      }

      if (result.passed) {
        toast.success(`Test passed: ${scenario?.name}`);
      } else {
        toast.error(`Test failed: ${result.message}`);
      }

      logger.info('Isolation test completed', {
        component: 'SecurityIsolationTester',
        operation: 'runTest',
        scenarioId: selectedScenario,
        passed: result.passed,
        duration_ms: result.duration_ms,
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Test execution failed';
      logger.error('Isolation test failed', {
        component: 'SecurityIsolationTester',
        operation: 'runTest',
        scenarioId: selectedScenario,
        error: errorMessage,
      });
      toast.error(errorMessage);
    } finally {
      setIsRunning(false);
    }
  }, [selectedScenario, tenantId]);

  const runAllTests = useCallback(async () => {
    setIsRunning(true);
    const allResults: IsolationTestResult[] = [];

    try {
      logger.info('Running all isolation tests', {
        component: 'SecurityIsolationTester',
        operation: 'runAllTests',
        tenantId,
      });

      for (const scenario of testScenarios) {
        try {
          const result = await apiClient.runIsolationTest(scenario.id, tenantId);
          allResults.push(result);
        } catch {
          allResults.push({
            scenario_id: scenario.id,
            passed: false,
            message: 'Test execution error',
            duration_ms: 0,
            timestamp: new Date().toISOString(),
          });
        }
      }

      setResults(allResults);

      const passedCount = allResults.filter((r) => r.passed).length;
      const totalCount = allResults.length;

      // Update policy compliance
      const egressTests = allResults.filter((r) =>
        testScenarios.find((s) => s.id === r.scenario_id)?.category === 'network'
      );
      const isolationTests = allResults.filter((r) =>
        testScenarios.find((s) => s.id === r.scenario_id)?.category === 'tenant'
      );

      setPolicyCompliance({
        egress: egressTests.every((r) => r.passed),
        determinism: true,
        isolation: isolationTests.every((r) => r.passed),
      });

      if (passedCount === totalCount) {
        toast.success(`All ${totalCount} tests passed`);
      } else {
        toast.warning(`${passedCount}/${totalCount} tests passed`);
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Test suite failed';
      logger.error('All isolation tests failed', {
        component: 'SecurityIsolationTester',
        operation: 'runAllTests',
        error: errorMessage,
      });
      toast.error(errorMessage);
    } finally {
      setIsRunning(false);
    }
  }, [tenantId]);

  const getCategoryIcon = (category: string) => {
    switch (category) {
      case 'tenant':
        return <Users className="w-4 h-4" />;
      case 'memory':
        return <HardDrive className="w-4 h-4" />;
      case 'network':
        return <Network className="w-4 h-4" />;
      case 'filesystem':
        return <Lock className="w-4 h-4" />;
      default:
        return <Shield className="w-4 h-4" />;
    }
  };

  const passedCount = results.filter((r) => r.passed).length;
  const failedCount = results.filter((r) => !r.passed).length;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold tracking-tight">Organization Isolation Tester</h2>
          <p className="text-muted-foreground">
            Test and verify organization isolation boundaries
          </p>
        </div>
        <Button onClick={runAllTests} disabled={isRunning} variant="outline">
          <Play className="w-4 h-4 mr-2" />
          Run All Tests
        </Button>
      </div>

      {/* Policy Compliance Indicator */}
      <Card>
        <CardHeader>
          <CardTitle className="text-lg flex items-center gap-2">
            <Shield className="w-5 h-5" />
            Policy Compliance Status
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div className="flex items-center gap-3 p-3 border rounded-lg">
              {policyCompliance.egress ? (
                <CheckCircle className="w-5 h-5 text-green-600" />
              ) : (
                <XCircle className="w-5 h-5 text-red-600" />
              )}
              <div>
                <p className="font-medium">Egress Control</p>
                <p className="text-sm text-muted-foreground">
                  {policyCompliance.egress ? 'Compliant' : 'Violation Detected'}
                </p>
              </div>
            </div>

            <div className="flex items-center gap-3 p-3 border rounded-lg">
              {policyCompliance.determinism ? (
                <CheckCircle className="w-5 h-5 text-green-600" />
              ) : (
                <XCircle className="w-5 h-5 text-red-600" />
              )}
              <div>
                <p className="font-medium">Determinism</p>
                <p className="text-sm text-muted-foreground">
                  {policyCompliance.determinism ? 'Compliant' : 'Violation Detected'}
                </p>
              </div>
            </div>

            <div className="flex items-center gap-3 p-3 border rounded-lg">
              {policyCompliance.isolation ? (
                <CheckCircle className="w-5 h-5 text-green-600" />
              ) : (
                <XCircle className="w-5 h-5 text-red-600" />
              )}
              <div>
                <p className="font-medium">Organization Isolation</p>
                <p className="text-sm text-muted-foreground">
                  {policyCompliance.isolation ? 'Compliant' : 'Violation Detected'}
                </p>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Test Scenario Selector */}
      <Card>
        <CardHeader>
          <CardTitle className="text-lg">Run Test Scenario</CardTitle>
          <CardDescription>
            Select a test scenario to verify isolation boundaries
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex gap-4">
            <Select value={selectedScenario} onValueChange={setSelectedScenario}>
              <SelectTrigger className="w-full">
                <SelectValue placeholder="Select a test scenario" />
              </SelectTrigger>
              <SelectContent>
                {testScenarios.map((scenario) => (
                  <SelectItem key={scenario.id} value={scenario.id}>
                    <div className="flex items-center gap-2">
                      {getCategoryIcon(scenario.category)}
                      <span>{scenario.name}</span>
                    </div>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Button onClick={runTest} disabled={isRunning || !selectedScenario}>
              {isRunning ? (
                <RefreshCw className="w-4 h-4 mr-2 animate-spin" />
              ) : (
                <Play className="w-4 h-4 mr-2" />
              )}
              Run Test
            </Button>
          </div>

          {selectedScenario && (
            <div className="mt-4 p-3 bg-muted rounded-lg">
              <p className="text-sm">
                {testScenarios.find((s) => s.id === selectedScenario)?.description}
              </p>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Test Results */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center justify-between">
            <span className="flex items-center gap-2">
              <Shield className="w-5 h-5" />
              Test Results
            </span>
            {results.length > 0 && (
              <div className="flex gap-2">
                <Badge variant="outline" className="bg-green-50 text-green-700">
                  {passedCount} Passed
                </Badge>
                <Badge variant="outline" className="bg-red-50 text-red-700">
                  {failedCount} Failed
                </Badge>
              </div>
            )}
          </CardTitle>
          <CardDescription>
            Results from isolation tests
          </CardDescription>
        </CardHeader>
        <CardContent>
          {results.length === 0 ? (
            <div className="text-center py-12 text-muted-foreground">
              <Shield className="w-12 h-12 mx-auto mb-3 opacity-20" />
              <p>No test results yet</p>
              <p className="text-sm">Run a test to see results</p>
            </div>
          ) : (
            <div className="space-y-3">
              {results.map((result, index) => {
                const scenario = testScenarios.find((s) => s.id === result.scenario_id);
                return (
                  <div
                    key={`${result.scenario_id}-${index}`}
                    className={`p-4 border rounded-lg ${
                      result.passed ? 'bg-green-50/50' : 'bg-red-50/50'
                    }`}
                  >
                    <div className="flex items-start justify-between">
                      <div className="flex items-start gap-3">
                        {result.passed ? (
                          <CheckCircle className="w-5 h-5 text-green-600 mt-0.5" />
                        ) : (
                          <XCircle className="w-5 h-5 text-red-600 mt-0.5" />
                        )}
                        <div>
                          <div className="flex items-center gap-2 mb-1">
                            <span className="font-medium">
                              {scenario?.name || result.scenario_id}
                            </span>
                            <Badge
                              variant="outline"
                              className={
                                result.passed
                                  ? 'bg-green-100 text-green-800'
                                  : 'bg-red-100 text-red-800'
                              }
                            >
                              {result.passed ? 'PASSED' : 'FAILED'}
                            </Badge>
                          </div>
                          <p className="text-sm text-muted-foreground">
                            {result.message}
                          </p>
                          <div className="flex gap-4 mt-2 text-xs text-muted-foreground">
                            <span className="flex items-center gap-1">
                              <Clock className="w-3 h-3" />
                              {result.duration_ms}ms
                            </span>
                            <span>
                              {new Date(result.timestamp).toLocaleString()}
                            </span>
                          </div>
                        </div>
                      </div>
                    </div>

                    {result.details && Object.keys(result.details).length > 0 && (
                      <div className="mt-3 pt-3 border-t">
                        <p className="text-xs font-medium mb-1">Details:</p>
                        <code className="text-xs text-muted-foreground">
                          {JSON.stringify(result.details, null, 2)}
                        </code>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
