/**
 * Concept Tooltips
 *
 * Canonical definitions from docs/CONCEPTS.md
 * Used for "?" tooltips throughout the UI
 */

export interface ConceptTooltip {
  term: string;
  definition: string;
  learnMoreUrl?: string;
}

export const conceptTooltips: Record<string, ConceptTooltip> = {
  tenant: {
    term: 'Tenant',
    definition: 'A tenant is the top-level isolation unit in AdapterOS, representing a user, organization, or environment.',
    learnMoreUrl: '/docs/concepts#tenant'
  },

  adapter: {
    term: 'Adapter',
    definition: 'An adapter is a LoRA (Low-Rank Adaptation) module that specializes a base model for a specific task, domain, or style.',
    learnMoreUrl: '/docs/concepts#adapter'
  },

  stack: {
    term: 'Stack',
    definition: 'A stack is a tenant-scoped set of adapters with execution rules (workflow type, policies) used for inference.',
    learnMoreUrl: '/docs/concepts#stack'
  },

  router: {
    term: 'Router',
    definition: 'The router is the K-sparse gating mechanism that selects the top-K most relevant adapters for each inference request.',
    learnMoreUrl: '/docs/concepts#router'
  },

  kernel: {
    term: 'Kernel',
    definition: 'Kernels are precompiled Metal compute shaders that execute LoRA operations on the GPU with deterministic, reproducible computation.',
    learnMoreUrl: '/docs/concepts#kernel'
  },

  telemetry: {
    term: 'Telemetry',
    definition: 'Telemetry is the structured event logging system that creates an immutable audit trail of all system operations.',
    learnMoreUrl: '/docs/concepts#telemetry'
  },

  goldenRun: {
    term: 'Golden Run',
    definition: 'A golden run is a verified, deterministic inference execution whose telemetry bundle serves as a reference for future replay.',
    learnMoreUrl: '/docs/concepts#golden-run'
  },

  replay: {
    term: 'Replay',
    definition: 'Replay re-executes a golden run to verify determinism by comparing outputs byte-for-byte.',
    learnMoreUrl: '/docs/concepts#replay'
  },

  policy: {
    term: 'Policy Pack',
    definition: 'Policy packs are sets of rules enforced across tenants, adapters, and execution (e.g., Egress Policy, Determinism Policy).',
    learnMoreUrl: '/docs/concepts#policy-layer'
  },

  lifecycle: {
    term: 'Lifecycle',
    definition: 'The lifecycle is the state machine for adapter memory management: Unloaded → Cold → Warm → Hot → Resident.',
    learnMoreUrl: '/docs/concepts#adapter'
  },

  pinning: {
    term: 'Pinning',
    definition: 'Pinning is a protection mechanism to prevent adapter eviction, used for production-critical adapters.',
    learnMoreUrl: '/docs/concepts#adapter'
  },

  eviction: {
    term: 'Eviction',
    definition: 'Eviction is the removal of adapter from memory due to pressure. The system evicts coldest (least-used) adapters first.',
    learnMoreUrl: '/docs/concepts#workflow-3'
  },

  kSparse: {
    term: 'K-Sparse Routing',
    definition: 'K-sparse routing selects the top-K most relevant adapters per request based on learned gate scores (e.g., K=3).',
    learnMoreUrl: '/docs/concepts#router'
  },

  workflowType: {
    term: 'Workflow Type',
    definition: 'Workflow type defines how adapters in a stack are executed: Sequential (ordered), Parallel (concurrent), or UpstreamDownstream (two-phase).',
    learnMoreUrl: '/docs/concepts#stack'
  },

  activationPercent: {
    term: 'Activation %',
    definition: 'Activation % is the percentage of requests where the router selected this adapter. Used for lifecycle promotion/demotion.',
    learnMoreUrl: '/docs/concepts#adapter'
  },

  tier: {
    term: 'Tier',
    definition: 'Tier is the lifecycle state of an adapter: Unloaded, Cold, Warm, Hot, or Resident.',
    learnMoreUrl: '/docs/concepts#adapter'
  },

  bundle: {
    term: 'Telemetry Bundle',
    definition: 'A telemetry bundle is a compressed, signed archive of telemetry events used for replay and audit.',
    learnMoreUrl: '/docs/concepts#telemetry'
  },

  divergence: {
    term: 'Divergence',
    definition: 'A divergence is a mismatch between golden run and replay execution, indicating non-determinism.',
    learnMoreUrl: '/docs/concepts#golden-run'
  },

  merkleChain: {
    term: 'Merkle Chain',
    definition: 'A Merkle chain is a linked sequence of hashed telemetry events, creating an immutable audit trail.',
    learnMoreUrl: '/docs/concepts#telemetry'
  },

  ttl: {
    term: 'TTL (Time-To-Live)',
    definition: 'TTL is the expiration time for ephemeral adapters. Adapters are auto-deleted when TTL expires.',
    learnMoreUrl: '/docs/concepts#adapter'
  },

  baseModel: {
    term: 'Base Model',
    definition: 'The base model is the foundation model (e.g., Qwen, Llama) that adapters specialize without modifying its weights.',
    learnMoreUrl: '/docs/concepts#glossary'
  },

  rank: {
    term: 'Rank',
    definition: 'Rank is the LoRA rank parameter (e.g., 8, 16, 32) that controls adapter capacity and memory footprint.',
    learnMoreUrl: '/docs/concepts#adapter'
  }
};

/**
 * Get tooltip for a concept
 */
export function getConceptTooltip(key: string): ConceptTooltip | undefined {
  return conceptTooltips[key];
}

/**
 * Get all tooltips as array
 */
export function getAllTooltips(): ConceptTooltip[] {
  return Object.values(conceptTooltips);
}

/**
 * Search tooltips by term or definition
 */
export function searchTooltips(query: string): ConceptTooltip[] {
  const lowerQuery = query.toLowerCase();
  return getAllTooltips().filter(
    tooltip =>
      tooltip.term.toLowerCase().includes(lowerQuery) ||
      tooltip.definition.toLowerCase().includes(lowerQuery)
  );
}
