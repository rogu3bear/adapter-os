# AdapterOS Database Schema Diagram

```mermaid
erDiagram
    %% Core Identity & Access Management
    users {
        text id PK "UUID primary key"
        text email UK "Unique email address"
        text display_name "Human-readable name"
        text pw_hash "Argon2 password hash"
        text role "admin|operator|sre|compliance|auditor|viewer"
        integer disabled "0=active, 1=disabled"
        text created_at "ISO timestamp"
    }
    
    tenants {
        text id PK "UUID primary key"
        text name UK "Unique tenant name"
        integer itar_flag "0=no ITAR, 1=ITAR restricted"
        text created_at "ISO timestamp"
    }
    
    jwt_secrets {
        text id PK "UUID primary key"
        text secret_hash "BLAKE3 hash of JWT secret"
        text not_before "Valid from timestamp"
        text not_after "Valid until timestamp"
        integer active "0=inactive, 1=active"
        text created_at "ISO timestamp"
    }
    
    %% Infrastructure Management
    nodes {
        text id PK "UUID primary key"
        text hostname UK "Unique hostname"
        text agent_endpoint "HTTP endpoint for aos-node runtime"
        text status "pending|active|offline|maintenance"
        text last_seen_at "Last heartbeat timestamp"
        text labels_json "Key-value labels for node"
        text created_at "ISO timestamp"
    }
    
    workers {
        text id PK "UUID primary key"
        text tenant_id FK "References tenants.id"
        text node_id FK "References nodes.id"
        text plan_id FK "References plans.id"
        text uds_path "Unix domain socket path"
        integer pid "Process ID"
        text status "starting|serving|draining|stopped|crashed"
        real memory_headroom_pct "Available memory percentage"
        integer k_current "Current K-sparse value"
        text adapters_loaded_json "JSON array of loaded adapter IDs"
        text started_at "Worker start timestamp"
        text last_heartbeat_at "Last heartbeat timestamp"
    }
    
    %% Models & Artifact Management
    models {
        text id PK "UUID primary key"
        text name UK "Unique model name"
        text hash_b3 UK "BLAKE3 hash of model weights"
        text license_hash_b3 "BLAKE3 hash of license file"
        text config_hash_b3 "BLAKE3 hash of model config"
        text tokenizer_hash_b3 "BLAKE3 hash of tokenizer"
        text tokenizer_cfg_hash_b3 "BLAKE3 hash of tokenizer config"
        text metadata_json "Model metadata (size, parameters, etc.)"
        text created_at "ISO timestamp"
    }
    
    artifacts {
        text hash_b3 PK "BLAKE3 hash as primary key"
        text kind "model|adapter|metallib|sbom|plan|bundle"
        text signature_b64 "Ed25519 signature (base64)"
        text sbom_hash_b3 "BLAKE3 hash of SBOM"
        integer size_bytes "Artifact size in bytes"
        text imported_by FK "References users.id"
        text imported_at "ISO timestamp"
    }
    
    bundle_signatures {
        text id PK "UUID primary key"
        text bundle_hash_b3 UK "BLAKE3 hash of bundle"
        text cpid "Control Plane ID"
        text signature_hex "Ed25519 signature (hex)"
        text public_key_hex "Ed25519 public key (hex)"
        text created_at "ISO timestamp"
    }
    
    %% Adapter Lifecycle Management
    adapters {
        text id PK "UUID primary key"
        text tenant_id FK "References tenants.id"
        text name "Adapter name"
        text tier "persistent|warm|ephemeral"
        text hash_b3 UK "BLAKE3 hash of adapter weights"
        integer rank "LoRA rank"
        real alpha "LoRA alpha scaling factor"
        text targets_json "JSON array of target layers"
        text acl_json "Access control list (JSON)"
        text adapter_id "External adapter ID for lookups"
        text languages_json "JSON array of supported languages"
        text framework "Framework identifier"
        integer active "0=inactive, 1=active"
        text created_at "ISO timestamp"
        text updated_at "ISO timestamp"
        text category "code|framework|codebase|ephemeral"
        text scope "global|tenant|repo|commit"
        text framework_id "Framework identifier"
        text framework_version "Framework version"
        text repo_id "Repository ID"
        text commit_sha "Git commit SHA"
        text intent "Adapter purpose/intent"
        text current_state "unloaded|cold|warm|hot|resident"
        integer pinned "0=not pinned, 1=pinned"
        integer memory_bytes "Memory usage in bytes"
        text last_activated "Last activation timestamp"
        integer activation_count "Number of activations"
    }
    
    ephemeral_adapters {
        text id PK "UUID primary key"
        text adapter_data "Serialized adapter data"
        text created_at "ISO timestamp"
    }
    
    adapter_provenance {
        text adapter_id PK "References adapters.id"
        text signer_key "Ed25519 public key of signer"
        text registered_by "Human registrar email"
        integer registered_uid "Unix UID of registrar"
        text registered_at "ISO timestamp"
        text bundle_b3 "BLAKE3 hash of adapter bundle"
    }
    
    adapter_categories {
        text name PK "Category name"
    }
    
    adapter_scopes {
        text name PK "Scope name"
    }
    
    adapter_states {
        text name PK "State name"
    }
    
    %% Configuration & Execution Plans
    manifests {
        text id PK "UUID primary key"
        text tenant_id FK "References tenants.id"
        text hash_b3 UK "BLAKE3 hash of manifest content"
        text body_json "Manifest configuration (JSON)"
        text created_at "ISO timestamp"
    }
    
    plans {
        text id PK "UUID primary key"
        text tenant_id FK "References tenants.id"
        text plan_id_b3 UK "BLAKE3 hash of compiled plan"
        text manifest_hash_b3 FK "References manifests.hash_b3"
        text kernel_hashes_json "JSON array of kernel hashes"
        text layout_hash_b3 "BLAKE3 hash of memory layout"
        text metadata_json "Plan metadata (JSON)"
        text created_at "ISO timestamp"
    }
    
    cp_pointers {
        text id PK "UUID primary key"
        text tenant_id FK "References tenants.id"
        text name "Pointer name (e.g., 'production', 'staging')"
        text plan_id FK "References plans.id"
        integer active "0=inactive, 1=active"
        text promoted_by FK "References users.id"
        text promoted_at "Promotion timestamp"
        text signing_public_key "Ed25519 public key for signing"
    }
    
    promotions {
        text id PK "UUID primary key"
        text cpid "Control Plane ID"
        text cp_pointer_id FK "References cp_pointers.id"
        text promoted_by FK "References users.id"
        text promoted_at "Promotion timestamp"
        text signature_b64 "Ed25519 signature (base64)"
        text signer_key_id "Key identifier"
        text quality_json "Quality metrics (ARR, ECS5, HLR, CR)"
        text before_cpid "Previous CPID"
        text created_at "ISO timestamp"
    }
    
    %% Policies & Compliance Management
    policies {
        text id PK "UUID primary key"
        text tenant_id FK "References tenants.id"
        text hash_b3 "BLAKE3 hash of policy content"
        text body_json "Policy configuration (JSON)"
        integer active "0=inactive, 1=active"
        text created_at "ISO timestamp"
    }
    
    code_policies {
        text id PK "UUID primary key"
        text tenant_id FK "References tenants.id"
        text evidence_config_json "Evidence requirements (JSON)"
        text auto_apply_config_json "Auto-apply settings (JSON)"
        text path_permissions_json "Path allowlist/denylist (JSON)"
        text secret_patterns_json "Secret detection patterns (JSON)"
        text patch_limits_json "Patch size/complexity limits (JSON)"
        integer active "0=inactive, 1=active"
        text created_at "ISO timestamp"
        text updated_at "ISO timestamp"
    }
    
    audits {
        text id PK "UUID primary key"
        text tenant_id FK "References tenants.id"
        text cpid "Control Plane ID"
        text suite_name "Test suite name"
        text bundle_id FK "References telemetry_bundles.id"
        real arr "Answer Relevance Rate"
        real ecs5 "Evidence Coverage Score @5"
        real hlr "Hallucination Rate"
        real cr "Conflict Rate"
        real nar "Numeric Accuracy Rate"
        real par "Provenance Attribution Rate"
        text verdict "pass|fail|warn"
        text details_json "Audit details (JSON)"
        text created_at "ISO timestamp"
        text before_cpid "Previous CPID"
        text after_cpid "New CPID"
        text status "Audit status"
    }
    
    %% Code Intelligence & Repository Management
    repositories {
        text id PK "UUID primary key"
        text repo_id UK "Unique repository identifier"
        text path "Repository filesystem path"
        text languages "Comma-separated language list"
        text default_branch "Default branch name"
        text status "registered|scanning|ready|error"
        text frameworks_json "Detected frameworks (JSON)"
        integer file_count "Total file count"
        integer symbol_count "Total symbol count"
        text created_at "ISO timestamp"
        text updated_at "ISO timestamp"
    }
    
    commits {
        text id PK "UUID primary key"
        text repo_id FK "References repositories.repo_id"
        text sha "Git commit SHA"
        text author "Commit author"
        text date "Commit date"
        text message "Commit message"
        text branch "Branch name"
        text changed_files_json "Changed files (JSON array)"
        text impacted_symbols_json "Impacted symbols (JSON array)"
        text test_results_json "Test results (JSON)"
        text ephemeral_adapter_id "Generated ephemeral adapter ID"
        text created_at "ISO timestamp"
    }
    
    patch_proposals {
        text id PK "UUID primary key"
        text repo_id "Repository ID"
        text commit_sha "Target commit SHA"
        text description "Patch description"
        text target_files_json "Target files (JSON array)"
        text patch_json "Patch content (JSON)"
        text validation_result_json "Validation results (JSON)"
        text status "Patch status"
        text created_at "ISO timestamp"
        text created_by "Creator user ID"
    }
    
    %% Jobs & Async Operations
    jobs {
        text id PK "UUID primary key"
        text kind "build_plan|audit|replay|node_command"
        text tenant_id FK "References tenants.id"
        text user_id FK "References users.id"
        text payload_json "Job parameters (JSON)"
        text status "queued|running|finished|failed|cancelled"
        text result_json "Job results (JSON)"
        text logs_path "Path to job logs"
        text created_at "ISO timestamp"
        text started_at "Start timestamp"
        text finished_at "Completion timestamp"
    }
    
    alerts {
        text id PK "UUID primary key"
        text severity "critical|high|medium|low"
        text kind "Alert type"
        text subject_id "Related entity ID"
        text message "Alert message"
        integer acknowledged "0=unacknowledged, 1=acknowledged"
        text created_at "ISO timestamp"
    }
    
    %% Telemetry & Monitoring
    telemetry_bundles {
        text id PK "UUID primary key"
        text tenant_id FK "References tenants.id"
        text cpid "Control Plane ID"
        text path UK "Bundle file path"
        text merkle_root_b3 "Merkle tree root hash"
        integer start_seq "Starting sequence number"
        integer end_seq "Ending sequence number"
        integer event_count "Number of events in bundle"
        text created_at "ISO timestamp"
    }
    
    system_metrics {
        integer id PK "Auto-increment primary key"
        integer timestamp "Unix timestamp"
        real cpu_usage "CPU usage percentage"
        real memory_usage "Memory usage percentage"
        integer disk_read_bytes "Disk read bytes"
        integer disk_write_bytes "Disk write bytes"
        integer network_rx_bytes "Network receive bytes"
        integer network_tx_bytes "Network transmit bytes"
        real gpu_utilization "GPU utilization percentage"
        integer gpu_memory_used "GPU memory used (bytes)"
        integer uptime_seconds "System uptime"
        integer process_count "Number of processes"
        real load_1min "1-minute load average"
        real load_5min "5-minute load average"
        real load_15min "15-minute load average"
        integer created_at "Unix timestamp"
    }
    
    system_health_checks {
        integer id PK "Auto-increment primary key"
        integer timestamp "Unix timestamp"
        text status "healthy|warning|critical"
        text check_name "Health check name"
        text check_status "healthy|warning|critical"
        text message "Check result message"
        real value "Current value"
        real threshold "Threshold value"
        integer created_at "Unix timestamp"
    }
    
    threshold_violations {
        integer id PK "Auto-increment primary key"
        integer timestamp "Unix timestamp"
        text metric_name "Metric name"
        real current_value "Current metric value"
        real threshold_value "Threshold value"
        text severity "warning|critical"
        integer resolved_at "Resolution timestamp"
        integer created_at "Unix timestamp"
    }
    
    metrics_aggregations {
        integer id PK "Auto-increment primary key"
        integer window_start "Window start timestamp"
        integer window_end "Window end timestamp"
        text window_type "hour|day|week"
        real avg_cpu_usage "Average CPU usage"
        real max_cpu_usage "Maximum CPU usage"
        real avg_memory_usage "Average memory usage"
        real max_memory_usage "Maximum memory usage"
        integer total_disk_read "Total disk read bytes"
        integer total_disk_write "Total disk write bytes"
        integer total_network_rx "Total network receive bytes"
        integer total_network_tx "Total network transmit bytes"
        integer sample_count "Number of samples"
        integer created_at "Unix timestamp"
    }
    
    system_metrics_config {
        integer id PK "Auto-increment primary key"
        text config_key UK "Configuration key"
        text config_value "Configuration value"
        integer updated_at "Last update timestamp"
    }
    
    %% Security & Incident Management
    incidents {
        text id PK "UUID primary key"
        text tenant_id FK "References tenants.id"
        text severity "critical|high|medium|low"
        text kind "Incident type"
        text description "Incident description"
        text worker_id FK "References workers.id"
        text bundle_id FK "References telemetry_bundles.id"
        integer resolved "0=unresolved, 1=resolved"
        text created_at "ISO timestamp"
        text resolved_at "Resolution timestamp"
    }
    
    enclave_operations {
        text id PK "UUID primary key"
        integer timestamp "Unix timestamp"
        text operation "sign|seal|unseal|get_public_key"
        text requester "Operation requester"
        text artifact_hash "Artifact hash"
        text result "success|error"
        text error_message "Error details"
        text created_at "ISO timestamp"
    }
    
    key_metadata {
        text key_label PK "Key label"
        integer created_at "Creation timestamp"
        text source "keychain|manual"
        text key_type "signing|encryption"
        text last_checked "Last check timestamp"
    }
    
    %% Replication & Distribution
    replication_journal {
        text session_id PK "UUID primary key"
        text from_node FK "References nodes.id"
        text to_node FK "References nodes.id"
        integer bytes "Total bytes transferred"
        integer artifacts "Number of artifacts replicated"
        text started_at "Session start timestamp"
        text completed_at "Session completion timestamp"
        text result "success|failed|partial"
        text error_message "Error details"
        text manifest_b3 "BLAKE3 hash of replication manifest"
        text signature "Ed25519 signature of manifest"
    }
    
    replication_artifacts {
        integer id PK "Auto-increment primary key"
        text session_id FK "References replication_journal.session_id"
        text adapter_id FK "References adapters.id"
        text artifact_hash "BLAKE3 hash of artifact"
        integer size_bytes "Artifact size in bytes"
        text transferred_at "Transfer completion timestamp"
        boolean verified "Hash verification status"
    }
    
    %% Comprehensive Relationships
    %% Tenant-scoped entities
    tenants ||--o{ adapters : "owns"
    tenants ||--o{ manifests : "owns"
    tenants ||--o{ plans : "owns"
    tenants ||--o{ cp_pointers : "owns"
    tenants ||--o{ policies : "owns"
    tenants ||--o{ code_policies : "owns"
    tenants ||--o{ workers : "owns"
    tenants ||--o{ telemetry_bundles : "owns"
    tenants ||--o{ audits : "owns"
    tenants ||--o{ incidents : "owns"
    tenants ||--o{ jobs : "owns"
    
    %% User actions
    users ||--o{ cp_pointers : "promotes"
    users ||--o{ promotions : "creates"
    users ||--o{ jobs : "creates"
    users ||--o{ artifacts : "imports"
    
    %% Infrastructure relationships
    nodes ||--o{ workers : "hosts"
    nodes ||--o{ replication_journal : "participates_as_source"
    nodes ||--o{ replication_journal : "participates_as_target"
    
    %% Worker relationships
    workers ||--o{ incidents : "generates"
    
    %% Plan and execution relationships
    plans ||--o{ cp_pointers : "references"
    plans ||--o{ workers : "executes"
    manifests ||--o{ plans : "compiles_to"
    
    %% Adapter relationships
    adapters ||--o{ adapter_provenance : "has"
    adapters ||--o{ replication_artifacts : "replicates"
    adapter_categories ||--o{ adapters : "categorizes"
    adapter_scopes ||--o{ adapters : "scopes"
    adapter_states ||--o{ adapters : "states"
    
    %% Telemetry and auditing relationships
    telemetry_bundles ||--o{ audits : "analyzes"
    telemetry_bundles ||--o{ incidents : "references"
    cp_pointers ||--o{ promotions : "promotes"
    
    %% Replication relationships
    replication_journal ||--o{ replication_artifacts : "contains"
    
    %% Code intelligence relationships
    repositories ||--o{ commits : "contains"
    repositories ||--o{ adapters : "generates"
    commits ||--o{ patch_proposals : "generates"
    commits ||--o{ ephemeral_adapters : "creates"
    
    %% Artifact relationships
    artifacts ||--o{ bundle_signatures : "signs"
    
    %% Security relationships
    enclave_operations ||--o{ key_metadata : "uses"
```

## Comprehensive Schema Analysis

### 1. **Identity & Access Control**
- **`users`**: Multi-role authentication with Argon2 password hashing
- **`tenants`**: ITAR-compliant multi-tenant isolation boundaries
- **`jwt_secrets`**: Cryptographic token rotation with BLAKE3 hashing

### 2. **Infrastructure Management**
- **`nodes`**: Distributed worker host management with health monitoring
- **`workers`**: Process lifecycle management with memory and K-sparse tracking
- **`system_metrics`**: Real-time performance monitoring (CPU, memory, disk, network, GPU)
- **`system_health_checks`**: Automated health status validation
- **`threshold_violations`**: Performance threshold breach detection
- **`metrics_aggregations`**: Pre-computed time-series summaries
- **`system_metrics_config`**: Configurable monitoring parameters

### 3. **Model & Artifact Management**
- **`models`**: Base model artifacts with cryptographic hashing
- **`artifacts`**: Content-addressed storage with Ed25519 signatures
- **`bundle_signatures`**: Cryptographic verification and provenance tracking

### 4. **Adapter Lifecycle Management**
- **`adapters`**: LoRA adapters with comprehensive state management
- **`ephemeral_adapters`**: Commit-aware temporary adapters
- **`adapter_provenance`**: Cryptographic signer and registrar tracking
- **`adapter_categories`**: Reference table for adapter types
- **`adapter_scopes`**: Reference table for adapter scopes
- **`adapter_states`**: Reference table for adapter lifecycle states

### 5. **Configuration & Execution Plans**
- **`manifests`**: Declarative configuration with content addressing
- **`plans`**: Compiled execution plans with kernel hash verification
- **`cp_pointers`**: Active plan pointers (production/staging) with signing
- **`promotions`**: Signed promotion records with quality metrics

### 6. **Policies & Compliance Management**
- **`policies`**: Policy packs per tenant with versioning
- **`code_policies`**: Code-specific policy configurations
- **`audits`**: Comprehensive hallucination metrics and compliance checks
- **`incidents`**: Security and policy violation tracking
- **`alerts`**: System-wide alerting and notification management

### 7. **Code Intelligence & Repository Management**
- **`repositories`**: Registered code repositories with language detection
- **`commits`**: Commit metadata and analysis with symbol tracking
- **`patch_proposals`**: AI-generated code patches with validation

### 8. **Async Operations & Job Management**
- **`jobs`**: Background task management (build, audit, replay, node commands)
- **`alerts`**: System alerts and notifications with acknowledgment tracking

### 9. **Telemetry & Monitoring**
- **`telemetry_bundles`**: NDJSON event bundles with Merkle tree verification
- **`system_metrics`**: Real-time system performance data
- **`system_health_checks`**: Automated health validation
- **`threshold_violations`**: Performance threshold breach detection
- **`metrics_aggregations`**: Time-series data aggregation
- **`system_metrics_config`**: Monitoring configuration management

### 10. **Security & Incident Management**
- **`incidents`**: Security and policy violation tracking
- **`enclave_operations`**: Secure enclave audit trail
- **`key_metadata`**: Cryptographic key lifecycle management
- **`jwt_secrets`**: Token rotation and security management

### 11. **Replication & Distribution**
- **`replication_journal`**: Cross-node replication sessions with manifest signing
- **`replication_artifacts`**: Individual artifact transfers with verification

## Advanced Design Principles

### **Multi-Tenant Architecture**
- **Isolation**: All tenant-scoped data references `tenants.id` with CASCADE deletes
- **ITAR Compliance**: `itar_flag` enables export control restrictions
- **Resource Quotas**: Tenant-level resource allocation and monitoring

### **Cryptographic Security**
- **Content Addressing**: BLAKE3 hashes for integrity and deduplication
- **Digital Signatures**: Ed25519 signatures for critical operations
- **Key Management**: Secure enclave integration with key rotation
- **Provenance Tracking**: Cryptographic signer and registrar information

### **Performance Optimization**
- **Strategic Indexing**: Optimized indexes for common query patterns
- **Aggregation Tables**: Pre-computed metrics for dashboard performance
- **Connection Pooling**: Efficient database connection management
- **Query Optimization**: Structured for high-throughput operations

### **Lifecycle Management**
- **State Tracking**: Comprehensive state management for adapters, workers, and plans
- **Audit Trails**: Complete operation history for compliance and debugging
- **Version Control**: Policy and configuration versioning
- **Rollback Support**: Safe rollback mechanisms for deployments

### **Code Intelligence Integration**
- **Repository Analysis**: Deep integration with Git repositories
- **Commit Tracking**: Symbol-level change analysis
- **Patch Generation**: AI-powered code modification proposals
- **Framework Detection**: Automatic framework and language identification

### **Observability & Monitoring**
- **Real-time Metrics**: Comprehensive system performance monitoring
- **Health Checks**: Automated system health validation
- **Alert Management**: Configurable alerting with acknowledgment
- **Telemetry Bundles**: Structured event logging with verification

### **Scalability & Distribution**
- **Horizontal Scaling**: Multi-node worker distribution
- **Replication**: Cross-node artifact synchronization
- **Load Balancing**: Intelligent request routing
- **Resource Management**: Memory and compute optimization

## Database Constraints & Validation

### **Referential Integrity**
- Foreign key constraints ensure data consistency
- CASCADE deletes maintain tenant isolation
- SET NULL for optional relationships

### **Data Validation**
- CHECK constraints for enumerated values
- UNIQUE constraints for business keys
- NOT NULL constraints for required fields

### **Performance Indexes**
- Primary key indexes on all tables
- Foreign key indexes for join performance
- Composite indexes for common query patterns
- Partial indexes for filtered queries

### **Security Constraints**
- Role-based access control (RBAC)
- Tenant isolation enforcement
- Cryptographic verification requirements
- Audit trail completeness

This comprehensive schema supports the full AdapterOS Control Plane operational depth features with enterprise-grade security, compliance, and performance characteristics.
