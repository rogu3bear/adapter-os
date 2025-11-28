import type { GlossaryEntry } from '../types';

export const lifecycleEntries: GlossaryEntry[] = [
  {
    id: 'adapter-lifecycle',
    term: 'Adapter Lifecycle',
    category: 'lifecycle',
    content: {
      brief: 'The lifecycle is the state machine for adapter memory management: Unloaded → Cold → Warm → Hot → Resident.',
      detailed: `The adapter lifecycle manages memory resources by automatically promoting and demoting adapters based on usage patterns. Adapters transition through five states (tiers) based on activation frequency and system memory pressure. This ensures frequently-used adapters stay readily available while unused adapters are evicted to free memory.

The lifecycle system tracks activation counts, last-used timestamps, and memory pressure to make intelligent decisions about which adapters to keep in memory and which to evict.`
    },
    relatedTerms: ['adapter-tier', 'unloaded', 'cold', 'warm', 'hot', 'resident', 'activation-count', 'eviction'],
    aliases: ['lifecycle', 'state machine', 'memory management']
  },
  {
    id: 'adapter-tier',
    term: 'Tier',
    category: 'lifecycle',
    content: {
      brief: 'Tier is the lifecycle state of an adapter: Unloaded, Cold, Warm, Hot, or Resident.',
      detailed: `Each tier represents a different level of memory priority and availability:

• **Unloaded**: Not in memory, must be loaded before use
• **Cold**: In memory but inactive, eligible for eviction
• **Warm**: Recently used, kept for faster access
• **Hot**: Frequently used, high retention priority
• **Resident**: Pinned in memory, protected from eviction

Adapters are automatically promoted to higher tiers when activation percentage increases, and demoted when usage decreases or memory pressure rises.`
    },
    relatedTerms: ['adapter-lifecycle', 'promote', 'demote', 'activation-percent', 'pinning'],
    aliases: ['tier', 'state', 'lifecycle state']
  },
  {
    id: 'unloaded',
    term: 'Unloaded',
    category: 'lifecycle',
    content: {
      brief: 'Adapter not in memory, requires loading before use.',
      detailed: `The Unloaded state indicates the adapter exists in the registry but is not currently loaded into memory. Before the adapter can be used for inference, it must be loaded, which incurs a one-time latency cost.

Newly registered adapters start in the Unloaded state. Adapters may also return to this state after eviction due to memory pressure or manual unloading.`
    },
    relatedTerms: ['adapter-tier', 'cold', 'eviction', 'adapter-lifecycle'],
    aliases: ['unloaded state', 'not loaded']
  },
  {
    id: 'cold',
    term: 'Cold',
    category: 'lifecycle',
    content: {
      brief: 'In memory but not actively used, eligible for eviction.',
      detailed: `Cold adapters are loaded into memory but haven't been used recently. They are the first candidates for eviction when the system needs to free memory.

An adapter enters the Cold state when:
• First loaded from Unloaded
• Demoted from Warm due to decreased usage
• Activation percentage falls below the warm threshold

Cold adapters can still be used immediately (no load latency) but have the lowest retention priority.`
    },
    relatedTerms: ['adapter-tier', 'unloaded', 'warm', 'eviction', 'activation-percent'],
    aliases: ['cold state', 'cold tier']
  },
  {
    id: 'warm',
    term: 'Warm',
    category: 'lifecycle',
    content: {
      brief: 'Recently used, kept in memory for faster access.',
      detailed: `Warm adapters have moderate usage and are kept in memory with medium priority. They balance between resource consumption and availability.

An adapter enters the Warm state when:
• Promoted from Cold due to increased usage
• Demoted from Hot due to decreased activation percentage
• Activation percentage is within the warm threshold range

Warm adapters have better retention than Cold adapters but lower priority than Hot or Resident adapters during memory pressure events.`
    },
    relatedTerms: ['adapter-tier', 'cold', 'hot', 'activation-percent', 'promote', 'demote'],
    aliases: ['warm state', 'warm tier']
  },
  {
    id: 'hot',
    term: 'Hot',
    category: 'lifecycle',
    content: {
      brief: 'Frequently used, high priority for retention.',
      detailed: `Hot adapters are heavily used and have high priority for memory retention. The system works to keep these adapters loaded even under memory pressure.

An adapter enters the Hot state when:
• Promoted from Warm due to high activation percentage
• Usage patterns indicate frequent selection by the router

Hot adapters are only evicted as a last resort when memory pressure is severe and all Cold and Warm adapters have been evicted. Production-critical adapters should be pinned to Resident to prevent any eviction.`
    },
    relatedTerms: ['adapter-tier', 'warm', 'resident', 'activation-percent', 'eviction', 'pinning'],
    aliases: ['hot state', 'hot tier', 'frequently used']
  },
  {
    id: 'resident',
    term: 'Resident',
    category: 'lifecycle',
    content: {
      brief: 'Pinned in memory, protected from eviction.',
      detailed: `Resident adapters are explicitly pinned in memory and protected from automatic eviction. This is the highest priority tier in the lifecycle.

Use Resident tier for:
• Production-critical adapters that must always be available
• Adapters requiring consistent low-latency performance
• High-value adapters where load latency is unacceptable

Resident adapters can only be evicted by manual intervention (explicit unpin or unload). They consume memory permanently until unpinned, so use this tier judiciously.`
    },
    relatedTerms: ['adapter-tier', 'pinning', 'hot', 'eviction', 'adapter-lifecycle'],
    aliases: ['resident state', 'resident tier', 'pinned']
  },
  {
    id: 'pinning',
    term: 'Pinning',
    category: 'lifecycle',
    content: {
      brief: 'Pinning is a protection mechanism to prevent adapter eviction, used for production-critical adapters.',
      detailed: `Pinning an adapter promotes it to the Resident tier and prevents automatic eviction. This guarantees the adapter remains in memory and available for inference.

**When to pin:**
• Production adapters requiring guaranteed availability
• Adapters with strict latency SLAs
• High-value adapters used in critical workflows

**Caution:** Pinned adapters consume memory permanently. Monitor total pinned memory to avoid exhausting system resources. Unpinning returns the adapter to automatic lifecycle management based on usage patterns.`
    },
    relatedTerms: ['resident', 'eviction', 'adapter-tier', 'adapter-lifecycle'],
    aliases: ['pin', 'pinned', 'pin adapter', 'memory protection']
  },
  {
    id: 'eviction',
    term: 'Eviction',
    category: 'lifecycle',
    content: {
      brief: 'Eviction is the removal of an adapter from memory due to pressure. The system evicts coldest (least-used) adapters first.',
      detailed: `Eviction is the automatic process of unloading adapters from memory when the system needs to free resources. The eviction policy prioritizes keeping frequently-used adapters in memory.

**Eviction order (first to last):**
1. Cold tier adapters (least recently used first)
2. Warm tier adapters
3. Hot tier adapters (only under severe pressure)
4. Resident tier adapters (never automatically evicted)

The system maintains at least 15% memory headroom. When available memory falls below this threshold, eviction begins. Evicted adapters return to the Unloaded state and must be reloaded before next use.`
    },
    relatedTerms: ['adapter-tier', 'unloaded', 'cold', 'warm', 'hot', 'resident', 'adapter-lifecycle'],
    aliases: ['evict', 'evicted', 'memory eviction', 'unload']
  },
  {
    id: 'activation-count',
    term: 'Activation Count',
    category: 'lifecycle',
    content: {
      brief: 'Total times adapter was selected by the router.',
      detailed: `The activation count tracks how many times the router has selected this adapter for inference requests. This is a cumulative metric since the adapter was registered.

Activation count is used to:
• Calculate activation percentage (activations / total requests)
• Inform lifecycle promotion/demotion decisions
• Identify frequently-used vs. rarely-used adapters
• Guide capacity planning and resource allocation

High activation counts typically indicate valuable adapters that should be kept in higher tiers (Warm, Hot, or Resident).`
    },
    relatedTerms: ['activation-percent', 'adapter-tier', 'promote', 'demote', 'adapter-lifecycle'],
    aliases: ['activations', 'usage count', 'selection count']
  },
  {
    id: 'activation-percent',
    term: 'Activation Percent',
    category: 'lifecycle',
    content: {
      brief: 'Activation % is the percentage of requests where the router selected this adapter. Used for lifecycle promotion/demotion.',
      detailed: `Activation percentage measures how frequently the router selects this adapter relative to all inference requests. It's calculated as: (activation_count / total_requests) × 100.

**Lifecycle thresholds (typical values):**
• Cold → Warm: activation % > 5%
• Warm → Hot: activation % > 15%
• Hot → Warm: activation % < 10%
• Warm → Cold: activation % < 3%

These thresholds ensure adapters are automatically promoted to higher tiers when usage increases and demoted when usage decreases, optimizing memory utilization.`
    },
    relatedTerms: ['activation-count', 'adapter-tier', 'promote', 'demote', 'adapter-lifecycle'],
    aliases: ['activation percentage', 'usage percentage', 'selection rate']
  },
  {
    id: 'promote',
    term: 'Promote',
    category: 'lifecycle',
    content: {
      brief: 'Move adapter to a higher tier (e.g., Cold → Warm).',
      detailed: `Promotion moves an adapter to a higher lifecycle tier, increasing its memory retention priority. Promotion can be automatic (based on activation percentage) or manual (operator-initiated).

**Automatic promotion triggers:**
• Cold → Warm: activation % exceeds warm threshold
• Warm → Hot: activation % exceeds hot threshold
• Hot → Resident: explicit pinning action

**Manual promotion:** Operators can manually promote adapters when:
• Preparing for anticipated high usage
• Warming up adapters before load tests
• Responding to business requirements

Promotion improves adapter availability but increases memory consumption.`
    },
    relatedTerms: ['adapter-tier', 'demote', 'activation-percent', 'adapter-lifecycle'],
    aliases: ['promotion', 'upgrade tier', 'move up']
  },
  {
    id: 'demote',
    term: 'Demote',
    category: 'lifecycle',
    content: {
      brief: 'Move adapter to a lower tier (e.g., Hot → Warm).',
      detailed: `Demotion moves an adapter to a lower lifecycle tier, decreasing its memory retention priority. Demotion can be automatic (based on usage decline) or manual (operator-initiated).

**Automatic demotion triggers:**
• Hot → Warm: activation % falls below hot threshold
• Warm → Cold: activation % falls below warm threshold
• Any tier → Unloaded: eviction due to memory pressure

**Manual demotion:** Operators can manually demote adapters to:
• Free memory for higher-priority adapters
• Test eviction scenarios
• Adjust to changing business priorities

Demotion reduces memory pressure but increases the risk of eviction.`
    },
    relatedTerms: ['adapter-tier', 'promote', 'activation-percent', 'eviction', 'adapter-lifecycle'],
    aliases: ['demotion', 'downgrade tier', 'move down']
  },
  {
    id: 'ttl',
    term: 'TTL (Time-To-Live)',
    category: 'lifecycle',
    content: {
      brief: 'TTL (Time-To-Live) is the expiration time for ephemeral adapters. Adapters are auto-deleted when TTL expires.',
      detailed: `TTL defines when an adapter will be automatically deleted from the system. This is useful for temporary adapters used in experiments, A/B tests, or time-limited features.

**Common use cases:**
• Experimental adapters (TTL: 7-30 days)
• A/B test variants (TTL: duration of test)
• Feature-flagged adapters (TTL: feature rollout period)
• Development/staging adapters (TTL: 1-7 days)

When TTL expires:
• Adapter is automatically unloaded (if in memory)
• Adapter record is deleted from registry
• Associated artifacts are cleaned up

Production adapters should typically have no TTL or very long TTL values.`
    },
    relatedTerms: ['adapter-lifecycle', 'eviction', 'unloaded'],
    aliases: ['time to live', 'expiration', 'auto-delete', 'ephemeral']
  },
  {
    id: 'last-activated',
    term: 'Last Activated',
    category: 'lifecycle',
    content: {
      brief: 'Time since the adapter was last used for inference.',
      detailed: `Last Activated tracks the most recent timestamp when the router selected this adapter for an inference request. This metric is crucial for eviction decisions and lifecycle management.

**Usage in lifecycle:**
• Cold tier eviction: Adapters with oldest last-activated timestamps are evicted first
• Staleness detection: Adapters unused for extended periods may be candidates for deletion
• Monitoring: Identify adapters that are registered but never used

**Typical patterns:**
• Active adapters: last-activated < 1 hour ago
• Occasional adapters: last-activated 1 hour - 24 hours ago
• Stale adapters: last-activated > 7 days ago

Combined with activation count and percentage, this provides a complete picture of adapter usage.`
    },
    relatedTerms: ['activation-count', 'activation-percent', 'eviction', 'adapter-tier', 'adapter-lifecycle'],
    aliases: ['last used', 'last activation', 'last active', 'recency']
  }
];
