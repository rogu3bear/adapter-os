import type { GlossaryEntry } from '@/data/glossary/types';

export const systemEntries: GlossaryEntry[] = [
  {
    id: 'node',
    term: 'Node',
    category: 'system',
    content: {
      brief: 'A compute node in the cluster running inference and training workloads.',
      detailed: 'Nodes are individual compute resources in the AdapterOS cluster that execute adapter inference and training tasks. Each node runs an agent that reports health status, capacity metrics, and handles adapter lifecycle operations. Nodes can be physical machines or virtual instances, and are managed collectively by the orchestrator for load balancing and failover.'
    },
    relatedTerms: ['worker', 'backend', 'active-adapters'],
    aliases: ['compute node', 'cluster node', 'worker node']
  },
  {
    id: 'node-name',
    term: 'Node Name',
    category: 'system',
    content: {
      brief: 'Unique hostname identifier for this compute node in the cluster.',
      detailed: 'Each node is assigned a unique identifier, typically derived from the system hostname or explicitly configured during initialization. The node name is used for routing requests, tracking metrics, and coordinating distributed operations across the cluster. Node names must be stable across restarts to maintain consistent routing and state.'
    },
    relatedTerms: ['node', 'node-endpoint'],
    aliases: ['hostname', 'node id', 'node identifier']
  },
  {
    id: 'node-status',
    term: 'Node Status',
    category: 'system',
    content: {
      brief: 'Current health status: healthy (online and responsive), offline (unreachable), or error (experiencing issues).',
      detailed: 'Node status reflects the operational state based on heartbeat monitoring and health checks. A healthy node responds to heartbeats within the timeout window and passes resource checks. Offline nodes have missed multiple consecutive heartbeats. Error status indicates the node is reachable but reporting failures such as resource exhaustion, adapter crashes, or policy violations.'
    },
    relatedTerms: ['node-last-seen', 'node'],
    aliases: ['health status', 'node health', 'operational status']
  },
  {
    id: 'node-cpu',
    term: 'Node CPU',
    category: 'system',
    content: {
      brief: 'Current CPU utilization percentage across all cores on this node.',
      detailed: 'CPU utilization measures the percentage of processor capacity in use by system processes, inference tasks, and training workloads. High CPU usage may indicate intensive computation or insufficient capacity. The orchestrator uses CPU metrics for scheduling decisions and capacity planning.'
    },
    relatedTerms: ['cpu-usage', 'node', 'worker'],
    aliases: ['processor usage', 'cpu load', 'cpu utilization']
  },
  {
    id: 'node-memory',
    term: 'Node Memory',
    category: 'system',
    content: {
      brief: 'Total available system memory in gigabytes for running workloads.',
      detailed: 'Available memory represents the total RAM capacity on the node that can be allocated to adapters, model weights, and inference operations. AdapterOS enforces a minimum 15% memory headroom policy to prevent OOM conditions. Memory availability is a critical factor in adapter placement and capacity planning.'
    },
    relatedTerms: ['memory-usage', 'ram-headroom', 'node'],
    aliases: ['system memory', 'ram', 'available memory']
  },
  {
    id: 'node-gpu',
    term: 'Node GPU',
    category: 'system',
    content: {
      brief: 'Number of GPU devices available for inference and training acceleration.',
      detailed: 'GPU count indicates the number of discrete or integrated graphics processors available for ML acceleration. On Apple Silicon, this includes the unified memory GPU cores. GPUs significantly accelerate matrix operations for inference and training. The system tracks GPU utilization and memory separately for scheduling and capacity management.'
    },
    relatedTerms: ['gpu-usage', 'backend', 'metal'],
    aliases: ['graphics processor', 'gpu count', 'accelerator']
  },
  {
    id: 'node-adapters',
    term: 'Node Adapters',
    category: 'system',
    content: {
      brief: 'Count of adapters currently loaded and running on this node.',
      detailed: 'The number of adapters in Warm, Hot, or Resident states on this node. This count excludes Cold and Unloaded adapters. The orchestrator uses adapter count along with memory and CPU metrics to determine node capacity for scheduling new adapter loads or inference requests.'
    },
    relatedTerms: ['active-adapters', 'node', 'worker'],
    aliases: ['loaded adapters', 'adapter count', 'running adapters']
  },
  {
    id: 'node-last-seen',
    term: 'Node Last Seen',
    category: 'system',
    content: {
      brief: 'Timestamp of the most recent heartbeat received from this node.',
      detailed: 'Each node sends periodic heartbeat messages to the orchestrator to confirm operational status. The last seen timestamp records when the most recent heartbeat was received. If the elapsed time exceeds the heartbeat timeout threshold, the node is marked offline and its adapters are evacuated to healthy nodes.'
    },
    relatedTerms: ['node-status', 'node'],
    aliases: ['last heartbeat', 'last contact', 'heartbeat timestamp']
  },
  {
    id: 'node-endpoint',
    term: 'Node Endpoint',
    category: 'system',
    content: {
      brief: 'Network endpoint URL where the node agent is listening for commands.',
      detailed: 'The endpoint specifies the protocol and address for communicating with the node agent. In production, AdapterOS enforces the Egress policy requiring Unix domain sockets (UDS) for all inter-process communication, eliminating network exposure. In development, HTTP endpoints may be used for testing.'
    },
    relatedTerms: ['node', 'node-name'],
    aliases: ['node url', 'agent endpoint', 'node address']
  },
  {
    id: 'cpu-usage',
    term: 'CPU Usage',
    category: 'system',
    content: {
      brief: 'Current CPU utilization percentage across all cores in the system.',
      detailed: 'System-wide CPU utilization measured as the percentage of processing capacity in use. This metric aggregates usage across all cores and nodes in the cluster. High CPU usage may indicate capacity constraints or inefficient inference pipelines. Monitoring CPU trends helps identify performance bottlenecks and guides capacity planning decisions.'
    },
    relatedTerms: ['node-cpu', 'worker', 'backend'],
    aliases: ['processor utilization', 'cpu load', 'cpu percentage']
  },
  {
    id: 'memory-usage',
    term: 'Memory Usage',
    category: 'system',
    content: {
      brief: 'Current RAM utilization percentage including system and application memory.',
      detailed: 'Total memory consumption across the system including OS overhead, model weights, adapter parameters, and inference buffers. AdapterOS tracks memory usage to enforce the 15% headroom policy and prevent memory exhaustion. High memory usage triggers adapter eviction and load shedding to maintain system stability.'
    },
    relatedTerms: ['node-memory', 'ram-headroom', 'memory-pressure'],
    aliases: ['ram usage', 'memory utilization', 'memory consumption']
  },
  {
    id: 'disk-usage',
    term: 'Disk Usage',
    category: 'system',
    content: {
      brief: 'Current disk space utilization percentage on the primary storage volume.',
      detailed: 'Percentage of storage capacity consumed by model files, adapter weights, training datasets, logs, and system data. AdapterOS stores adapters in .aos format and maintains training artifacts on disk. Disk usage monitoring prevents storage exhaustion which would block training jobs and adapter registration.'
    },
    relatedTerms: ['node'],
    aliases: ['storage usage', 'disk space', 'storage utilization']
  },
  {
    id: 'gpu-usage',
    term: 'GPU Usage',
    category: 'system',
    content: {
      brief: 'Current GPU utilization percentage.',
      detailed: 'GPU compute utilization measures the percentage of GPU processing capacity actively executing kernels for inference or training. On Apple Silicon with unified memory, GPU usage reflects both graphics and ML workloads. High GPU utilization indicates efficient hardware use, while sustained 100% may signal the need for additional capacity or workload distribution.'
    },
    relatedTerms: ['node-gpu', 'backend', 'metal', 'ane'],
    aliases: ['gpu utilization', 'gpu load', 'accelerator usage']
  },
  {
    id: 'network-bandwidth',
    term: 'Network Bandwidth',
    category: 'system',
    content: {
      brief: 'Current network throughput in megabytes per second.',
      detailed: 'Measures data transfer rate for cluster communication, metric reporting, and (in development) API traffic. In production deployments, the Egress policy restricts network usage to Unix domain sockets, minimizing bandwidth consumption and eliminating external network exposure. Bandwidth monitoring helps detect anomalies and policy violations.'
    },
    relatedTerms: ['node-endpoint'],
    aliases: ['network throughput', 'data transfer rate', 'network speed']
  },
  {
    id: 'ram-headroom',
    term: 'RAM Headroom',
    category: 'system',
    content: {
      brief: 'Reserved memory buffer (>=15% recommended) to prevent OOM.',
      detailed: 'Memory headroom is the percentage of total RAM kept available for system stability and unexpected demand. AdapterOS enforces a minimum 15% headroom policy to prevent out-of-memory conditions that could crash adapters or the entire system. When headroom drops below the threshold, the system triggers adapter eviction and blocks new loads until capacity is restored.'
    },
    relatedTerms: ['memory-usage', 'memory-pressure', 'node-memory'],
    aliases: ['memory buffer', 'memory reserve', 'memory headroom']
  },
  {
    id: 'vram-headroom',
    term: 'VRAM Headroom',
    category: 'system',
    content: {
      brief: 'Reserved GPU memory buffer for safe operation.',
      detailed: 'Similar to RAM headroom, VRAM headroom maintains a reserve of GPU memory for stability and burst capacity. On Apple Silicon with unified memory architecture, VRAM and RAM are shared, so the 15% headroom policy applies to the combined pool. Discrete GPUs may have separate VRAM headroom thresholds.'
    },
    relatedTerms: ['ram-headroom', 'gpu-usage', 'node-gpu'],
    aliases: ['gpu memory buffer', 'gpu memory reserve', 'vram buffer']
  },
  {
    id: 'memory-pressure',
    term: 'Memory Pressure',
    category: 'system',
    content: {
      brief: 'System state when available memory is critically low.',
      detailed: 'Memory pressure occurs when RAM usage exceeds safe thresholds, typically when headroom drops below 15%. Under memory pressure, AdapterOS activates defensive measures: evicting Cold adapters, demoting Hot adapters to Warm, rejecting new loads, and potentially terminating low-priority workloads. Monitoring memory pressure is critical for maintaining system reliability.'
    },
    relatedTerms: ['ram-headroom', 'memory-usage', 'node-memory'],
    aliases: ['memory stress', 'low memory', 'memory exhaustion']
  },
  {
    id: 'worker',
    term: 'Worker',
    category: 'system',
    content: {
      brief: 'Background process handling inference or training tasks.',
      detailed: 'Workers are spawned processes or threads that execute adapter inference requests and training jobs. Each worker manages its own memory pool, backend instance, and task queue. Workers report metrics and health status to the orchestrator and can be gracefully restarted for hot-swapping adapters or recovering from errors.'
    },
    relatedTerms: ['node', 'backend', 'active-adapters'],
    aliases: ['worker process', 'inference worker', 'training worker']
  },
  {
    id: 'active-sessions',
    term: 'Active Sessions',
    category: 'system',
    content: {
      brief: 'Concurrent active user or service sessions currently using the system.',
      detailed: 'Active sessions represent authenticated users or services with valid JWT tokens currently interacting with AdapterOS. Session count affects resource allocation, rate limiting, and capacity planning. Each session maintains state including workspace context, permissions, and request history for audit logging.'
    },
    relatedTerms: ['worker'],
    aliases: ['concurrent sessions', 'user sessions', 'active users']
  },
  {
    id: 'active-adapters',
    term: 'Active Adapters',
    category: 'system',
    content: {
      brief: 'Total number of adapters loaded in memory across all nodes.',
      detailed: 'The count of adapters in Warm, Hot, or Resident lifecycle states across the entire cluster. Active adapters consume memory and may be immediately available for inference. This metric is critical for capacity planning and understanding actual vs. theoretical cluster capacity. Cold and Unloaded adapters are not included in this count.'
    },
    relatedTerms: ['node-adapters', 'worker', 'node'],
    aliases: ['loaded adapters', 'running adapters', 'adapters in memory']
  },
  {
    id: 'ane',
    term: 'ANE',
    category: 'system',
    content: {
      brief: 'Apple Neural Engine - dedicated ML accelerator on Apple Silicon.',
      detailed: 'The Apple Neural Engine (ANE) is a specialized hardware accelerator built into Apple Silicon chips (M1, M2, M3, etc.) optimized for machine learning inference. ANE provides significant performance and energy efficiency improvements over CPU/GPU for compatible ML operations. AdapterOS leverages ANE through the CoreML backend for deterministic, high-performance inference.'
    },
    relatedTerms: ['coreml', 'backend', 'metal'],
    aliases: ['neural engine', 'apple neural engine', 'neural processor']
  },
  {
    id: 'coreml',
    term: 'CoreML',
    category: 'system',
    content: {
      brief: 'Apple\'s CoreML framework for ML inference on Apple devices.',
      detailed: 'CoreML is Apple\'s optimized machine learning framework that automatically selects the best compute backend (ANE, GPU, or CPU) for inference. AdapterOS uses CoreML as the primary backend on macOS 15+ for guaranteed deterministic execution with ANE acceleration. CoreML models are compiled to .mlmodelc format and loaded through a Swift bridge for optimal performance.'
    },
    relatedTerms: ['ane', 'backend', 'metal', 'mlx'],
    aliases: ['core ml', 'coreml framework', 'apple coreml']
  },
  {
    id: 'mlx',
    term: 'MLX',
    category: 'system',
    content: {
      brief: 'Apple\'s MLX framework for ML research and inference.',
      detailed: 'MLX is Apple\'s machine learning framework designed for research and experimentation on Apple Silicon. AdapterOS supports MLX as a secondary backend with HKDF-seeded randomness for determinism. MLX provides Python-like APIs in Rust through FFI bindings and includes features like hot-swap, circuit breakers, and memory pooling. Enable with --features mlx.'
    },
    relatedTerms: ['coreml', 'backend', 'metal'],
    aliases: ['apple mlx', 'mlx framework']
  },
  {
    id: 'metal',
    term: 'Metal',
    category: 'system',
    content: {
      brief: 'Apple\'s Metal GPU API for graphics and compute.',
      detailed: 'Metal is Apple\'s low-level GPU programming framework providing direct access to graphics and compute capabilities. AdapterOS uses Metal as the last-resort fallback backend when CoreML and MLX are unavailable. Note: Metal backend has incomplete model loading (LM head weights issue) and should only be used for legacy scenarios. Determinism support is planned but not yet fully implemented for this backend.'
    },
    relatedTerms: ['metal-family', 'backend', 'gpu-usage'],
    aliases: ['metal api', 'metal framework', 'metal gpu']
  },
  {
    id: 'metal-family',
    term: 'Metal Family',
    category: 'system',
    content: {
      brief: 'Apple GPU architecture generation (e.g., Apple7 = M1/M2).',
      detailed: 'Metal family identifies the GPU architecture generation of Apple Silicon chips, indicating feature support and performance characteristics. Apple7 corresponds to M1/M2 chips, Apple8 to M3, and Apple9 to M4. AdapterOS queries the Metal family during initialization to optimize compute kernels and determine ANE availability for CoreML acceleration.'
    },
    relatedTerms: ['metal', 'backend', 'ane'],
    aliases: ['gpu family', 'gpu generation', 'metal architecture']
  },
  {
    id: 'backend',
    term: 'Backend',
    category: 'system',
    content: {
      brief: 'Compute backend for inference: CoreML, MLX, or Metal.',
      detailed: 'The backend is the underlying compute framework used to execute adapter inference. AdapterOS supports three backends with automatic fallback: CoreML (primary, ANE-accelerated), MLX (secondary, production inference/training), and Metal (legacy fallback, incomplete). Backend selection affects performance, determinism guarantees, and hardware utilization. The backend factory manages initialization and hot-swapping between backends.'
    },
    relatedTerms: ['coreml', 'mlx', 'metal', 'worker'],
    aliases: ['compute backend', 'inference backend', 'ml backend']
  }
];
