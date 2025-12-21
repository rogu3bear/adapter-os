/**
 * @deprecated DUPLICATE PAGE - Naming conflict with pages/TestingPage.tsx
 * Routes use pages/TestingPage.tsx for /testing route.
 * This file appears to be an alternative testing implementation.
 *
 * TODO: Rename to TestingDetailPage.tsx or merge with pages/TestingPage.tsx
 * Audit date: 2025-12-19
 */
import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { LoadingState } from '@/components/ui/loading-state';
import { EmptyState } from '@/components/ui/empty-state';
import GoldenRuns from '@/components/GoldenRuns';
import GoldenCompareModal from '@/components/GoldenCompareModal';
import { apiClient } from '@/api/services';
import { Adapter, VerificationReport } from '@/api/types';
import { logger, toError } from '@/utils/logger';
import { Link } from 'react-router-dom';
import { FlaskConical, CheckCircle, XCircle, AlertTriangle, Settings, Play, GitCompare } from 'lucide-react';
import { buildReplayLink } from '@/utils/navLinks';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { errorRecoveryTemplates } from '@/components/ui/error-recovery';
import { GlossaryTooltip } from '@/components/ui/glossary-tooltip';
import { useRBAC } from '@/hooks/security/useRBAC';
import { LIFECYCLE_STATE_LABELS } from '@/constants/terminology';

export function TestingPage() {
  const { can } = useRBAC();
  const [adapters, setAdapters] = useState<Adapter[]>([]);
  const [selectedAdapter, setSelectedAdapter] = useState<string | null>(null);
  const [isCompareModalOpen, setIsCompareModalOpen] = useState(false);
  const [isConfigModalOpen, setIsConfigModalOpen] = useState(false);
  const [testConfig, setTestConfig] = useState({
    epsilonThreshold: 1e-6,
    passRateThreshold: 95,
    selectedGolden: '',
  });
  const [testResults, setTestResults] = useState<VerificationReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [statusMessage, setStatusMessage] = useState<{ message: string; variant: 'success' | 'info' | 'warning' } | null>(null);
  const [errorRecovery, setErrorRecovery] = useState<React.ReactElement | null>(null);

  const canRunTests = can('testing:execute');
  const canViewTests = can('testing:view');

  const showStatus = (message: string, variant: 'success' | 'info' | 'warning') => {
    setStatusMessage({ message, variant });
  };

  useEffect(() => {
    const fetchAdapters = async () => {
      try {
        const adaptersData = await apiClient.listAdapters();
        setAdapters(adaptersData.filter(a => a.active));
        setStatusMessage(null);
        setErrorRecovery(null);
      } catch (err) {
        logger.error('Failed to fetch adapters for testing', { component: 'TestingPage' }, toError(err));
        setStatusMessage({ message: 'Failed to load adapters.', variant: 'warning' });
        setErrorRecovery(
          errorRecoveryTemplates.genericError(
            err instanceof Error ? err : new Error('Failed to load adapters.'),
            () => fetchAdapters()
          )
        );
      } finally {
        setLoading(false);
      }
    };
    fetchAdapters();
  }, []);

  const handleStartTest = (adapterId: string) => {
    setSelectedAdapter(adapterId);
    setIsConfigModalOpen(true);
  };

  const handleRunTest = async () => {
    if (!selectedAdapter || !testConfig.selectedGolden) {
      showStatus('Please select adapter and golden baseline.', 'warning');
      return;
    }
    try {
      const report = await apiClient.goldenCompare({
        golden: testConfig.selectedGolden,
        bundle_id: selectedAdapter, // Assuming adapterId is used as bundle_id for comparison
        strictness: 'epsilon-tolerant',
        epsilon_tolerance: testConfig.epsilonThreshold,
      });
      setTestResults(report);
      setIsCompareModalOpen(true);
      showStatus('Test completed.', 'success');
      setErrorRecovery(null);
    } catch (err) {
      setStatusMessage({ message: 'Test failed.', variant: 'warning' });
      setErrorRecovery(
        errorRecoveryTemplates.genericError(
          err instanceof Error ? err : new Error('Test failed.'),
          () => handleRunTest()
        )
      );
    }
    setIsConfigModalOpen(false);
  };

  const isPassed = testResults?.passed && (testResults.epsilon_comparison.pass_rate ?? 0) >= testConfig.passRateThreshold;

  return (
    <div className="space-y-6">
      {errorRecovery && (
        <div>
          {errorRecovery}
        </div>
      )}

      {statusMessage && (
        <Alert
          className={
            statusMessage.variant === 'success'
              ? 'border-green-200 bg-green-50'
              : statusMessage.variant === 'warning'
                ? 'border-amber-200 bg-amber-50'
                : 'border-blue-200 bg-blue-50'
          }
        >
          <AlertDescription
            className={
              statusMessage.variant === 'success'
                ? 'text-green-700'
                : statusMessage.variant === 'warning'
                  ? 'text-amber-700'
                  : 'text-blue-700'
            }
          >
            {statusMessage.message}
          </AlertDescription>
        </Alert>
      )}

      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold flex items-center gap-2">
            Testing & Validation
            <GlossaryTooltip termId="testing-config" />
          </h1>
          <p className="text-muted-foreground">Test and validate adapters against golden baselines</p>
        </div>
      </div>

      {/* Adapters Table */}
      <Card>
        <CardHeader>
          <CardTitle>Adapters Ready for Testing</CardTitle>
        </CardHeader>
        <CardContent>
          {loading ? (
            <LoadingState variant="minimal" message="Loading adapters..." />
          ) : adapters.length === 0 ? (
            <EmptyState
              variant="minimal"
              icon={FlaskConical}
              title="No adapters ready for testing"
              description="No active adapters available."
            />
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Created</TableHead>
                  <TableHead>Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {adapters.map(adapter => (
                  <TableRow key={adapter.id}>
                    <TableCell className="font-medium">{adapter.name}</TableCell>
                    <TableCell>
                      <Badge>{adapter.current_state ? (LIFECYCLE_STATE_LABELS[adapter.current_state] || adapter.current_state) : 'unknown'}</Badge>
                    </TableCell>
                    <TableCell>{new Date(adapter.created_at).toLocaleString()}</TableCell>
                    <TableCell>
                      <Button onClick={() => handleStartTest(adapter.id)} disabled={!canRunTests}>
                        <FlaskConical className="mr-2 h-4 w-4" />
                        Start Test
                        <GlossaryTooltip termId="testing-run" />
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {!canRunTests && (
        <Alert>
          <AlertDescription className="text-muted-foreground">
            You need the testing:execute permission to run tests on adapters.
          </AlertDescription>
        </Alert>
      )}

      {/* Golden Runs Component */}
      <GoldenRuns />

      {/* Test Config Modal */}
      <Dialog open={isConfigModalOpen} onOpenChange={setIsConfigModalOpen}>
        <DialogContent>
          <CardHeader>
            <CardTitle className="flex items-center gap-1">
              Configure Test
              <GlossaryTooltip termId="testing-config" />
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div>
              <Label className="flex items-center gap-1">
                Epsilon Threshold
                <GlossaryTooltip termId="testing-epsilon" />
              </Label>
              <Input
                type="number"
                value={testConfig.epsilonThreshold}
                onChange={(e) => setTestConfig(prev => ({ ...prev, epsilonThreshold: parseFloat(e.target.value) }))}
              />
            </div>
            <div>
              <Label className="flex items-center gap-1">
                Pass Rate Threshold (%)
                <GlossaryTooltip termId="testing-pass-rate" />
              </Label>
              <Input
                type="number"
                value={testConfig.passRateThreshold}
                onChange={(e) => setTestConfig(prev => ({ ...prev, passRateThreshold: parseInt(e.target.value) }))}
              />
            </div>
            <div>
              <Label className="flex items-center gap-1">
                Golden Baseline
                <GlossaryTooltip termId="golden-baseline" />
              </Label>
              <Select onValueChange={(value) => setTestConfig(prev => ({ ...prev, selectedGolden: value }))}>
                <SelectTrigger>
                  <SelectValue placeholder="Select golden run" />
                </SelectTrigger>
                <SelectContent>
                  {/* Populate from GoldenRuns */}
                  <SelectItem value="golden-1">Golden Run 1</SelectItem>
                  <SelectItem value="golden-2">Golden Run 2</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </CardContent>
          <div className="flex justify-end gap-2 p-4">
            <Button variant="outline" onClick={() => setIsConfigModalOpen(false)}>Cancel</Button>
            <Button onClick={handleRunTest} disabled={!canRunTests}>Run Test</Button>
          </div>
        </DialogContent>
      </Dialog>

      {/* Compare Modal */}
      <GoldenCompareModal
        open={isCompareModalOpen}
        onOpenChange={setIsCompareModalOpen}
        bundleId={selectedAdapter}
      />

      {/* Test Results */}
      {testResults && (
        <Card>
          <CardHeader>
            <CardTitle>Test Results</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              <Badge variant={isPassed ? 'default' : 'destructive'}>
                {isPassed ? 'PASSED' : 'FAILED'}
              </Badge>
              <p>Pass Rate: {testResults.epsilon_comparison.pass_rate}%</p>
              <p>Divergent Layers: {testResults.epsilon_comparison.divergent_layers.length}</p>
              {isPassed && (
                <Link to={buildReplayLink()}>
                  <Button>View run history</Button>
                </Link>
              )}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
