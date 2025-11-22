import { useCallback, useMemo } from 'react';
import { Adapter, AdapterState, LifecycleState } from '../../api/types';

interface StackAdapter {
  adapter: Adapter;
  order: number;
  enabled: boolean;
}

export interface ValidationIssue {
  level: 'error' | 'warning' | 'info';
  category: string;
  message: string;
  adapter?: string;
  suggestion?: string;
}

export interface ValidationReport {
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

const RESERVED_TENANTS = ['system', 'admin', 'root', 'default', 'test'];
const RESERVED_DOMAINS = ['core', 'internal', 'deprecated'];
const MAX_ADAPTERS_PER_STACK = 10;
const MAX_RANK_VARIANCE = 16;

/**
 * Validates framework compatibility across adapters
 */
const validateFrameworkCompatibility = (adapters: StackAdapter[]): ValidationIssue[] => {
  const issues: ValidationIssue[] = [];
  const frameworks = new Set<string>();
  const enabledAdapters = adapters.filter((item) => item.enabled);

  enabledAdapters.forEach((item) => {
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
        'Adapters trained on different frameworks may have reduced effectiveness when combined. Consider using adapters from the same framework for optimal performance.',
    });
  }

  return issues;
};

/**
 * Validates rank compatibility across adapters
 */
const validateRankCompatibility = (adapters: StackAdapter[]): ValidationIssue[] => {
  const issues: ValidationIssue[] = [];
  const enabledAdapters = adapters.filter((item) => item.enabled);

  if (enabledAdapters.length === 0) return issues;

  const ranks = enabledAdapters.map((item) => item.adapter.rank);
  const minRank = Math.min(...ranks);
  const maxRank = Math.max(...ranks);
  const rankDiff = maxRank - minRank;

  if (rankDiff > MAX_RANK_VARIANCE) {
    issues.push({
      level: 'warning',
      category: 'Rank Compatibility',
      message: `Rank variance is ${rankDiff} (min: ${minRank}, max: ${maxRank})`,
      suggestion: `Consider using adapters with similar ranks (max variance: ${MAX_RANK_VARIANCE}). High variance may impact performance characteristics.`,
    });
  }

  return issues;
};

/**
 * Validates tier alignment across adapters
 */
const validateTierAlignment = (adapters: StackAdapter[]): ValidationIssue[] => {
  const issues: ValidationIssue[] = [];
  const enabledAdapters = adapters.filter((item) => item.enabled);

  if (enabledAdapters.length === 0) return issues;

  const tiers = enabledAdapters
    .map((item) => item.adapter.tier)
    .filter((tier) => tier !== undefined) as string[];

  const uniqueTiers = new Set(tiers);

  if (uniqueTiers.size > 1) {
    issues.push({
      level: 'info',
      category: 'Tier Alignment',
      message: `Stack contains adapters from different tiers (${Array.from(uniqueTiers).join(', ')})`,
      suggestion: 'Mixing storage tiers is allowed but may affect memory efficiency. For consistent behavior, consider using adapters from the same tier.',
    });
  }

  return issues;
};

/**
 * Validates semantic naming conventions
 */
const validateSemanticNaming = (adapters: StackAdapter[], stackName?: string): ValidationIssue[] => {
  const issues: ValidationIssue[] = [];

  // Validate stack name
  if (stackName) {
    const stackNameParts = stackName.split('/');

    // Check for reserved tenant names
    if (
      stackNameParts.length > 0 &&
      RESERVED_TENANTS.includes(stackNameParts[0].toLowerCase())
    ) {
      issues.push({
        level: 'error',
        category: 'Semantic Naming',
        message: `Stack name uses reserved tenant: "${stackNameParts[0]}"`,
        suggestion: `Use a valid tenant name instead of: ${RESERVED_TENANTS.join(', ')}`,
      });
    }

    // Check for reserved domain names
    if (
      stackNameParts.length > 1 &&
      RESERVED_DOMAINS.includes(stackNameParts[1].toLowerCase())
    ) {
      issues.push({
        level: 'error',
        category: 'Semantic Naming',
        message: `Stack name uses reserved domain: "${stackNameParts[1]}"`,
        suggestion: `Use a domain name instead of: ${RESERVED_DOMAINS.join(', ')}`,
      });
    }
  }

  // Validate adapter semantic naming (should be {tenant}/{domain}/{purpose}/{revision})
  adapters.forEach((item) => {
    const nameParts = item.adapter.name.split('/');
    if (nameParts.length < 4) {
      issues.push({
        level: 'warning',
        category: 'Semantic Naming',
        message: `Adapter "${item.adapter.name}" doesn't follow semantic naming format`,
        adapter: item.adapter.name,
        suggestion:
          'Use format: {tenant}/{domain}/{purpose}/{revision} (e.g., tenant-a/engineering/code-review/r001)',
      });
    }

    // Check for revision gap (max 5)
    if (nameParts.length === 4) {
      const revision = nameParts[3];
      const revNum = parseInt(revision.substring(1), 10);
      if (isNaN(revNum)) {
        issues.push({
          level: 'info',
          category: 'Semantic Naming',
          message: `Adapter "${item.adapter.name}" has non-standard revision format`,
          adapter: item.adapter.name,
          suggestion: 'Revision should be in format r### (e.g., r001, r042)',
        });
      }
    }
  });

  return issues;
};

/**
 * Validates router compliance (K-sparse routing)
 */
const validateRouterCompliance = (adapters: StackAdapter[]): ValidationIssue[] => {
  const issues: ValidationIssue[] = [];
  const enabledAdapters = adapters.filter((item) => item.enabled);

  // Check stack size
  if (enabledAdapters.length > MAX_ADAPTERS_PER_STACK) {
    issues.push({
      level: 'warning',
      category: 'Router Compliance',
      message: `Stack has ${enabledAdapters.length} adapters (recommended max: ${MAX_ADAPTERS_PER_STACK} for K-sparse routing)`,
      suggestion: 'Consider reducing stack size for better router performance and lower latency.',
    });
  }

  // Check for empty stack
  if (enabledAdapters.length === 0) {
    issues.push({
      level: 'error',
      category: 'Router Compliance',
      message: 'No adapters are enabled in the stack',
      suggestion: 'Enable at least one adapter to create a valid stack.',
    });
  }

  return issues;
};

/**
 * Validates policy compliance
 */
const validatePolicyCompliance = (adapters: StackAdapter[]): ValidationIssue[] => {
  const issues: ValidationIssue[] = [];

  adapters.forEach((item) => {
    const { adapter } = item;

    // Check for adapters with no activation history
    if (adapter.activation_count === 0 && !item.enabled) {
      issues.push({
        level: 'info',
        category: 'Policy Compliance',
        message: `Adapter "${adapter.name}" has no activation history`,
        adapter: adapter.name,
        suggestion:
          'Consider testing the adapter before adding it to production stacks.',
      });
    }

    // Check for deprecated adapters
    if (adapter.lifecycle_state === 'deprecated' && item.enabled) {
      issues.push({
        level: 'warning',
        category: 'Policy Compliance',
        message: `Adapter "${adapter.name}" is marked as deprecated`,
        adapter: adapter.name,
        suggestion: 'Use an active adapter instead of deprecated ones. Schedule migration to replacement adapters.',
      });
    }

    // Check for retired adapters (blocking error)
    if (adapter.lifecycle_state === 'retired') {
      issues.push({
        level: 'error',
        category: 'Policy Compliance',
        message: `Adapter "${adapter.name}" is retired and cannot be used`,
        adapter: adapter.name,
        suggestion: 'Remove this adapter from the stack. It is no longer supported.',
      });
    }

    // Check for pinned adapters in ephemeral stacks (warning)
    if (adapter.pinned && adapter.category === 'ephemeral') {
      issues.push({
        level: 'warning',
        category: 'Policy Compliance',
        message: `Adapter "${adapter.name}" is pinned but marked as ephemeral`,
        adapter: adapter.name,
        suggestion: 'Pinned ephemeral adapters may cause issues. Review the adapter lifecycle configuration.',
      });
    }
  });

  return issues;
};

/**
 * Calculates stack metrics
 */
const calculateStackMetrics = (adapters: StackAdapter[]) => {
  const enabledAdapters = adapters.filter((item) => item.enabled);

  // Estimate total parameters (rough: rank * 1000 per adapter)
  const totalParameters = enabledAdapters.reduce(
    (sum, item) => sum + (item.adapter.rank || 0) * 1000,
    0
  );

  // Sum memory usage
  const totalMemory = enabledAdapters.reduce(
    (sum, item) => sum + (item.adapter.memory_bytes || 0),
    0
  );

  // Estimate latency: 2.5ms per adapter + 5ms base
  const estimatedLatency = enabledAdapters.length * 2.5 + 5;

  // Calculate compatibility score (0-100)
  let compatScore = 100;

  if (enabledAdapters.length === 0) {
    compatScore = 0;
  } else {
    // Deduct for retired adapters
    if (adapters.some((item) => item.adapter.lifecycle_state === 'retired')) {
      compatScore -= 50;
    }

    // Deduct for deprecated adapters
    if (adapters.some((item) => item.adapter.lifecycle_state === 'deprecated')) {
      compatScore -= 20;
    }

    // Deduct for oversized stacks
    if (enabledAdapters.length > MAX_ADAPTERS_PER_STACK) {
      compatScore -= 15;
    }

    // Deduct for rank variance
    const ranks = enabledAdapters.map((item) => item.adapter.rank);
    const rankDiff = Math.max(...ranks) - Math.min(...ranks);
    if (rankDiff > MAX_RANK_VARIANCE) {
      compatScore -= 10;
    }
  }

  return {
    totalAdapters: adapters.length,
    enabledAdapters: enabledAdapters.length,
    totalParameters: Math.floor(totalParameters),
    totalMemory: Math.floor(totalMemory),
    estimatedLatency: Math.round(estimatedLatency * 10) / 10,
    compatibilityScore: Math.max(0, Math.min(100, compatScore)),
  };
};

/**
 * Custom hook for comprehensive stack validation
 */
export const useStackValidation = (adapters: StackAdapter[], stackName?: string) => {
  const validate = useCallback((): ValidationReport => {
    const allIssues: ValidationIssue[] = [];

    // Run all validation rule sets
    allIssues.push(...validateFrameworkCompatibility(adapters));
    allIssues.push(...validateRankCompatibility(adapters));
    allIssues.push(...validateTierAlignment(adapters));
    allIssues.push(...validateSemanticNaming(adapters, stackName));
    allIssues.push(...validateRouterCompliance(adapters));
    allIssues.push(...validatePolicyCompliance(adapters));

    // Determine overall validity (no errors allowed)
    const errorCount = allIssues.filter((i) => i.level === 'error').length;

    return {
      isValid: errorCount === 0,
      issues: allIssues,
      summary: calculateStackMetrics(adapters),
    };
  }, [adapters, stackName]);

  // Memoize the validation report
  const report = useMemo(() => validate(), [validate]);

  return {
    report,
    isValid: report.isValid,
    issues: report.issues,
    errors: report.issues.filter((i) => i.level === 'error'),
    warnings: report.issues.filter((i) => i.level === 'warning'),
    infos: report.issues.filter((i) => i.level === 'info'),
    summary: report.summary,
  };
};

export default useStackValidation;
