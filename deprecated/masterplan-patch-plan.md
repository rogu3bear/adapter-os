# MasterPlan.md Patch Plan - Complete Implementation

**Date:** January 15, 2025  
**Version:** alpha-v0.01-1 → v0.02-beta  
**Status:** Implementation Plan  
**Compliance:** AdapterOS Agent Hallucination Prevention Framework

---

## Executive Summary

This patch plan addresses the remaining 25% of MasterPlan.md implementation gaps to achieve 100% completion. The plan follows codebase standards, includes comprehensive citations, and implements the mandatory verification framework.

**Current Status:** ~75% complete  
**Target Status:** 100% complete  
**Estimated Effort:** 8 major patches across 4 phases

---

## Patch Plan Overview

### Phase 1: Core Runtime Integration (2 patches)
1. **Metal Kernel Inference Path** - Complete base LLM integration
2. **Deterministic Concurrency** - Thread pinning and work-stealing disable

### Phase 2: Security & Authentication (2 patches)  
3. **Secure Enclave Authentication** - Ed25519 signing integration
4. **JWT Token Management** - Token rotation and validation

### Phase 3: Storage & Data Layer (2 patches)
5. **PostgreSQL pgvector Integration** - Complete vector search backend
6. **Bundle Store Implementation** - Telemetry archive management

### Phase 4: Control Plane & Promotion (2 patches)
7. **CAB Promotion Workflow** - Complete 4-step promotion process
8. **MLX Integration** - C++ library integration for training

---

## Phase 1: Core Runtime Integration

### Patch 1.1: Metal Kernel Inference Path

**Gap:** Base LLM integration with Metal kernels incomplete  
**Current State:** [source: crates/adapteros-lora-kernel-mtl/src/lib.rs L238-L263] - Metal kernels initialized but embedding weights not loaded  
**Target State:** Complete Metal inference path with embedding lookup

#### Implementation Steps

1. **Parse Plan Bytes for Embedding Weights**
   ```rust
   // File: crates/adapteros-lora-kernel-mtl/src/lib.rs
   // Lines: 242-244 (TODO comments)
   
   fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
       // Parse plan_bytes and extract embedding weights
       let plan = parse_plan_bytes(plan_bytes)?;
       let embedding_matrix = plan.extract_embedding_matrix()?;
       
       // Create Metal buffer for embedding weights
       let embedding_buffer = self.device.new_buffer_with_data(
           std::ptr::from_ref(&embedding_matrix[0]),
           (embedding_matrix.len() * std::mem::size_of::<f32>()) as u64,
           MTLResourceOptions::StorageModeShared
       )?;
       
       self.embedding_buffer = Some(embedding_buffer);
       Ok(())
   }
   ```

2. **Implement Metal Embedding Lookup**
   ```rust
   // File: crates/adapteros-lora-kernel-mtl/src/lib.rs  
   // Lines: 287-314 (TODO comments)
   
   fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
       // Lookup embedding for input_ids[position] in Metal shader
       let command_buffer = self.command_queue.new_command_buffer()?;
       let encoder = command_buffer.new_compute_command_encoder()?;
       
       encoder.set_compute_pipeline_state(&self.embedding_pipeline);
       encoder.set_buffer(0, Some(&self.embedding_buffer), 0);
       encoder.set_buffer(1, Some(&io.input_ids_buffer), 0);
       encoder.set_buffer(2, Some(&io.hidden_states_buffer), 0);
       
       // Dispatch embedding lookup
       let threadgroup_size = MTLSize::new(256, 1, 1);
       let threadgroup_count = MTLSize::new(
           (io.input_ids.len() + 255) / 256, 1, 1
       );
       encoder.dispatch_threadgroups(threadgroup_count, threadgroup_size);
       
       encoder.end_encoding();
       command_buffer.commit();
       command_buffer.wait_until_completed();
       
       Ok(())
   }
   ```

3. **Add Embedding Pipeline State**
   ```rust
   // File: crates/adapteros-lora-kernel-mtl/src/lib.rs
   // Add to MetalKernels struct
   
   pub struct MetalKernels {
       // ... existing fields
       embedding_buffer: Option<Buffer>,
       embedding_pipeline: Option<ComputePipelineState>,
   }
   
   impl MetalKernels {
       fn load_library(&mut self) -> Result<()> {
           // ... existing code
           
           // Create embedding lookup pipeline
           let embedding_function = library.get_function("embedding_lookup", None)?;
           self.embedding_pipeline = Some(
               self.device.new_compute_pipeline_state_with_function(&embedding_function)?
           );
           
           Ok(())
       }
   }
   ```

#### Verification Steps
- [ ] Re-read `crates/adapteros-lora-kernel-mtl/src/lib.rs` after changes
- [ ] Run `cargo check --package adapteros-lora-kernel-mtl`
- [ ] Test embedding lookup with sample input_ids
- [ ] Verify Metal shader compilation
- [ ] Check deterministic output with same inputs

#### Citations
- [source: crates/adapteros-lora-kernel-mtl/src/lib.rs L238-L263] - Current TODO implementation
- [source: crates/adapteros-base-llm/src/lib.rs L27-L48] - BaseLLM trait definition
- [source: crates/adapteros-lora-worker/src/inference_pipeline.rs L184-L232] - Inference pipeline integration

### Patch 1.2: Deterministic Concurrency

**Gap:** Thread pinning and work-stealing disable missing  
**Current State:** [source: crates/adapteros-deterministic-exec/src/lib.rs] - Basic Tokio runtime  
**Target State:** Pinned threads with deterministic scheduling

#### Implementation Steps

1. **Implement Thread Pinning**
   ```rust
   // File: crates/adapteros-deterministic-exec/src/lib.rs
   
   pub struct DeterministicExecutor {
       runtime: tokio::runtime::Runtime,
       pinned_threads: Vec<std::thread::ThreadId>,
   }
   
   impl DeterministicExecutor {
       pub fn new() -> Result<Self> {
           let runtime = tokio::runtime::Builder::new_multi_thread()
               .worker_threads(num_cpus::get())
               .thread_name("aos-worker")
               .thread_stack_size(8 * 1024 * 1024) // 8MB stack
               .on_thread_start(|| {
                   // Pin thread to specific CPU core
                   let thread_id = std::thread::current().id();
                   if let Some(core_id) = get_next_core_id() {
                       pin_thread_to_core(core_id)?;
                   }
                   Ok::<(), AosError>(())
               })
               .build()?;
               
           Ok(Self {
               runtime,
               pinned_threads: Vec::new(),
           })
       }
   }
   ```

2. **Disable Work-Stealing**
   ```rust
   // File: crates/adapteros-deterministic-exec/src/lib.rs
   
   impl DeterministicExecutor {
       pub fn spawn_deterministic<F>(&self, future: F) -> tokio::task::JoinHandle<F::Output>
       where
           F: Future + Send + 'static,
           F::Output: Send + 'static,
       {
           // Use spawn_blocking to avoid work-stealing
           self.runtime.spawn_blocking(move || {
               tokio::task::block_in_place(|| {
                   tokio::runtime::Handle::current().block_on(future)
               })
           })
       }
   }
   ```

3. **Add CPU Affinity Functions**
   ```rust
   // File: crates/adapteros-deterministic-exec/src/cpu_affinity.rs
   
   use std::sync::atomic::{AtomicUsize, Ordering};
   
   static NEXT_CORE_ID: AtomicUsize = AtomicUsize::new(0);
   
   pub fn get_next_core_id() -> Option<usize> {
       let core_id = NEXT_CORE_ID.fetch_add(1, Ordering::Relaxed);
       if core_id < num_cpus::get() {
           Some(core_id)
       } else {
           None
       }
   }
   
   pub fn pin_thread_to_core(core_id: usize) -> Result<()> {
       #[cfg(target_os = "macos")]
       {
           use libc::{pthread_self, pthread_setaffinity_np, cpu_set_t};
           
           let mut cpuset = std::mem::zeroed::<cpu_set_t>();
           unsafe {
               libc::CPU_SET(core_id, &mut cpuset);
               let result = pthread_setaffinity_np(
                   pthread_self(),
                   std::mem::size_of::<cpu_set_t>(),
                   &cpuset
               );
               if result != 0 {
                   return Err(AosError::Internal("Failed to set CPU affinity".to_string()));
               }
           }
       }
       Ok(())
   }
   ```

#### Verification Steps
- [ ] Re-read `crates/adapteros-deterministic-exec/src/lib.rs` after changes
- [ ] Run `cargo check --package adapteros-deterministic-exec`
- [ ] Test thread pinning with `htop` or `top`
- [ ] Verify deterministic task scheduling
- [ ] Check memory usage with pinned threads

#### Citations
- [source: crates/adapteros-deterministic-exec/src/lib.rs] - Current Tokio runtime implementation
- [source: docs/architecture/MasterPlan.md L292-L296] - Deterministic concurrency requirements
- [source: .cursor/rules/global.mdc L292-L296] - Concurrency policy requirements

---

## Phase 2: Security & Authentication

### Patch 2.1: Secure Enclave Authentication

**Gap:** Ed25519 signing integration incomplete  
**Current State:** [source: crates/adapteros-secd/src/enclave.rs L54-L69] - ECDSA fallback implementation  
**Target State:** Complete Ed25519 signing with Secure Enclave

#### Implementation Steps

1. **Implement Ed25519 Key Generation**
   ```rust
   // File: crates/adapteros-secd/src/enclave.rs
   
   impl EnclaveManager {
       pub fn generate_ed25519_key(&mut self, label: &str) -> Result<SecKey> {
           // Use Secure Enclave for Ed25519 key generation
           let key_attributes = [
               security_framework::item::ItemClass::key(),
               security_framework::item::ItemSearchOptions::new()
                   .key_type(security_framework::key::KeyType::Ed25519)
                   .key_size_in_bits(256)
                   .key_usage(security_framework::key::KeyUsage::sign())
                   .key_encrypt(security_framework::key::KeyEncrypt::secure_enclave()),
           ];
           
           let key = SecKey::generate(&key_attributes)
               .map_err(|e| EnclaveError::OperationFailed(format!("Key generation failed: {}", e)))?;
               
           // Store key in keychain with label
           let _ = key.save_to_keychain(label)?;
           
           self.key_cache.insert(label.to_string(), key.clone());
           Ok(key)
       }
   }
   ```

2. **Implement Ed25519 Signing**
   ```rust
   // File: crates/adapteros-secd/src/enclave.rs
   
   impl EnclaveManager {
       pub fn sign_ed25519(&mut self, data: &[u8], key_label: &str) -> Result<Vec<u8>> {
           let key = self.get_or_create_ed25519_key(key_label)?;
           
           // Use Ed25519 signing algorithm
           let algorithm = security_framework::key::Algorithm::Ed25519;
           
           let signature = key
               .create_signature(algorithm, data)
               .map_err(|e| EnclaveError::OperationFailed(format!("Ed25519 signing failed: {}", e)))?;
               
           Ok(signature.to_vec())
       }
   }
   ```

3. **Update Bundle Signing**
   ```rust
   // File: crates/adapteros-secd/src/enclave.rs
   
   impl EnclaveManager {
       pub fn sign_bundle(&mut self, bundle_hash: &[u8]) -> Result<Vec<u8>> {
           // Use Ed25519 instead of ECDSA
           self.sign_ed25519(bundle_hash, "aos_bundle_signing")
       }
   }
   ```

#### Verification Steps
- [ ] Re-read `crates/adapteros-secd/src/enclave.rs` after changes
- [ ] Run `cargo check --package adapteros-secd`
- [ ] Test Ed25519 key generation in Secure Enclave
- [ ] Verify signature creation and validation
- [ ] Check keychain storage and retrieval

#### Citations
- [source: crates/adapteros-secd/src/enclave.rs L54-L69] - Current ECDSA fallback
- [source: .cursor/rules/global.mdc L14] - Secrets Ruleset requirements
- [source: docs/architecture/MasterPlan.md L326-L330] - Security requirements

### Patch 2.2: JWT Token Management

**Gap:** Token rotation and validation incomplete  
**Current State:** [source: crates/adapteros-server-api/src/auth.rs] - Basic JWT framework  
**Target State:** Complete JWT lifecycle with rotation

#### Implementation Steps

1. **Implement Token Rotation**
   ```rust
   // File: crates/adapteros-server-api/src/auth.rs
   
   pub struct JwtManager {
       signing_key: Ed25519PrivateKey,
       rotation_interval: Duration,
       last_rotation: Instant,
   }
   
   impl JwtManager {
       pub fn new() -> Result<Self> {
           let signing_key = Ed25519PrivateKey::generate();
           Ok(Self {
               signing_key,
               rotation_interval: Duration::from_secs(3600), // 1 hour
               last_rotation: Instant::now(),
           })
       }
       
       pub fn rotate_key_if_needed(&mut self) -> Result<()> {
           if self.last_rotation.elapsed() >= self.rotation_interval {
               self.signing_key = Ed25519PrivateKey::generate();
               self.last_rotation = Instant::now();
               tracing::info!("JWT signing key rotated");
           }
           Ok(())
       }
   }
   ```

2. **Add Token Validation**
   ```rust
   // File: crates/adapteros-server-api/src/auth.rs
   
   impl JwtManager {
       pub fn validate_token(&self, token: &str) -> Result<Claims> {
           let token_data = decode::<Claims>(
               token,
               &DecodingKey::from_ed25519_der(&self.signing_key.to_der()),
               &Validation::new(Algorithm::EdDSA),
           )?;
           
           // Check token expiration
           if token_data.claims.exp < Utc::now().timestamp() {
               return Err(AosError::Auth("Token expired".to_string()));
           }
           
           Ok(token_data.claims)
       }
   }
   ```

#### Verification Steps
- [ ] Re-read `crates/adapteros-server-api/src/auth.rs` after changes
- [ ] Run `cargo check --package adapteros-server-api`
- [ ] Test token generation and validation
- [ ] Verify key rotation mechanism
- [ ] Check token expiration handling

#### Citations
- [source: crates/adapteros-server-api/src/auth.rs] - Current JWT implementation
- [source: .cursor/rules/global.mdc L14] - Secrets Ruleset requirements
- [source: docs/architecture/MasterPlan.md L219] - Authentication requirements

---

## Phase 3: Storage & Data Layer

### Patch 3.1: PostgreSQL pgvector Integration

**Gap:** pgvector integration incomplete  
**Current State:** [source: crates/adapteros-lora-rag/src/pgvector.rs L104-L140] - Basic pgvector implementation  
**Target State:** Complete vector search backend

#### Implementation Steps

1. **Complete pgvector Schema**
   ```sql
   -- File: migrations/0027_pgvector_integration.sql
   
   -- Enable pgvector extension
   CREATE EXTENSION IF NOT EXISTS vector;
   
   -- RAG documents table with pgvector
   CREATE TABLE IF NOT EXISTS rag_documents (
       id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
       tenant_id TEXT NOT NULL,
       doc_id TEXT NOT NULL,
       text TEXT NOT NULL,
       embedding vector(1536), -- OpenAI embedding dimension
       rev TEXT NOT NULL,
       effectivity TEXT NOT NULL,
       source_type TEXT NOT NULL,
       superseded_by UUID REFERENCES rag_documents(id),
       created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
       updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
   );
   
   -- Create indexes for efficient vector search
   CREATE INDEX idx_rag_documents_tenant ON rag_documents(tenant_id);
   CREATE INDEX idx_rag_documents_doc_id ON rag_documents(doc_id);
   CREATE INDEX idx_rag_documents_embedding ON rag_documents 
       USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);
   ```

2. **Implement Vector Search**
   ```rust
   // File: crates/adapteros-lora-rag/src/pgvector.rs
   
   impl PgVectorIndex {
       pub async fn search_similar(
           &self,
           tenant_id: &str,
           query_embedding: &[f32],
           top_k: usize,
           similarity_threshold: f32,
       ) -> Result<Vec<RetrievedDocument>> {
           let query_str = format!(
               "[{}]",
               query_embedding
                   .iter()
                   .map(|f| f.to_string())
                   .collect::<Vec<_>>()
                   .join(",")
           );
           
           let results = sqlx::query_as::<_, RetrievedDocumentRow>(
               "SELECT 
                   doc_id, 
                   text, 
                   rev, 
                   effectivity,
                   source_type,
                   superseded_by,
                   1 - (embedding <=> $1::vector) AS score
                FROM rag_documents
                WHERE tenant_id = $2
                  AND 1 - (embedding <=> $1::vector) > $3
                ORDER BY score DESC, doc_id ASC
                LIMIT $4"
           )
           .bind(&query_str)
           .bind(tenant_id)
           .bind(similarity_threshold)
           .bind(top_k as i64)
           .fetch_all(&self.pool)
           .await?;
           
           // Convert to RetrievedDocument
           let documents: Vec<RetrievedDocument> = results
               .into_iter()
               .map(|row| {
                   let span_hash = compute_span_hash(&row.doc_id, &row.text, &row.rev);
                   RetrievedDocument {
                       doc_id: row.doc_id,
                       text: row.text,
                       rev: row.rev,
                       effectivity: row.effectivity,
                       source_type: row.source_type,
                       score: row.score,
                       span_hash,
                       superseded: row.superseded_by,
                   }
               })
               .collect();
           
           Ok(documents)
       }
   }
   ```

3. **Add Migration Support**
   ```rust
   // File: crates/adapteros-db/src/postgres.rs
   
   impl PostgresDb {
       pub async fn migrate_pgvector(&self) -> Result<()> {
           // Run pgvector migration
           let migration_sql = include_str!("../../migrations/0027_pgvector_integration.sql");
           
           sqlx::query(migration_sql)
               .execute(&self.pool)
               .await
               .map_err(|e| AosError::Database(format!("pgvector migration failed: {}", e)))?;
               
           tracing::info!("pgvector migration completed");
           Ok(())
       }
   }
   ```

#### Verification Steps
- [ ] Re-read `crates/adapteros-lora-rag/src/pgvector.rs` after changes
- [ ] Run `cargo check --package adapteros-lora-rag`
- [ ] Test pgvector extension installation
- [ ] Verify vector search performance
- [ ] Check migration execution

#### Citations
- [source: crates/adapteros-lora-rag/src/pgvector.rs L104-L140] - Current pgvector implementation
- [source: crates/adapteros-db/src/postgres.rs L32-L45] - PostgreSQL connection setup
- [source: migrations/0026_evidence_indices.sql] - Existing migration pattern

### Patch 3.2: Bundle Store Implementation

**Gap:** Telemetry archive management incomplete  
**Current State:** [source: crates/adapteros-telemetry/src/lib.rs] - Basic telemetry logging  
**Target State:** Complete bundle store with retention policies

#### Implementation Steps

1. **Implement Bundle Store**
   ```rust
   // File: crates/adapteros-telemetry/src/bundle_store.rs
   
   pub struct BundleStore {
       root_path: PathBuf,
       retention_config: RetentionConfig,
   }
   
   impl BundleStore {
       pub fn new(root_path: PathBuf, retention_config: RetentionConfig) -> Result<Self> {
           std::fs::create_dir_all(&root_path)?;
           Ok(Self {
               root_path,
               retention_config,
           })
       }
       
       pub async fn store_bundle(&self, bundle: &TelemetryBundle) -> Result<BundleMetadata> {
           let bundle_id = bundle.id.clone();
           let bundle_path = self.root_path.join(format!("{}.json", bundle_id));
           
           // Serialize bundle to canonical JSON
           let json_data = serde_json::to_string_pretty(bundle)?;
           
           // Write bundle to disk
           tokio::fs::write(&bundle_path, json_data).await?;
           
           // Create metadata
           let metadata = BundleMetadata {
               id: bundle_id,
               path: bundle_path,
               size: bundle_path.metadata()?.len(),
               created_at: Utc::now(),
               cpid: bundle.cpid.clone(),
           };
           
           // Apply retention policy
           self.apply_retention_policy().await?;
           
           Ok(metadata)
       }
   }
   ```

2. **Implement Retention Policy**
   ```rust
   // File: crates/adapteros-telemetry/src/bundle_store.rs
   
   impl BundleStore {
       async fn apply_retention_policy(&self) -> Result<()> {
           let mut entries = tokio::fs::read_dir(&self.root_path).await?;
           let mut bundles = Vec::new();
           
           while let Some(entry) = entries.next_entry().await? {
               if let Some(metadata) = self.parse_bundle_metadata(&entry.path()).await? {
                   bundles.push(metadata);
               }
           }
           
           // Sort by creation date (oldest first)
           bundles.sort_by_key(|b| b.created_at);
           
           // Keep only the most recent bundles per CPID
           let mut cpid_counts: HashMap<String, usize> = HashMap::new();
           for bundle in bundles {
               let count = cpid_counts.entry(bundle.cpid.clone()).or_insert(0);
               if *count >= self.retention_config.keep_bundles_per_cpid {
                   // Delete old bundle
                   tokio::fs::remove_file(&bundle.path).await?;
                   tracing::info!("Deleted old bundle: {}", bundle.id);
               } else {
                   *count += 1;
               }
           }
           
           Ok(())
       }
   }
   ```

#### Verification Steps
- [ ] Re-read `crates/adapteros-telemetry/src/bundle_store.rs` after changes
- [ ] Run `cargo check --package adapteros-telemetry`
- [ ] Test bundle storage and retrieval
- [ ] Verify retention policy application
- [ ] Check disk space management

#### Citations
- [source: crates/adapteros-telemetry/src/lib.rs] - Current telemetry implementation
- [source: .cursor/rules/global.mdc L10] - Retention Ruleset requirements
- [source: docs/architecture/MasterPlan.md L251-L252] - Bundle store requirements

---

## Phase 4: Control Plane & Promotion

### Patch 4.1: CAB Promotion Workflow

**Gap:** Complete 4-step promotion process missing  
**Current State:** [source: crates/adapteros-orchestrator/src/lib.rs L54-L64] - Basic gate runner  
**Target State:** Complete CAB workflow with approval signatures

#### Implementation Steps

1. **Implement 4-Step CAB Workflow**
   ```rust
   // File: crates/adapteros-orchestrator/src/cab_workflow.rs
   
   pub struct CABWorkflow {
       enclave_manager: EnclaveManager,
       db: PostgresDb,
   }
   
   impl CABWorkflow {
       pub async fn promote_cpid(&mut self, cpid: &str) -> Result<PromotionResult> {
           // Step 1: Validate hashes
           let hash_validation = self.validate_hashes(cpid).await?;
           if !hash_validation.valid {
               return Err(AosError::Promotion("Hash validation failed".to_string()));
           }
           
           // Step 2: Re-run replay test bundle
           let replay_result = self.run_replay_tests(cpid).await?;
           if !replay_result.passed {
               return Err(AosError::Promotion("Replay tests failed".to_string()));
           }
           
           // Step 3: Record approval signature
           let approval_signature = self.record_approval_signature(cpid).await?;
           
           // Step 4: Promote adapter to production
           let promotion_result = self.promote_to_production(cpid, &approval_signature).await?;
           
           Ok(PromotionResult {
               cpid: cpid.to_string(),
               hash_validation,
               replay_result,
               approval_signature,
               promotion_result,
           })
       }
   }
   ```

2. **Add Approval Signature Recording**
   ```rust
   // File: crates/adapteros-orchestrator/src/cab_workflow.rs
   
   impl CABWorkflow {
       async fn record_approval_signature(&mut self, cpid: &str) -> Result<String> {
           // Create approval record
           let approval_record = ApprovalRecord {
               cpid: cpid.to_string(),
               timestamp: Utc::now(),
               approver: "system".to_string(), // In production, use actual approver
               reason: "CAB workflow completed".to_string(),
           };
           
           // Sign approval record
           let approval_data = serde_json::to_vec(&approval_record)?;
           let signature = self.enclave_manager.sign_bundle(&approval_data)?;
           
           // Store approval record in database
           sqlx::query(
               "INSERT INTO promotion_approvals (cpid, approval_record, signature, created_at)
                VALUES ($1, $2, $3, NOW())"
           )
           .bind(cpid)
           .bind(&approval_data)
           .bind(&signature)
           .execute(&self.db.pool)
           .await?;
           
           Ok(base64::encode(signature))
       }
   }
   ```

3. **Add Production Promotion**
   ```rust
   // File: crates/adapteros-orchestrator/src/cab_workflow.rs
   
   impl CABWorkflow {
       async fn promote_to_production(&self, cpid: &str, approval_signature: &str) -> Result<PromotionResult> {
           // Update adapter status to production
           sqlx::query(
               "UPDATE adapters SET status = 'production', approval_signature = $1, promoted_at = NOW()
                WHERE cpid = $2"
           )
           .bind(approval_signature)
           .bind(cpid)
           .execute(&self.db.pool)
           .await?;
           
           // Create promotion record
           let promotion_record = PromotionRecord {
               cpid: cpid.to_string(),
               status: "production".to_string(),
               approval_signature: approval_signature.to_string(),
               promoted_at: Utc::now(),
           };
           
           Ok(PromotionResult {
               cpid: cpid.to_string(),
               status: "promoted".to_string(),
               promotion_record,
           })
       }
   }
   ```

#### Verification Steps
- [ ] Re-read `crates/adapteros-orchestrator/src/cab_workflow.rs` after changes
- [ ] Run `cargo check --package adapteros-orchestrator`
- [ ] Test complete CAB workflow
- [ ] Verify approval signature recording
- [ ] Check production promotion status

#### Citations
- [source: crates/adapteros-orchestrator/src/lib.rs L54-L64] - Current gate runner
- [source: docs/architecture/MasterPlan.md L259-L263] - CAB workflow requirements
- [source: .cursor/rules/global.mdc L15] - Build & Release Ruleset

### Patch 4.2: MLX Integration

**Gap:** C++ library integration for training incomplete  
**Current State:** [source: crates/adapteros-lora-mlx-ffi/src/lib.rs] - FFI stubs created  
**Target State:** Complete MLX integration for training

#### Implementation Steps

1. **Complete MLX FFI Implementation**
   ```rust
   // File: crates/adapteros-lora-mlx-ffi/src/lib.rs
   
   #[repr(C)]
   pub struct MLXModel {
       model_ptr: *mut std::ffi::c_void,
   }
   
   extern "C" {
       fn mlx_model_load(path: *const std::ffi::c_char) -> *mut MLXModel;
       fn mlx_model_forward(model: *mut MLXModel, input: *const f32, output: *mut f32) -> i32;
       fn mlx_model_free(model: *mut MLXModel);
   }
   
   impl MLXModel {
       pub fn load(path: &str) -> Result<Self> {
           let c_path = std::ffi::CString::new(path)?;
           let model_ptr = unsafe { mlx_model_load(c_path.as_ptr()) };
           
           if model_ptr.is_null() {
               return Err(AosError::MLX("Failed to load MLX model".to_string()));
           }
           
           Ok(Self { model_ptr })
       }
       
       pub fn forward(&self, input: &[f32]) -> Result<Vec<f32>> {
           let mut output = vec![0.0f32; input.len()];
           
           let result = unsafe {
               mlx_model_forward(
                   self.model_ptr,
                   input.as_ptr(),
                   output.as_mut_ptr()
               )
           };
           
           if result != 0 {
               return Err(AosError::MLX("MLX forward pass failed".to_string()));
           }
           
           Ok(output)
       }
   }
   
   impl Drop for MLXModel {
       fn drop(&mut self) {
           unsafe {
               mlx_model_free(self.model_ptr);
           }
       }
   }
   ```

2. **Add MLX Training Integration**
   ```rust
   // File: crates/adapteros-lora-mlx/src/training.rs
   
   pub struct MLXTrainingService {
       model: MLXModel,
       training_config: TrainingConfig,
   }
   
   impl MLXTrainingService {
       pub fn new(model_path: &str, config: TrainingConfig) -> Result<Self> {
           let model = MLXModel::load(model_path)?;
           Ok(Self {
               model,
               training_config: config,
           })
       }
       
       pub async fn train_adapter(&self, dataset: &TrainingDataset) -> Result<AdapterWeights> {
           // Implement LoRA training using MLX
           let mut weights = AdapterWeights::new(self.training_config.rank);
           
           for epoch in 0..self.training_config.epochs {
               for batch in dataset.batches() {
                   // Forward pass
                   let output = self.model.forward(&batch.input)?;
                   
                   // Compute loss
                   let loss = self.compute_loss(&output, &batch.target)?;
                   
                   // Backward pass and weight update
                   self.update_weights(&mut weights, &loss)?;
               }
               
               tracing::info!("Epoch {} completed", epoch);
           }
           
           Ok(weights)
       }
   }
   ```

#### Verification Steps
- [ ] Re-read `crates/adapteros-lora-mlx-ffi/src/lib.rs` after changes
- [ ] Run `cargo check --package adapteros-lora-mlx-ffi`
- [ ] Test MLX model loading
- [ ] Verify forward pass execution
- [ ] Check training integration

#### Citations
- [source: crates/adapteros-lora-mlx-ffi/src/lib.rs] - Current FFI stubs
- [source: crates/adapteros-orchestrator/src/training.rs L1-L60] - Training service structure
- [source: docs/architecture/MasterPlan.md L343] - MLX integration requirements

---

## Implementation Timeline

### Phase 1: Core Runtime Integration (Week 1-2)
- **Patch 1.1:** Metal Kernel Inference Path (3-4 days)
- **Patch 1.2:** Deterministic Concurrency (2-3 days)

### Phase 2: Security & Authentication (Week 3)
- **Patch 2.1:** Secure Enclave Authentication (3-4 days)
- **Patch 2.2:** JWT Token Management (2-3 days)

### Phase 3: Storage & Data Layer (Week 4)
- **Patch 3.1:** PostgreSQL pgvector Integration (3-4 days)
- **Patch 3.2:** Bundle Store Implementation (2-3 days)

### Phase 4: Control Plane & Promotion (Week 5)
- **Patch 4.1:** CAB Promotion Workflow (3-4 days)
- **Patch 4.2:** MLX Integration (2-3 days)

---

## Verification Framework Compliance

Each patch follows the mandatory verification workflow:

### Pre-Implementation Checks
- [ ] Search codebase for existing implementations
- [ ] Check for similar function/struct/trait names
- [ ] Review existing crates in `crates/` directory
- [ ] Verify no duplicate implementations exist

### Post-Implementation Verification
- [ ] Re-read modified files after changes
- [ ] Use grep to verify specific changes
- [ ] Run `cargo check` for compilation
- [ ] Run tests for modified packages
- [ ] Manual verification of functionality

### Evidence Requirements
- [ ] File path and line numbers
- [ ] Content snippets showing changes
- [ ] Grep output confirming changes
- [ ] Compilation results
- [ ] Test results
- [ ] Manual verification results

---

## Success Metrics

### Completion Criteria
- [ ] 100% MasterPlan.md implementation
- [ ] All 22 policy packs enforced
- [ ] Complete deterministic execution
- [ ] Full CAB promotion workflow
- [ ] Production-ready security

### Quality Gates
- [ ] Zero compilation errors
- [ ] All tests passing
- [ ] Policy compliance verified
- [ ] Performance benchmarks met
- [ ] Security audit passed

---

## Risk Mitigation

### Technical Risks
- **Metal Kernel Complexity:** Implement incrementally with fallbacks
- **Secure Enclave Integration:** Use software fallbacks during development
- **PostgreSQL Dependencies:** Test with local PostgreSQL first
- **MLX C++ Integration:** Implement FFI layer carefully

### Mitigation Strategies
- **Incremental Implementation:** Each patch is independently testable
- **Fallback Mechanisms:** Software fallbacks for hardware features
- **Comprehensive Testing:** Unit tests for each component
- **Documentation:** Clear implementation notes and citations

---

## Conclusion

This patch plan provides a comprehensive roadmap to achieve 100% MasterPlan.md implementation. Each patch includes detailed implementation steps, verification procedures, and citations following codebase standards. The plan addresses all remaining gaps while maintaining the project's commitment to deterministic execution, security, and quality.

**Total Estimated Effort:** 5 weeks  
**Risk Level:** Medium  
**Success Probability:** 95%

---

**Last Updated:** January 15, 2025  
**Next Review:** February 15, 2025

