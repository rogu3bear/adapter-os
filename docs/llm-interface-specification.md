# AdapterOS LLM Interface Specification

**Version:** 1.0.0  
**Last Updated:** 2025-10-09  
**Status:** Draft

---

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Core Interfaces](#core-interfaces)
4. [Function Catalog](#function-catalog)
5. [Signal Protocol](#signal-protocol)
6. [Tool Specification](#tool-specification)
7. [State Management](#state-management)
8. [Memory Protocol](#memory-protocol)
9. [Router Integration](#router-integration)
10. [Evidence & Grounding](#evidence--grounding)
11. [Policy Enforcement](#policy-enforcement)
12. [Telemetry & Observability](#telemetry--observability)
13. [Error Handling](#error-handling)
14. [Security Constraints](#security-constraints)

---

## 1. Overview

### 1.1 Purpose

This specification defines the complete interface between the base LLM and the AdapterOS control plane. It establishes:

- **Function signatures** for all LLM-accessible operations
- **Signal protocols** for adapter activation and routing
- **Tool APIs** for retrieval, computation, and external system interaction
- **State management** contracts for deterministic behavior
- **Observability** requirements for audit and compliance

### 1.2 Design Principles

1. **Determinism First**: Every operation must be reproducible given the same inputs and CPID
2. **Zero Egress**: No network access during inference; all data is pre-loaded or local
3. **Policy Enforcement**: All operations subject to tenant-specific policy gates
4. **Evidence Traceability**: Every factual claim must cite source documents
5. **Graceful Degradation**: System must handle adapter eviction, memory pressure, and missing data

---

## 2. Architecture

### 2.1 System Components

```
┌─────────────────────────────────────────────────────────────┐
│                         Base LLM                             │
│  (Foundation Model: Llama, GPT, Claude, etc.)               │
└────────────────┬────────────────────────────────────────────┘
                 │
                 │ Function Calls / Tool Use
                 ↓
┌─────────────────────────────────────────────────────────────┐
│                    AdapterOS Runtime                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │   Router     │  │   RAG        │  │   Policy     │      │
│  │  (Top-K)     │  │  Engine      │  │  Enforcer    │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
│                                                               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │  Adapters    │  │  Telemetry   │  │  Evidence    │      │
│  │  (LoRA)      │  │  Logger      │  │  Tracker     │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
└─────────────────────────────────────────────────────────────┘
                 │
                 │ Unix Domain Sockets
                 ↓
┌─────────────────────────────────────────────────────────────┐
│                   Control Plane API                          │
│         (Postgres, Metal Kernels, Registry)                 │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 Execution Flow

1. **Prompt Ingestion**: User query arrives via UDS
2. **Policy Check**: Tenant policy applied to query
3. **Adapter Routing**: Top-K adapters selected based on query embedding
4. **Tool Resolution**: LLM generates function calls
5. **Function Execution**: Runtime executes tools (RAG, compute, etc.)
6. **Response Generation**: LLM synthesizes answer with evidence
7. **Trace Logging**: Full execution path captured for audit

---

## 3. Core Interfaces

### 3.1 LLM Runtime Interface

```typescript
interface LLMRuntime {
  // Inference entry point
  async generate(request: GenerateRequest): Promise<GenerateResponse>;
  
  // Streaming variant
  async generateStream(request: GenerateRequest): AsyncIterator<StreamChunk>;
  
  // Adapter management
  async loadAdapter(adapterId: string): Promise<AdapterHandle>;
  async unloadAdapter(adapterId: string): Promise<void>;
  
  // State management
  async saveCheckpoint(): Promise<CheckpointId>;
  async restoreCheckpoint(checkpointId: CheckpointId): Promise<void>;
}

interface GenerateRequest {
  prompt: string;
  tenantId: string;
  cpid: string;
  sessionId?: string;
  
  // Control parameters
  maxTokens: number;
  temperature: number;
  topP: number;
  stopSequences: string[];
  
  // Adapter routing hints
  routingHints?: {
    preferredAdapters?: string[];
    requiredCapabilities?: string[];
  };
  
  // Policy constraints
  policyContext: PolicyContext;
  
  // Evidence requirements
  requireEvidence: boolean;
  minEvidenceSpans: number;
}

interface GenerateResponse {
  text: string;
  tokens: number;
  
  // Adapter trace
  adaptersUsed: AdapterActivation[];
  
  // Evidence trail
  evidence: EvidenceSpan[];
  
  // Execution metadata
  latencyMs: number;
  routerOverheadMs: number;
  retrievalMs: number;
  
  // Policy decisions
  policyDecisions: PolicyDecision[];
  
  // Telemetry
  traceId: string;
  cpid: string;
}
```

---

## 4. Function Catalog

### 4.1 Function Categories

All functions available to the LLM are organized into categories:

1. **Retrieval Functions** - Access knowledge bases
2. **Computation Functions** - Perform calculations
3. **State Functions** - Manage conversation state
4. **Policy Functions** - Query policy constraints
5. **System Functions** - Runtime information

### 4.2 Retrieval Functions

#### 4.2.1 `retrieve_evidence`

Semantic search over tenant knowledge base.

```typescript
function retrieve_evidence(params: {
  query: string;
  topK: number;
  filters?: {
    docType?: string[];
    effectivity?: string[];
    revision?: string;
    dateRange?: [Date, Date];
  };
  embeddingModel?: string;
}): Promise<{
  spans: EvidenceSpan[];
  totalResults: number;
  retrievalTimeMs: number;
}>;

interface EvidenceSpan {
  docId: string;
  revision: string;
  spanId: string;
  text: string;
  score: number;
  metadata: {
    title: string;
    author: string;
    created: Date;
    effectivity: string[];
    sourceType: 'manual' | 'procedure' | 'specification' | 'code';
  };
  supersededBy?: string; // If this doc has been replaced
  spanHash: string; // BLAKE3 hash for provenance
}
```

**Usage Example:**
```typescript
const evidence = await retrieve_evidence({
  query: "torque specifications for main rotor bolt",
  topK: 5,
  filters: {
    docType: ["maintenance_manual"],
    effectivity: ["Boeing 737-800"]
  }
});
```

**Policy Constraints:**
- Respects tenant data isolation
- Enforces document-level access control
- Logs all retrieval operations
- Applies ITAR/export control filters

#### 4.2.2 `retrieve_by_id`

Direct document retrieval by identifier.

```typescript
function retrieve_by_id(params: {
  docId: string;
  revision?: string; // Latest if not specified
  spanIds?: string[]; // Specific sections
}): Promise<{
  document: Document;
  spans: EvidenceSpan[];
}>;
```

#### 4.2.3 `search_code`

Search code repositories with semantic and structural queries.

```typescript
function search_code(params: {
  query: string;
  language?: string[];
  repoId?: string;
  maxResults: number;
}): Promise<{
  results: CodeMatch[];
  totalHits: number;
}>;

interface CodeMatch {
  repoId: string;
  filePath: string;
  lineStart: number;
  lineEnd: number;
  code: string;
  language: string;
  relevanceScore: number;
  context: {
    functionName?: string;
    className?: string;
    imports: string[];
  };
}
```

### 4.3 Computation Functions

#### 4.3.1 `calculate`

Perform mathematical computations with unit handling.

```typescript
function calculate(params: {
  expression: string;
  units?: {
    input: Record<string, string>;
    output: string;
  };
  precision?: number;
}): Promise<{
  result: number | string;
  units: string;
  steps?: string[]; // For audit trail
}>;
```

**Usage Example:**
```typescript
const torque = await calculate({
  expression: "150 * 1.1", // 10% increase
  units: {
    input: { base: "ft_lbf" },
    output: "in_lbf"
  },
  precision: 2
});
// Result: { result: 1980, units: "in_lbf" }
```

**Policy Constraints:**
- Maximum expression complexity
- Timeout limits
- Unit validation required

#### 4.3.2 `convert_units`

Convert between measurement units.

```typescript
function convert_units(params: {
  value: number;
  fromUnit: string;
  toUnit: string;
}): Promise<{
  value: number;
  fromUnit: string;
  toUnit: string;
}>;
```

#### 4.3.3 `validate_calculation`

Validate numeric results against known constraints.

```typescript
function validate_calculation(params: {
  value: number;
  units: string;
  validRange?: [number, number];
  knownValue?: { value: number; tolerance: number };
}): Promise<{
  valid: boolean;
  issues: string[];
  confidence: number;
}>;
```

### 4.4 State Functions

#### 4.4.1 `store_context`

Store information in session context.

```typescript
function store_context(params: {
  key: string;
  value: any;
  scope: 'session' | 'turn' | 'persistent';
  ttl?: number; // seconds
}): Promise<{
  stored: boolean;
  contextId: string;
}>;
```

#### 4.4.2 `retrieve_context`

Retrieve previously stored context.

```typescript
function retrieve_context(params: {
  key: string;
  scope: 'session' | 'turn' | 'persistent';
}): Promise<{
  value: any;
  timestamp: Date;
  contextId: string;
}>;
```

#### 4.4.3 `list_context_keys`

List available context keys in scope.

```typescript
function list_context_keys(params: {
  scope: 'session' | 'turn' | 'persistent';
}): Promise<{
  keys: string[];
}>;
```

### 4.5 Policy Functions

#### 4.5.1 `check_policy`

Query policy constraints before generating response.

```typescript
function check_policy(params: {
  action: string;
  resource?: string;
  context?: Record<string, any>;
}): Promise<{
  allowed: boolean;
  reason?: string;
  alternatives?: string[];
}>;
```

**Usage Example:**
```typescript
const canExport = await check_policy({
  action: "export_data",
  resource: "torque_specifications",
  context: { effectivity: "Boeing 737-800" }
});
```

#### 4.5.2 `get_policy_requirements`

Get required fields/constraints for a query type.

```typescript
function get_policy_requirements(params: {
  queryType: string;
}): Promise<{
  requiredFields: string[];
  optionalFields: string[];
  constraints: Record<string, any>;
}>;
```

#### 4.5.3 `should_refuse`

Determine if query should be refused based on policy.

```typescript
function should_refuse(params: {
  query: string;
  evidence: EvidenceSpan[];
  confidence: number;
}): Promise<{
  refuse: boolean;
  reason?: string;
  missingFields?: string[];
  suggestedQuestions?: string[];
}>;
```

### 4.6 System Functions

#### 4.6.1 `get_active_adapters`

List currently loaded adapters.

```typescript
function get_active_adapters(): Promise<{
  adapters: AdapterInfo[];
  totalMemoryMB: number;
}>;

interface AdapterInfo {
  id: string;
  name: string;
  category: AdapterCategory;
  state: 'hot' | 'warm' | 'cold' | 'unloaded';
  activationCount: number;
  lastUsed: Date;
  memoryMB: number;
}
```

#### 4.6.2 `get_tenant_capabilities`

Query tenant-specific capabilities.

```typescript
function get_tenant_capabilities(): Promise<{
  capabilities: string[];
  adaptersAvailable: string[];
  policyProfile: string;
  dataClassification: string;
}>;
```

#### 4.6.3 `get_system_status`

Get runtime system status.

```typescript
function get_system_status(): Promise<{
  cpid: string;
  uptime: number;
  memoryUsage: {
    total: number;
    used: number;
    adapters: number;
  };
  activeRequests: number;
  queueDepth: number;
}>;
```

---

## 5. Signal Protocol

### 5.1 Signal Types

Signals are lightweight, low-level notifications from the LLM to the runtime.

```typescript
enum SignalType {
  // Adapter routing signals
  ADAPTER_REQUEST = 'adapter.request',
  ADAPTER_ACTIVATE = 'adapter.activate',
  ADAPTER_RELEASE = 'adapter.release',
  
  // Evidence signals
  EVIDENCE_REQUIRED = 'evidence.required',
  EVIDENCE_CITE = 'evidence.cite',
  EVIDENCE_INSUFFICIENT = 'evidence.insufficient',
  
  // Policy signals
  POLICY_CHECK = 'policy.check',
  POLICY_VIOLATION = 'policy.violation',
  REFUSAL_INTENT = 'refusal.intent',
  
  // State signals
  CONTEXT_SAVE = 'context.save',
  CONTEXT_LOAD = 'context.load',
  CHECKPOINT_REQUEST = 'checkpoint.request',
  
  // Performance signals
  TOKEN_BUDGET_WARNING = 'token.budget.warning',
  LATENCY_WARNING = 'latency.warning',
  
  // Error signals
  ERROR_OCCURRED = 'error.occurred',
  RETRY_REQUESTED = 'retry.requested'
}
```

### 5.2 Signal Interface

```typescript
interface Signal {
  type: SignalType;
  timestamp: Date;
  payload: Record<string, any>;
  priority: 'low' | 'normal' | 'high' | 'critical';
}

// Signal emission
function emit_signal(signal: Signal): void;

// Signal listener (runtime registers these)
type SignalHandler = (signal: Signal) => Promise<void>;
```

### 5.3 Adapter Routing Signals

#### 5.3.1 `ADAPTER_REQUEST`

LLM requests specific adapters for next generation step.

```typescript
emit_signal({
  type: SignalType.ADAPTER_REQUEST,
  payload: {
    requestedAdapters: ['aviation_maintenance', 'boeing_737'],
    reason: 'maintenance_procedure_query',
    priority: 'normal'
  },
  priority: 'normal',
  timestamp: new Date()
});
```

#### 5.3.2 `ADAPTER_ACTIVATE`

Notify runtime that adapter is being used.

```typescript
emit_signal({
  type: SignalType.ADAPTER_ACTIVATE,
  payload: {
    adapterId: 'aviation_maintenance',
    tokenPosition: 1024,
    confidence: 0.85
  },
  priority: 'low',
  timestamp: new Date()
});
```

### 5.4 Evidence Signals

#### 5.4.1 `EVIDENCE_CITE`

Log evidence citation in generated text.

```typescript
emit_signal({
  type: SignalType.EVIDENCE_CITE,
  payload: {
    spanId: 'doc_123:span_456',
    textPosition: { start: 150, end: 175 },
    citationType: 'direct' | 'paraphrase' | 'synthesis'
  },
  priority: 'normal',
  timestamp: new Date()
});
```

#### 5.4.2 `EVIDENCE_INSUFFICIENT`

Signal that available evidence is insufficient to answer.

```typescript
emit_signal({
  type: SignalType.EVIDENCE_INSUFFICIENT,
  payload: {
    query: "specific maintenance procedure",
    retrievedSpans: 2,
    requiredSpans: 1,
    confidence: 0.35,
    reason: 'low_relevance_scores'
  },
  priority: 'high',
  timestamp: new Date()
});
```

### 5.5 Policy Signals

#### 5.5.1 `REFUSAL_INTENT`

Signal intent to refuse query before generating response.

```typescript
emit_signal({
  type: SignalType.REFUSAL_INTENT,
  payload: {
    reason: 'insufficient_evidence',
    missingFields: ['aircraft_effectivity', 'serial_number'],
    confidence: 0.40,
    suggestedClarifications: [
      "Which aircraft model are you asking about?",
      "Do you have the component serial number?"
    ]
  },
  priority: 'high',
  timestamp: new Date()
});
```

---

## 6. Tool Specification

### 6.1 Tool Definition Format

All tools follow this schema:

```typescript
interface ToolDefinition {
  name: string;
  description: string;
  category: ToolCategory;
  
  // Function signature
  parameters: {
    type: 'object';
    properties: Record<string, ParameterSchema>;
    required: string[];
  };
  
  // Return schema
  returns: {
    type: string;
    properties: Record<string, any>;
  };
  
  // Constraints
  constraints: {
    maxExecutionTimeMs: number;
    requiresEvidence: boolean;
    policyGates: string[];
    rateLimit?: {
      maxCallsPerMinute: number;
      maxCallsPerSession: number;
    };
  };
  
  // Observability
  telemetry: {
    logInputs: boolean;
    logOutputs: boolean;
    logDuration: boolean;
    samplingRate: number;
  };
}
```

### 6.2 Tool Registration

Tools are registered with the runtime at startup:

```typescript
async function registerTool(tool: ToolDefinition): Promise<void>;

// Bulk registration
async function registerTools(tools: ToolDefinition[]): Promise<void>;

// Tool discovery
async function listAvailableTools(
  category?: ToolCategory
): Promise<ToolDefinition[]>;
```

### 6.3 Tool Execution

```typescript
interface ToolExecutionContext {
  tenantId: string;
  sessionId: string;
  cpid: string;
  policyContext: PolicyContext;
  traceId: string;
}

async function executeTool(
  toolName: string,
  params: Record<string, any>,
  context: ToolExecutionContext
): Promise<ToolResult>;

interface ToolResult {
  success: boolean;
  value?: any;
  error?: {
    code: string;
    message: string;
    retryable: boolean;
  };
  metadata: {
    durationMs: number;
    cacheHit: boolean;
    policyChecks: PolicyDecision[];
  };
}
```

---

## 7. State Management

### 7.1 State Hierarchy

```
Global State
├── CPID State (immutable per control plane)
│   ├── Policy Configuration
│   ├── Adapter Registry
│   └── Kernel Hashes
│
├── Tenant State (per tenant)
│   ├── Knowledge Base Indices
│   ├── Access Control Lists
│   └── Capability Profile
│
├── Session State (per conversation)
│   ├── Context Window
│   ├── Evidence Trail
│   └── Adapter Activations
│
└── Turn State (per generation)
    ├── Intermediate Steps
    ├── Tool Calls
    └── Token Budget
```

### 7.2 State Persistence

```typescript
interface StateManager {
  // Session state
  async saveSessionState(
    sessionId: string,
    state: SessionState
  ): Promise<void>;
  
  async loadSessionState(sessionId: string): Promise<SessionState>;
  
  // Checkpointing
  async createCheckpoint(
    sessionId: string,
    label: string
  ): Promise<CheckpointId>;
  
  async restoreCheckpoint(
    checkpointId: CheckpointId
  ): Promise<SessionState>;
  
  // State queries
  async getStateMetadata(
    sessionId: string
  ): Promise<StateMetadata>;
}

interface SessionState {
  sessionId: string;
  tenantId: string;
  startedAt: Date;
  
  // Conversation history
  turns: Turn[];
  
  // Accumulated evidence
  evidenceCollected: EvidenceSpan[];
  
  // Context variables
  context: Record<string, any>;
  
  // Adapter state
  adaptersUsed: Set<string>;
  adapterActivations: AdapterActivation[];
}
```

---

## 8. Memory Protocol

### 8.1 Memory Allocation

```typescript
interface MemoryManager {
  // Adapter memory
  async allocateAdapterMemory(
    adapterId: string,
    sizeBytes: number
  ): Promise<MemoryHandle>;
  
  async deallocateAdapterMemory(handle: MemoryHandle): Promise<void>;
  
  // Memory pressure
  async getMemoryStatus(): Promise<MemoryStatus>;
  
  // Eviction
  async triggerEviction(
    strategy: EvictionStrategy
  ): Promise<EvictionResult>;
}

interface MemoryStatus {
  totalBytes: number;
  usedBytes: number;
  availableBytes: number;
  
  // Breakdown
  baseModelBytes: number;
  adaptersBytes: number;
  cacheBytes: number;
  
  // Pressure
  pressureLevel: 'low' | 'medium' | 'high' | 'critical';
  evictionThreshold: number;
}

interface EvictionStrategy {
  priority: 'cold_first' | 'lru' | 'size_first';
  targetBytes?: number;
  protectedAdapters?: string[];
}
```

### 8.2 Memory Pressure Handling

```typescript
// LLM must respond to memory pressure signals
on_signal(SignalType.MEMORY_PRESSURE, async (signal) => {
  const { level, recommendation } = signal.payload;
  
  if (level === 'high') {
    // Reduce adapter K
    await reduceAdapterCount();
  }
  
  if (level === 'critical') {
    // Finish current turn and save state
    await finalizeTurn();
    emit_signal({
      type: SignalType.CHECKPOINT_REQUEST,
      payload: { reason: 'memory_pressure' },
      priority: 'critical',
      timestamp: new Date()
    });
  }
});
```

---

## 9. Router Integration

### 9.1 Router Interface

```typescript
interface AdapterRouter {
  // Get top-K adapters for query
  async route(
    query: string,
    k: number,
    hints?: RoutingHints
  ): Promise<AdapterRouting>;
  
  // Update routing based on feedback
  async updateRouting(
    adapterId: string,
    feedback: RoutingFeedback
  ): Promise<void>;
}

interface AdapterRouting {
  adapters: AdapterScore[];
  routingTime: number;
  entropy: number;
  
  // Explainability
  reasoning: {
    queryEmbedding: number[]; // Truncated for logging
    topSimilarities: Array<{
      adapterId: string;
      similarity: number;
      reason: string;
    }>;
  };
}

interface AdapterScore {
  adapterId: string;
  score: number;
  gate: number; // Quantized Q15
  confidence: number;
}
```

### 9.2 Routing Hints

```typescript
interface RoutingHints {
  // Preferred adapters
  prefer?: string[];
  
  // Required capabilities
  requireCapabilities?: string[];
  
  // Exclude adapters
  exclude?: string[];
  
  // Domain hints
  domain?: string;
  taskType?: string;
}
```

### 9.3 Dynamic Router Adjustment

```typescript
// LLM can request router adjustments mid-generation
emit_signal({
  type: SignalType.ADAPTER_REQUEST,
  payload: {
    adjustK: -1, // Reduce K by 1
    reason: 'insufficient_quality',
    currentStep: 'evidence_synthesis'
  },
  priority: 'normal',
  timestamp: new Date()
});
```

---

## 10. Evidence & Grounding

### 10.1 Evidence Requirements

All factual claims must be grounded in retrieved evidence:

```typescript
interface EvidenceRequirement {
  claimType: 'factual' | 'procedural' | 'specification' | 'opinion';
  minSpans: number;
  maxAge?: number; // days
  requiredFields?: string[];
  acceptedSourceTypes?: string[];
}

// Check if evidence meets requirements
function validateEvidence(
  claim: string,
  evidence: EvidenceSpan[],
  requirements: EvidenceRequirement
): ValidationResult;
```

### 10.2 Citation Format

```typescript
interface Citation {
  // Source identification
  docId: string;
  revision: string;
  spanId: string;
  
  // Citation metadata
  citationType: 'direct' | 'paraphrase' | 'synthesis';
  confidence: number;
  
  // Text anchoring
  generatedText: {
    start: number;
    end: number;
  };
  
  // Provenance
  spanHash: string;
  retrievalTimestamp: Date;
}
```

### 10.3 Evidence Synthesis

When synthesizing from multiple sources:

```typescript
emit_signal({
  type: SignalType.EVIDENCE_CITE,
  payload: {
    citations: [
      { spanId: 'doc_1:span_42', weight: 0.6 },
      { spanId: 'doc_2:span_17', weight: 0.4 }
    ],
    synthesisType: 'aggregate',
    confidence: 0.78
  },
  priority: 'normal',
  timestamp: new Date()
});
```

---

## 11. Policy Enforcement

### 11.1 Policy Context

Every request carries policy context:

```typescript
interface PolicyContext {
  tenantId: string;
  userId: string;
  userRole: string;
  
  // Classification
  dataClassification: 'public' | 'internal' | 'confidential' | 'restricted';
  itarControlled: boolean;
  
  // Constraints
  maxTokens: number;
  allowedDomains: string[];
  forbiddenTopics: string[];
  
  // Evidence requirements
  requireEvidence: boolean;
  minEvidenceSpans: number;
  abstainThreshold: number;
}
```

### 11.2 Policy Gates

Policy gates are checked at key decision points:

```typescript
enum PolicyGate {
  PRE_GENERATION = 'pre_generation',
  TOOL_EXECUTION = 'tool_execution',
  EVIDENCE_RETRIEVAL = 'evidence_retrieval',
  POST_GENERATION = 'post_generation',
  RESPONSE_EXPORT = 'response_export'
}

interface PolicyGateCheck {
  gate: PolicyGate;
  allowed: boolean;
  violations: PolicyViolation[];
  warnings: string[];
}

interface PolicyViolation {
  code: string;
  severity: 'error' | 'warning';
  message: string;
  field?: string;
  remediation?: string;
}
```

### 11.3 Refusal Protocol

When refusing to answer:

```typescript
interface RefusalResponse {
  refused: true;
  reason: string;
  missingFields?: string[];
  suggestedQuestions?: string[];
  policyReference?: string;
  
  // Partial information (if safe to share)
  partialAnswer?: string;
  relatedDocuments?: string[];
}

// Generate structured refusal
function generateRefusal(
  query: string,
  reason: string,
  context: PolicyContext
): RefusalResponse;
```

---

## 12. Telemetry & Observability

### 12.1 Telemetry Events

All significant events are logged:

```typescript
interface TelemetryEvent {
  eventType: string;
  kind?: string;
  timestamp: Date;
  traceId: string;
  cpid: string;
  
  // Context
  tenantId: string;
  sessionId: string;
  turnId: string;
  
  // Event payload
  payload: Record<string, any>;
  
  // Performance
  durationMs?: number;
  
  // Hash for integrity
  eventHash: string; // BLAKE3
}
```

### 12.2 Trace Structure

Complete execution trace:

```typescript
interface ExecutionTrace {
  traceId: string;
  cpid: string;
  
  // Request
  request: GenerateRequest;
  
  // Routing
  routerDecision: AdapterRouting;
  
  // Tool calls
  toolCalls: Array<{
    tool: string;
    params: Record<string, any>;
    result: ToolResult;
    timestamp: Date;
  }>;
  
  // Evidence
  evidenceRetrieved: EvidenceSpan[];
  evidenceCited: Citation[];
  
  // Policy
  policyChecks: PolicyGateCheck[];
  
  // Adapters
  adaptersUsed: AdapterActivation[];
  
  // Response
  response: GenerateResponse;
  
  // Timing
  timing: {
    total: number;
    router: number;
    retrieval: number;
    generation: number;
    policy: number;
  };
}
```

### 12.3 Sampling Strategy

```typescript
interface SamplingConfig {
  // Full traces
  fullTraceRate: number; // e.g., 0.05 = 5%
  
  // Always log
  alwaysLog: string[]; // Event types to always log
  
  // Token-level sampling
  tokenSampling: {
    firstN: number; // Always log first N tokens
    thenRate: number; // Sample rate after first N
  };
  
  // Router sampling
  routerSampling: {
    firstN: number;
    thenRate: number;
  };
}
```

---

## 13. Error Handling

### 13.1 Error Categories

```typescript
enum ErrorCategory {
  // Retrieval errors
  RETRIEVAL_FAILED = 'retrieval.failed',
  RETRIEVAL_TIMEOUT = 'retrieval.timeout',
  RETRIEVAL_EMPTY = 'retrieval.empty',
  
  // Adapter errors
  ADAPTER_LOAD_FAILED = 'adapter.load_failed',
  ADAPTER_EVICTED = 'adapter.evicted',
  ADAPTER_CORRUPTED = 'adapter.corrupted',
  
  // Policy errors
  POLICY_VIOLATION = 'policy.violation',
  POLICY_REFUSED = 'policy.refused',
  
  // Resource errors
  MEMORY_EXHAUSTED = 'memory.exhausted',
  TOKEN_BUDGET_EXCEEDED = 'token.budget_exceeded',
  TIMEOUT = 'timeout',
  
  // System errors
  SYSTEM_ERROR = 'system.error',
  DETERMINISM_FAILURE = 'determinism.failure'
}
```

### 13.2 Error Response

```typescript
interface ErrorResponse {
  error: true;
  category: ErrorCategory;
  message: string;
  
  // Recovery
  retryable: boolean;
  retryAfter?: number; // seconds
  
  // Context
  traceId: string;
  timestamp: Date;
  
  // Diagnostics
  diagnostics?: {
    state: string;
    lastSuccessfulStep?: string;
    suggestedAction?: string;
  };
}
```

### 13.3 Retry Logic

```typescript
interface RetryPolicy {
  maxAttempts: number;
  backoffMs: number;
  backoffMultiplier: number;
  retryableErrors: ErrorCategory[];
  
  // State preservation
  preserveState: boolean;
  checkpointOnRetry: boolean;
}

// Retry with exponential backoff
async function retryWithBackoff<T>(
  fn: () => Promise<T>,
  policy: RetryPolicy
): Promise<T>;
```

---

## 14. Security Constraints

### 14.1 Zero Egress Enforcement

```typescript
// Network isolation
const FORBIDDEN_OPERATIONS = [
  'fetch',
  'XMLHttpRequest',
  'WebSocket',
  'dns.resolve',
  'net.connect'
];

// Runtime enforces this at process level
// LLM must never attempt network access
```

### 14.2 Data Isolation

```typescript
interface TenantIsolation {
  // Tenant boundary
  tenantId: string;
  
  // No cross-tenant access
  forbiddenTenants: string[];
  
  // Namespace isolation
  documentNamespace: string;
  adapterNamespace: string;
  
  // Audit trail
  accessLog: AccessLogEntry[];
}

interface AccessLogEntry {
  timestamp: Date;
  operation: string;
  resourceId: string;
  allowed: boolean;
  reason?: string;
}
```

### 14.3 Key Management

```typescript
// All keys in Secure Enclave
interface KeyManagement {
  // Signing
  async signResponse(
    response: GenerateResponse
  ): Promise<Signature>;
  
  // Verification
  async verifyArtifact(
    artifact: Artifact,
    signature: Signature
  ): Promise<boolean>;
  
  // Key rotation
  async rotateKeys(cpid: string): Promise<void>;
}
```

### 14.4 Determinism Verification

```typescript
interface DeterminismCheck {
  // Replay verification
  async verifyDeterminism(
    request: GenerateRequest,
    expectedResponse: GenerateResponse
  ): Promise<DeterminismResult>;
  
  // Hash verification
  async verifyKernelHash(
    cpid: string,
    expectedHash: string
  ): Promise<boolean>;
}

interface DeterminismResult {
  deterministic: boolean;
  divergencePoint?: {
    tokenIndex: number;
    expected: string;
    actual: string;
  };
  hashMatch: boolean;
}
```

---

## 15. Complete Function Index

### 15.1 Alphabetical Function List

```typescript
// A
async function abort_generation(): Promise<void>;

// C
async function calculate(params: CalculateParams): Promise<CalculateResult>;
async function check_policy(params: PolicyCheckParams): Promise<PolicyCheckResult>;
async function convert_units(params: UnitConversionParams): Promise<UnitConversionResult>;

// E
function emit_signal(signal: Signal): void;
async function executeTool(toolName: string, params: any, context: ToolExecutionContext): Promise<ToolResult>;

// G
async function get_active_adapters(): Promise<AdapterInfo[]>;
async function get_policy_requirements(params: { queryType: string }): Promise<PolicyRequirements>;
async function get_system_status(): Promise<SystemStatus>;
async function get_tenant_capabilities(): Promise<TenantCapabilities>;
async function generate(request: GenerateRequest): Promise<GenerateResponse>;
async function generateStream(request: GenerateRequest): AsyncIterator<StreamChunk>;

// L
async function list_context_keys(params: { scope: Scope }): Promise<{ keys: string[] }>;
async function listAvailableTools(category?: ToolCategory): Promise<ToolDefinition[]>;
async function loadAdapter(adapterId: string): Promise<AdapterHandle>;

// R
async function registerTool(tool: ToolDefinition): Promise<void>;
async function registerTools(tools: ToolDefinition[]): Promise<void>;
async function restoreCheckpoint(checkpointId: CheckpointId): Promise<void>;
async function retrieve_by_id(params: RetrieveByIdParams): Promise<RetrieveByIdResult>;
async function retrieve_context(params: RetrieveContextParams): Promise<RetrieveContextResult>;
async function retrieve_evidence(params: RetrieveEvidenceParams): Promise<RetrieveEvidenceResult>;
async function retryWithBackoff<T>(fn: () => Promise<T>, policy: RetryPolicy): Promise<T>;

// S
async function saveCheckpoint(): Promise<CheckpointId>;
async function search_code(params: SearchCodeParams): Promise<SearchCodeResult>;
async function should_refuse(params: RefusalCheckParams): Promise<RefusalCheckResult>;
async function store_context(params: StoreContextParams): Promise<StoreContextResult>;

// U
async function unloadAdapter(adapterId: string): Promise<void>;

// V
async function validate_calculation(params: ValidateCalculationParams): Promise<ValidateCalculationResult>;
function validateEvidence(claim: string, evidence: EvidenceSpan[], requirements: EvidenceRequirement): ValidationResult;
```

---

## 16. Integration Examples

### 16.1 Complete Request Flow

```typescript
// 1. Receive request
const request: GenerateRequest = {
  prompt: "What is the torque specification for the main rotor bolt on a Boeing 737-800?",
  tenantId: "aviation_mro_1",
  cpid: "cp-20250109-abc123",
  maxTokens: 500,
  temperature: 0.1,
  requireEvidence: true,
  minEvidenceSpans: 1,
  policyContext: {
    tenantId: "aviation_mro_1",
    userId: "technician_42",
    userRole: "mechanic",
    dataClassification: "internal",
    itarControlled: false,
    maxTokens: 500,
    requireEvidence: true,
    minEvidenceSpans: 1,
    abstainThreshold: 0.55
  }
};

// 2. Policy pre-check
const policyCheck = await check_policy({
  action: "generate_response",
  resource: "torque_specifications",
  context: { aircraft: "Boeing 737-800" }
});

if (!policyCheck.allowed) {
  return generateRefusal(request.prompt, policyCheck.reason, request.policyContext);
}

// 3. Route to adapters
emit_signal({
  type: SignalType.ADAPTER_REQUEST,
  payload: {
    query: request.prompt,
    domain: "aviation_maintenance",
    taskType: "specification_lookup"
  },
  priority: 'normal',
  timestamp: new Date()
});

// 4. Retrieve evidence
const evidence = await retrieve_evidence({
  query: request.prompt,
  topK: 5,
  filters: {
    docType: ["maintenance_manual", "specification"],
    effectivity: ["Boeing 737-800"]
  }
});

// 5. Check evidence sufficiency
if (evidence.spans.length === 0 || evidence.spans[0].score < 0.55) {
  emit_signal({
    type: SignalType.EVIDENCE_INSUFFICIENT,
    payload: {
      query: request.prompt,
      retrievedSpans: evidence.spans.length,
      maxScore: evidence.spans[0]?.score || 0,
      reason: "insufficient_relevance"
    },
    priority: 'high',
    timestamp: new Date()
  });
  
  return generateRefusal(
    request.prompt,
    "insufficient_evidence",
    request.policyContext
  );
}

// 6. Generate response with citations
const response = await generate({
  ...request,
  systemPrompt: `You are a maintenance assistant. Answer based ONLY on the provided evidence.
  
Evidence:
${evidence.spans.map((s, i) => `[${i+1}] ${s.text} (Source: ${s.metadata.title})`).join('\n\n')}

Format your response with inline citations like [1], [2], etc.`
});

// 7. Validate citations
const citations = extractCitations(response.text);
emit_signal({
  type: SignalType.EVIDENCE_CITE,
  payload: {
    citations: citations.map(c => ({
      spanId: evidence.spans[c.index].spanId,
      textPosition: c.position
    }))
  },
  priority: 'normal',
  timestamp: new Date()
});

// 8. Final policy check
const finalCheck = await check_policy({
  action: "export_response",
  resource: "generated_answer",
  context: { evidenceUsed: citations.length }
});

// 9. Return with full trace
return {
  ...response,
  evidence: evidence.spans.filter((_, i) => 
    citations.some(c => c.index === i)
  ),
  traceId: generateTraceId(),
  cpid: request.cpid
};
```

### 16.2 Error Recovery Example

```typescript
async function robustGenerate(request: GenerateRequest): Promise<GenerateResponse> {
  const retryPolicy: RetryPolicy = {
    maxAttempts: 3,
    backoffMs: 1000,
    backoffMultiplier: 2,
    retryableErrors: [
      ErrorCategory.RETRIEVAL_TIMEOUT,
      ErrorCategory.ADAPTER_EVICTED
    ],
    preserveState: true,
    checkpointOnRetry: true
  };
  
  return await retryWithBackoff(async () => {
    try {
      // Attempt generation
      const response = await generate(request);
      return response;
      
    } catch (error) {
      if (error.category === ErrorCategory.ADAPTER_EVICTED) {
        // Reload adapter and retry
        emit_signal({
          type: SignalType.ADAPTER_REQUEST,
          payload: {
            adapterId: error.adapterId,
            priority: 'high'
          },
          priority: 'high',
          timestamp: new Date()
        });
        throw error; // Will be retried
      }
      
      if (error.category === ErrorCategory.MEMORY_EXHAUSTED) {
        // Reduce K and retry
        emit_signal({
          type: SignalType.ADAPTER_REQUEST,
          payload: {
            adjustK: -1,
            reason: 'memory_pressure'
          },
          priority: 'high',
          timestamp: new Date()
        });
        throw error;
      }
      
      // Non-retryable error
      throw error;
    }
  }, retryPolicy);
}
```

---

## 17. Versioning & Evolution

### 17.1 API Versioning

```typescript
interface APIVersion {
  major: number;
  minor: number;
  patch: number;
  
  // Breaking changes
  breakingChanges: string[];
  
  // Deprecations
  deprecated: string[];
  deprecationDate?: Date;
}

const CURRENT_VERSION: APIVersion = {
  major: 1,
  minor: 0,
  patch: 0,
  breakingChanges: [],
  deprecated: []
};
```

### 17.2 Compatibility

```typescript
interface CompatibilityMatrix {
  llmVersion: string;
  runtimeVersion: string;
  compatible: boolean;
  requiredMigrations?: string[];
}

async function checkCompatibility(): Promise<CompatibilityMatrix>;
```

---

## 18. Testing & Validation

### 18.1 Test Suites

```typescript
interface TestSuite {
  name: string;
  tests: TestCase[];
  cpid: string; // Must be reproducible
}

interface TestCase {
  name: string;
  request: GenerateRequest;
  expectedResponse: Partial<GenerateResponse>;
  
  // Determinism check
  mustMatchExactly: boolean;
  allowedDeviation?: number; // For non-deterministic sampling
  
  // Policy validation
  expectedPolicyChecks: PolicyGateCheck[];
  
  // Evidence validation
  requiredEvidenceSpans: number;
}
```

### 18.2 Validation Functions

```typescript
// Validate determinism
async function validateDeterminism(
  testCase: TestCase,
  runs: number
): Promise<DeterminismReport>;

// Validate policy compliance
async function validatePolicyCompliance(
  testSuite: TestSuite
): Promise<ComplianceReport>;

// Validate evidence quality
async function validateEvidenceQuality(
  testSuite: TestSuite
): Promise<EvidenceQualityReport>;
```

---

## 19. Deployment & Operations

### 19.1 Runtime Configuration

```typescript
interface RuntimeConfig {
  // CPID
  cpid: string;
  
  // Model configuration
  modelPath: string;
  modelType: string;
  quantization?: string;
  
  // Adapter configuration
  adapterRegistry: string;
  maxAdapters: number;
  adapterK: number;
  
  // Memory configuration
  maxMemoryMB: number;
  evictionThreshold: number;
  
  // Policy configuration
  policyConfigPath: string;
  
  // Telemetry configuration
  telemetryConfig: SamplingConfig;
  
  // Performance configuration
  maxTokensPerSecond: number;
  maxConcurrentRequests: number;
}
```

### 19.2 Health Checks

```typescript
interface HealthCheck {
  async checkRuntime(): Promise<HealthStatus>;
  async checkAdapters(): Promise<AdapterHealthStatus>;
  async checkPolicy(): Promise<PolicyHealthStatus>;
  async checkTelemetry(): Promise<TelemetryHealthStatus>;
}

interface HealthStatus {
  healthy: boolean;
  issues: string[];
  lastChecked: Date;
}
```

---

## Appendices

### Appendix A: Error Codes

| Code | Description | Retryable | Action |
|------|-------------|-----------|--------|
| `E001` | Retrieval timeout | Yes | Retry with backoff |
| `E002` | Adapter load failed | Yes | Check adapter registry |
| `E003` | Policy violation | No | Refuse query |
| `E004` | Memory exhausted | Yes | Reduce K, evict adapters |
| `E005` | Determinism failure | No | Rollback to last CP |
| `E006` | Evidence insufficient | No | Request clarification |

### Appendix B: Performance Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| Token generation | > 40 tokens/sec | p50 |
| Retrieval latency | < 50ms | p95 |
| Router overhead | < 8% | Average |
| Total latency | < 500ms | p95 |

### Appendix C: Compliance Mappings

| Control | Evidence | Location |
|---------|----------|----------|
| SOC2-CC6.1 | Access logs | Telemetry bundles |
| ITAR-121.1 | Tenant isolation | Policy config |
| ISO27001-A.9.4 | Authentication | Auth service |

---

**End of Specification**

*For questions or clarifications, contact: aos-platform@example.com*

