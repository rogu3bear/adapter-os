/**
 * PolicyPreflightDialog Usage Examples
 *
 * Demonstrates how to integrate policy preflight checks into adapter loading
 * and stack activation workflows.
 *
 * Citation: [2025-11-25†ui†policy-preflight-examples]
 */

import React, { useState } from 'react';
import { PolicyPreflightDialog, PolicyCheck } from './PolicyPreflightDialog';
import { Button } from './ui/button';

/**
 * Example 1: Adapter Loading with Policy Checks
 */
export function AdapterLoadingExample() {
  const [showDialog, setShowDialog] = useState(false);
  const [isLoading, setIsLoading] = useState(false);

  // Mock policy checks for adapter loading
  const policyChecks: PolicyCheck[] = [
    {
      policy_id: 'egress-001',
      policy_name: 'Zero Network Egress',
      passed: true,
      severity: 'error',
      message: 'No network egress detected in production mode',
      can_override: false,
    },
    {
      policy_id: 'determinism-001',
      policy_name: 'Deterministic Execution',
      passed: true,
      severity: 'error',
      message: 'All randomness is HKDF-seeded',
      can_override: false,
    },
    {
      policy_id: 'naming-001',
      policy_name: 'Semantic Naming',
      passed: false,
      severity: 'warning',
      message: 'Adapter name should follow {tenant}/{domain}/{purpose}/{revision} format',
      can_override: true,
      details: 'Current: "my-adapter", Expected: "default/code/linter/r001"',
    },
    {
      policy_id: 'tenant-001',
      policy_name: 'Tenant Isolation',
      passed: true,
      severity: 'error',
      message: 'Adapter restricted to authorized tenant',
      can_override: false,
    },
  ];

  // Check if all critical policies passed
  const canProceed = !policyChecks.some(
    c => !c.passed && c.severity === 'error' && !c.can_override
  );

  const handleLoadAdapter = async () => {
    setIsLoading(true);
    try {
      // API call to load adapter
      await fetch('/v1/adapters/my-adapter/load', { method: 'POST' });
      // Adapter loaded successfully
      setShowDialog(false);
    } catch (_error) {
      // Error handled by UI state
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="p-4">
      <Button onClick={() => setShowDialog(true)}>
        Load Adapter
      </Button>

      <PolicyPreflightDialog
        open={showDialog}
        onOpenChange={setShowDialog}
        title="Load Adapter - Policy Validation"
        description="The following policies will be enforced when loading this adapter"
        checks={policyChecks}
        canProceed={canProceed}
        onProceed={handleLoadAdapter}
        onCancel={() => setShowDialog(false)}
        isAdmin={true} // User role from auth context
        isLoading={isLoading}
      />
    </div>
  );
}

/**
 * Example 2: Stack Activation with Policy Checks
 */
export function StackActivationExample() {
  const [showDialog, setShowDialog] = useState(false);
  const [isLoading, setIsLoading] = useState(false);

  // Mock policy checks for stack activation
  const policyChecks: PolicyCheck[] = [
    {
      policy_id: 'router-001',
      policy_name: 'K-Sparse Routing',
      passed: true,
      severity: 'error',
      message: 'Q15 quantized gates configured for adapter selection',
      can_override: false,
    },
    {
      policy_id: 'evidence-001',
      policy_name: 'Evidence Quality',
      passed: false,
      severity: 'warning',
      message: 'Some adapters in stack have low relevance scores',
      can_override: true,
      details: 'Adapter "code-assistant" relevance: 0.65 (threshold: 0.70)',
    },
    {
      policy_id: 'telemetry-001',
      policy_name: 'Telemetry Enabled',
      passed: true,
      severity: 'info',
      message: 'Structured event logging enabled for audit trail',
      can_override: true,
    },
  ];

  const canProceed = !policyChecks.some(
    c => !c.passed && c.severity === 'error' && !c.can_override
  );

  const handleActivateStack = async () => {
    setIsLoading(true);
    try {
      // API call to activate stack
      await fetch('/v1/adapter-stacks/my-stack/activate', { method: 'POST' });
      // Stack activated successfully
      setShowDialog(false);
    } catch (_error) {
      // Error handled by UI state
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="p-4">
      <Button onClick={() => setShowDialog(true)}>
        Activate Stack
      </Button>

      <PolicyPreflightDialog
        open={showDialog}
        onOpenChange={setShowDialog}
        title="Activate Stack - Policy Validation"
        description="The following policies apply to this adapter stack"
        checks={policyChecks}
        canProceed={canProceed}
        onProceed={handleActivateStack}
        onCancel={() => setShowDialog(false)}
        isAdmin={false} // Non-admin user
        isLoading={isLoading}
      />
    </div>
  );
}

/**
 * Example 3: Fetching Policy Checks from API
 */
export function ApiIntegrationExample() {
  const [showDialog, setShowDialog] = useState(false);
  const [policyChecks, setPolicyChecks] = useState<PolicyCheck[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const fetchPolicyChecks = async (adapterId: string) => {
    try {
      // Fetch policy validation results from API
      const response = await fetch(`/v1/adapters/${adapterId}/validate-policies`);
      const data = await response.json();

      // Transform API response to PolicyCheck format
      const checks: PolicyCheck[] = data.policies.map((p: any) => ({
        policy_id: p.id,
        policy_name: p.name,
        passed: p.status === 'passed',
        severity: p.severity,
        message: p.message,
        can_override: p.can_override,
        details: p.details,
      }));

      setPolicyChecks(checks);
      setShowDialog(true);
    } catch (error) {
      console.error('Failed to fetch policy checks:', error);
    }
  };

  const handleProceed = async () => {
    setIsLoading(true);
    try {
      // Proceed with operation
      await fetch('/v1/adapters/my-adapter/load', { method: 'POST' });
      setShowDialog(false);
    } finally {
      setIsLoading(false);
    }
  };

  const canProceed = !policyChecks.some(
    c => !c.passed && c.severity === 'error' && !c.can_override
  );

  return (
    <div className="p-4">
      <Button onClick={() => fetchPolicyChecks('my-adapter')}>
        Load Adapter (with API validation)
      </Button>

      <PolicyPreflightDialog
        open={showDialog}
        onOpenChange={setShowDialog}
        title="Policy Validation"
        checks={policyChecks}
        canProceed={canProceed}
        onProceed={handleProceed}
        onCancel={() => setShowDialog(false)}
        isLoading={isLoading}
      />
    </div>
  );
}

/**
 * Example 4: Integration with AdapterDetailPage
 */
export function AdapterDetailPageIntegration() {
  const [showPreflight, setShowPreflight] = useState(false);
  const [isLoading, setIsLoading] = useState(false);

  // This would typically come from an API call
  const validateAdapterPolicies = async (adapterId: string): Promise<PolicyCheck[]> => {
    const response = await fetch(`/v1/adapters/${adapterId}/validate-policies`);
    const data = await response.json();
    return data.policies.map((p: any) => ({
      policy_id: p.id,
      policy_name: p.name,
      passed: p.status === 'passed',
      severity: p.severity as 'error' | 'warning' | 'info',
      message: p.message,
      can_override: p.can_override,
      details: p.details,
    }));
  };

  const [checks, setChecks] = useState<PolicyCheck[]>([]);

  const handleLoadClick = async () => {
    // Fetch policy checks before showing dialog
    const policyChecks = await validateAdapterPolicies('adapter-123');
    setChecks(policyChecks);
    setShowPreflight(true);
  };

  const handleLoad = async () => {
    setIsLoading(true);
    try {
      await fetch('/v1/adapters/adapter-123/load', { method: 'POST' });
      // Refresh adapter state
      setShowPreflight(false);
    } finally {
      setIsLoading(false);
    }
  };

  const canProceed = !checks.some(
    c => !c.passed && c.severity === 'error' && !c.can_override
  );

  return (
    <div>
      <Button onClick={handleLoadClick}>Load Adapter</Button>

      <PolicyPreflightDialog
        open={showPreflight}
        onOpenChange={setShowPreflight}
        title="Load Adapter - Policy Validation"
        description="23 canonical policies will be enforced"
        checks={checks}
        canProceed={canProceed}
        onProceed={handleLoad}
        onCancel={() => setShowPreflight(false)}
        isAdmin={true}
        isLoading={isLoading}
      />
    </div>
  );
}
