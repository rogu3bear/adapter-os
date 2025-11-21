import React from 'react';
import { Tooltip, TooltipContent, TooltipTrigger } from './tooltip';
import { HelpCircle } from 'lucide-react';
import { cn } from './utils';

import { getHelpText } from '@/data/help-text';


interface HelpTooltipProps {
  helpId?: string;
  content?: string;
  children?: React.ReactNode;
  className?: string;
  side?: 'top' | 'right' | 'bottom' | 'left';
  align?: 'start' | 'center' | 'end';
}

export function HelpTooltip({
  helpId,
  content,
  children,
  className,
  side = 'top',
  align = 'center'
}: HelpTooltipProps) {

  // Fallback help texts for items not yet in database
  const fallbackTexts: Record<string, string> = {
    'cpid': 'Control Plane ID: identifier that groups policies, plans, and telemetry.',
    'merkle-root': 'Root hash of a Merkle tree used to attest integrity of bundled events.',
    'schema-hash': 'Content hash of the policy schema version applied to a policy pack.',
    'tokens-per-second': 'Throughput: number of tokens processed per second across the system.',
    'latency-p95': 'Latency p95: 95th percentile end-to-end response latency in milliseconds.',
    'adapter-count': 'Total number of active code adapters loaded in the system.',
    'active-sessions': 'Concurrent active user or service sessions currently using the system.',
    'requires-admin': 'This action requires the Admin role. Contact an administrator for access.',
    'operations': 'Runtime management, plan execution, and system monitoring.',
    'settings': 'System configuration and administration.',
    'compute-nodes': 'Worker nodes in the cluster running inference and training workloads.',
    'active-tenants': 'Isolated tenant environments with their own adapters and policies.',
    'data-classification': 'Sensitivity level of data (Public, Internal, Confidential, Restricted) that determines access controls and handling requirements.',
    'itar-compliance': 'International Traffic in Arms Regulations compliance flag. When enabled, enforces strict US export control requirements for defense-related data.',
    'policy-name': 'Human-readable name for the policy pack identifying its purpose and scope.',
    'policy-version': 'Semantic version number (e.g., v1.2.3) tracking policy revisions and updates.',
    'policy-status': 'Current state of the policy: Active (enforced), Draft (pending review), or Disabled.',
    'policy-signed': 'Cryptographic signature status: indicates if the policy has been digitally signed for authenticity.',
    'policy-cpid': 'Control Plane ID: unique identifier that groups policies, plans, and telemetry.',
    'policy-schema-hash': 'BLAKE3 hash of the policy schema ensuring integrity and version tracking.',
    'policy-actions': 'Available operations: Edit, Sign, Compare versions, and Export policy configurations.',
    // Inference Playground help texts
    'inference-model': 'Select the base model for inference. Different models have varying capabilities, context lengths, and performance characteristics.',
    'inference-adapter-stack': 'Select a trained LoRA adapter to customize model behavior. Adapters add domain-specific knowledge without retraining the base model.',
    'inference-prompt': 'The input text or question for the model. Clear, specific prompts produce better results. Supports multi-turn conversations.',
    'inference-max-tokens': 'Maximum number of tokens to generate in the response. Higher values allow longer responses but increase latency and cost.',
    'inference-temperature': 'Controls output randomness. Lower values (0.0-0.3) for factual tasks, higher values (0.7-1.5) for creative tasks.',
    'inference-top-k': 'Limits token selection to top K most probable tokens. Lower values (10-50) make output more focused and deterministic.',
    'inference-top-p': 'Nucleus sampling threshold. Selects from smallest set of tokens whose cumulative probability exceeds P. Typically 0.9-0.95.',
    'inference-seed': 'Fixed random seed for reproducible outputs. Same seed with identical parameters produces consistent results for testing.',
    'inference-evidence': 'Enable retrieval-augmented generation (RAG). Requires evidence spans from indexed documents to support the response.',
    'inference-stream': 'Enable streaming mode to receive tokens as they are generated. Provides faster perceived response for interactive use.',
    'inference-compare-mode': 'Run inference with two different configurations side-by-side to compare outputs, latency, and quality.',
    // Adapter page help texts
    'adapter-name': 'Unique identifier for this adapter using semantic naming: tenant/domain/purpose/revision.',
    'adapter-tier': 'Adapter tier (tier_1, tier_2, tier_3) determines priority for routing and resource allocation.',
    'adapter-rank': 'LoRA rank determines model capacity. Higher ranks (16, 32) capture more patterns but use more memory.',
    'adapter-lifecycle': 'Current lifecycle state: active (in use), deprecated (phasing out), or archived (read-only).',
    'adapter-state': 'Memory state: Unloaded (not in memory), Cold (disk), Warm (CPU), Hot (GPU), Resident (pinned to GPU).',
    'adapter-memory': 'Current memory usage of this adapter in bytes. Includes weights and activation buffers.',
    'adapter-activation': 'Number of times this adapter has been selected by the router for inference.',
    'adapter-category': 'Functional category of the adapter (code, docs, analysis, etc.).',
    'adapter-version': 'Semantic version of the adapter weights and configuration.',
    'adapter-last-used': 'Timestamp of the last inference request that used this adapter.',
    'adapter-actions': 'Available operations: Promote (increase tier), Pin/Unpin (prevent eviction), Evict (free memory), Delete (remove permanently).',
    // Training page help texts
    'training-job-id': 'Unique identifier or name for the training job.',
    'training-dataset': 'The dataset used for training this adapter.',
    'training-status': 'Current state of the training job: queued, running, completed, failed, or cancelled.',
    'training-progress': 'Percentage of training epochs completed.',
    'training-loss': 'Current loss value - lower indicates better model fit. Target varies by task.',
    'training-learning-rate': 'Step size for gradient descent optimization. Smaller values = slower but more stable training.',
    'training-tokens-per-sec': 'Training throughput measured in tokens processed per second.',
    'training-created': 'When this training job was created.',
    'training-actions': 'Available actions for this job based on its status and your permissions.',
    // Status-specific help texts
    'status-running': 'Training is actively in progress. You can pause or stop the job.',
    'status-completed': 'Training finished successfully. The adapter is ready for testing.',
    'status-failed': 'Training encountered an error. Check logs for details.',
    'status-queued': 'Job is waiting in queue. Will start when resources are available.',
    'status-paused': 'Training is temporarily paused. Resume to continue.',
    'status-cancelled': 'Training was cancelled by user.',
    // Audit page help texts
    'audit-timestamp': 'When the audit event occurred (local time). All timestamps are recorded in UTC and converted to your local timezone.',
    'audit-level': 'Severity level of the audit event: debug (detailed diagnostics), info (general events), warn (attention needed), error (failures), critical (urgent issues).',
    'audit-event': 'The type of audit event (e.g., adapter.register, policy.apply, user.login). Events follow a hierarchical naming convention.',
    'audit-user': 'The user who triggered this event. System events are marked as "System" for automated processes.',
    'audit-details': 'Additional metadata associated with the event in JSON format. Contains context-specific information about the action performed.',
    'audit-controls': 'Configure pagination and manually refresh audit logs. Use filters above for advanced searching.',
    'audit-items-per-page': 'Number of audit log entries to display per page. Higher values may affect performance.',
    'audit-refresh': 'Manually refresh audit logs from the server. Logs also auto-refresh every 30 seconds.',
    'audit-export': 'Export audit logs as JSON file. Exports filtered results if filters are active, otherwise exports all loaded logs.',
    'audit-events': 'Immutable audit trail of security and system events. Accessible by Admin, SRE, and Compliance roles only.',
    'audit-pagination-prev': 'Navigate to the previous page of audit log results.',
    'audit-pagination-next': 'Navigate to the next page of audit log results.',
    'audit-filter-level': 'Filter audit logs by severity level. Select multiple levels to show events matching any selected level.',
    'audit-date-range': 'Filter audit logs by timestamp range. Useful for investigating events within a specific time window.',
    'audit-search': 'Search across event type, user ID, tenant ID, component, trace ID, and metadata fields.',
    // Node management help texts
    'node-name': 'Unique hostname identifier for this compute node in the cluster.',
    'node-status': 'Current health status: healthy (online and responsive), offline (unreachable), or error (experiencing issues).',
    'node-cpu': 'Current CPU utilization percentage across all cores on this node.',
    'node-memory': 'Total available system memory in gigabytes for running workloads.',
    'node-gpu': 'Number of GPU devices available for inference and training acceleration.',
    'node-adapters': 'Count of adapters currently loaded and running on this node.',
    'node-last-seen': 'Timestamp of the most recent heartbeat received from this node.',
    'node-endpoint': 'Network endpoint URL where the node agent is listening for commands.',
    'node-actions': 'Available operations: view details, test connectivity, mark offline, or evict.',
    'node-register': 'Register a new compute node to join the cluster. Requires node:manage permission.',
    'node-labels': 'Key-value metadata tags for organizing and filtering nodes (e.g., region, tier).',
    // Dashboard System Resources
    'cpu-usage': 'Current CPU utilization percentage across all cores in the system.',
    'memory-usage': 'Current RAM utilization percentage including system and application memory.',
    'disk-usage': 'Current disk space utilization percentage on the primary storage volume.',
    'network-bandwidth': 'Current network throughput in megabytes per second for incoming traffic.',
    // Dashboard Activity and Actions
    'recent-activity': 'Real-time feed of system events including adapter operations, policy changes, and telemetry.',
    'quick-actions': 'Frequently used operations for managing tenants, adapters, and system health.',
    'export-logs': 'Download system logs for debugging and audit purposes.',
    // Dashboard Quick Action buttons
    'quick-action-health': 'View detailed system health metrics including CPU, memory, and performance indicators.',
    'quick-action-create-tenant': 'Create a new isolated tenant environment with dedicated adapters and policies.',
    'quick-action-deploy-adapter': 'Deploy a code adapter to a specific tenant for inference workloads.',
    'quick-action-policies': 'Review and manage policy packs that govern adapter behavior and compliance.',
    // Dashboard Modal form fields
    'tenant-name-field': 'Unique identifier for the tenant. Use lowercase letters, numbers, and hyphens only.',
    'isolation-level-field': 'Security isolation level: Standard (shared resources), High (dedicated compute), Maximum (air-gapped).',
    'adapter-select-field': 'Choose an adapter from the registry to deploy to the target tenant.',
    'target-tenant-field': 'The tenant environment where the adapter will be deployed and made available.',
    // Tenant management specific help texts
    'tenant-name': 'Unique identifier for this tenant. Used for isolation and access control purposes.',
    'tenant-description': 'Brief description of the tenant purpose and scope.',
    'tenant-uid': 'Unix User ID for filesystem isolation. Each tenant should have a unique UID.',
    'tenant-gid': 'Unix Group ID for filesystem isolation. Controls group-level access permissions.',
    'tenant-isolation': 'Isolation level determines the degree of resource separation (standard, strict, or custom).',
    'tenant-status': 'Current operational state: Active (running), Paused (temporarily stopped), Suspended (admin action), Maintenance (upgrades), or Archived (decommissioned).',
    'tenant-created': 'Timestamp when this tenant was first created in the system.',
    'tenant-users': 'Number of users assigned to this tenant with active access.',
    'tenant-adapters': 'Number of LoRA adapters assigned to this tenant for inference.',
    'tenant-policies': 'Number of policy packs applied to this tenant for governance.',
    'tenant-last-activity': 'Most recent activity timestamp for this tenant.',
    'tenant-actions': 'Available management actions for this tenant.',
    'create-tenant-button': 'Create a new tenant with isolated resources and policies. Requires tenant:manage permission.',
    'create-tenant-action': 'Finalize tenant creation with the specified configuration.',
    'save-tenant-changes': 'Save modifications to tenant configuration.',
    'archive-tenant-action': 'Archive this tenant. Resources will be suspended but can be restored by an administrator.',
    'assign-policies-action': 'Assign selected policy packs to this tenant for governance enforcement.',
    'assign-adapters-action': 'Assign selected LoRA adapters to this tenant for inference.',
    'import-tenants': 'Import tenant configurations from JSON or CSV file.',
    'export-tenants': 'Export tenant data to JSON or CSV format for backup or migration.',
    'export-usage-csv': 'Download tenant usage metrics as a CSV file.',
    // Promotion page help texts
    'promotion-cpid': 'Control Plane ID: unique identifier for the promotion candidate. Enter the CPID of the adapter or bundle to promote.',
    'promotion-gates': 'Promotion gates are automated checks that must pass before promotion: policy compliance, test coverage, performance benchmarks, and security scans.',
    'promotion-dry-run': 'Preview the promotion without making changes. Simulates the entire promotion workflow and reports what would happen.',
    'promotion-history': 'Chronological record of all promotions and rollbacks. Includes CPID, operator, timestamp, and outcome status.',
    'promotion-execute': 'Execute the promotion to move the adapter to a higher tier or environment. Requires all gates to pass.',
    'promotion-rollback': 'Revert to the previous promotion state. Use when a promoted adapter causes issues in the target environment.',
    // Golden run page help texts
    'golden-run': 'A golden run is a reference baseline capturing model outputs under controlled conditions. Used to verify determinism and detect regressions.',
    'golden-baseline': 'The reference golden run to compare against. Select a stable baseline that represents expected behavior.',
    'golden-comparison': 'Side-by-side comparison of two golden runs showing metric differences, epsilon divergence, and output variations.',
    'golden-create': 'Create a new golden baseline from the current model state. Captures outputs for all test inputs.',
    // Testing page help texts
    'testing-epsilon': 'Maximum allowed numerical difference between outputs. Smaller values (1e-8) require stricter determinism, larger values (1e-4) allow more variance.',
    'testing-pass-rate': 'Percentage of test cases that must pass for overall success. 100% for critical systems, 95%+ for production.',
    'testing-config': 'Configure test parameters including epsilon threshold, pass rate, and baseline selection before running validation.',
    'testing-run': 'Execute validation tests comparing adapter outputs against golden baselines. Results determine promotion eligibility.',
    // Base Model help texts
    'base-model-name': 'The name and identifier of the currently loaded base model used for inference.',
    'base-model-status': 'Current state of the base model: loaded (ready for inference), loading, unloading, unloaded, or error.',
    'base-model-memory': 'Memory consumption of the base model in GPU VRAM. Larger models require more memory.',
    // Single File Adapter Trainer help texts
    'trainer-file-upload': 'Upload a training file (.txt, .json, .py, .js, .ts, .md). The file content will be used to create training examples for your adapter.',
    'trainer-adapter-name': 'Unique name for your trained adapter. Follows semantic naming: tenant/domain/purpose/revision format.',
    'trainer-rank': 'LoRA rank controls adapter capacity. Lower (4-8) = faster training, less memory. Higher (16-64) = more capacity, slower training.',
    'trainer-alpha': 'Scaling factor for LoRA weights. Typically set to 2x rank value. Higher alpha = stronger adaptation.',
    'trainer-learning-rate': 'Step size for optimization. Smaller (0.0001) = stable but slow. Larger (0.001) = faster but may overshoot.',
    'trainer-epochs': 'Number of complete passes through training data. More epochs = better fit but risk of overfitting.',
    'trainer-batch-size': 'Samples processed together. Larger = faster, more memory. Smaller = less memory, more gradient noise.',
    // Management Panel help texts
    'management-services': 'Service management: monitor and control core services, monitoring tools, and background processes.',
    'management-resources': 'Resource overview: view tenants, adapters, models, and policies with quick navigation links.',
    'management-workers': 'Quick actions: common operations for ML pipelines, operations, monitoring, and compliance.',
    // Monitoring Page help texts
    'monitoring-overview': 'System health overview: real-time status of services, nodes, and key performance indicators.',
    'monitoring-resources': 'Resource utilization: CPU, memory, disk, and GPU usage across compute nodes.',
    'monitoring-alerts': 'Active alerts: critical and warning alerts requiring attention, with acknowledgment workflow.',
    'monitoring-metrics': 'Real-time metrics: live performance charts, throughput, latency, and system telemetry.',
    // Telemetry help texts
    'telemetry-event': 'Unique identifier for this telemetry bundle. Bundles group related events for efficient storage and transmission.',
    'telemetry-timestamp': 'When this telemetry bundle was created. Bundles are created periodically or when event thresholds are reached.',
    'telemetry-type': 'Number of telemetry events contained in this bundle. Events include inference requests, policy enforcement, and system metrics.',
    'telemetry-export': 'Export telemetry bundles for offline analysis or archival. Requires audit:view permission. Available in JSON and CSV formats.',
    'telemetry-filters': 'Filter telemetry bundles by search terms, CPID, date range, event count, or file size to find specific events.',
    // Replay panel help texts
    'replay-session': 'Replay session containing a snapshot of execution state at a specific point in time for deterministic replay and verification.',
    'replay-manifest-hash': 'BLAKE3 hash of the manifest file that defines the execution context, including model configuration and adapter stack.',
    'replay-policy-hash': 'BLAKE3 hash of the policy pack applied during execution. Used to verify policy integrity during replay.',
    'replay-kernel-hash': 'BLAKE3 hash of the Metal/CoreML kernel used for computation. Ensures deterministic execution across replays.',
    'replay-verification': 'Cryptographic verification of the replay session. Validates signature chain, hash integrity, and checks for execution divergences.',
    'replay-divergence': 'Points where replay execution differs from the original. Indicates non-determinism or configuration mismatch.',
    // Routing inspector help texts (additional)
    'routing-k-value': 'Number of adapters selected by K-sparse routing. Higher K increases expressiveness but adds compute overhead.',
    'routing-entropy': 'Shannon entropy of gate distribution. Higher entropy indicates more uniform adapter selection. Low entropy may indicate collapsed routing.',
    'routing-overhead': 'Routing overhead as percentage of inference time. Budget limit is 8%. Values above indicate performance issues.',
    'routing-latency': 'Router decision latency in microseconds. Lower values indicate faster adapter selection.'
  };

  // If direct content is provided, use it; otherwise look up by helpId
  let helpText: string;
  if (content) {
    helpText = content;
  } else if (helpId) {
    const helpItem = getHelpText(helpId);
    helpText = helpItem?.content || fallbackTexts[helpId] || 'Help information not available.';
  } else {
    helpText = 'Help information not available.';
  }



  // Default trigger is a help icon if no children provided
  const trigger = children || (
    <span className="inline-flex items-center ml-1 cursor-help">
      <HelpCircle className="h-3 w-3 text-muted-foreground hover:text-foreground transition-colors" />
    </span>
  );

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        {trigger}
      </TooltipTrigger>
      <TooltipContent
        side={side}
        align={align}
        className={cn("max-w-xs", className)}
      >
        <div className="space-y-1">
          <div className="flex items-center gap-1">
            <HelpCircle className="h-3 w-3" />
            <span className="font-medium text-xs">Help</span>
          </div>
          <p className="text-xs leading-relaxed">{helpText}</p>
        </div>
      </TooltipContent>
    </Tooltip>
  );
}
