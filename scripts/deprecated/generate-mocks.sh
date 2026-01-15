#!/bin/bash
# adapterOS Mock Data Generator
# Generates realistic mock fixtures for UI and CLI development

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
MOCKS_DIR="$ROOT_DIR/mocks"

# Colors
CYAN='\033[0;36m'
GREEN='\033[0;32m'
NC='\033[0m'

echo -e "${CYAN}=== adapterOS Mock Generator ===${NC}"

# Create mocks directory structure
mkdir -p "$MOCKS_DIR"/{adapters,stacks,telemetry,inference,router,training,policies}

# Function to generate timestamp
now_iso() {
  date -u +"%Y-%m-%dT%H:%M:%SZ"
}

# Generate Adapter mocks
echo -e "${CYAN}Generating adapter mocks...${NC}"
cat > "$MOCKS_DIR/adapters/adapter-meta.json" << 'EOF'
{
  "adapters": [
    {
      "id": "adapter-001",
      "adapter_id": "code-review-specialist",
      "name": "tenant-a/engineering/code-review/r001",
      "hash_b3": "b3:abc123def456789012345678901234567890123456789012345678901234567890",
      "rank": 16,
      "tier": 2,
      "current_state": "hot",
      "languages": ["rust", "typescript", "python"],
      "framework": "general-code",
      "category": "code-intelligence",
      "memory_usage_mb": 256.5,
      "activation_percentage": 85.3,
      "created_at": "2025-01-15T10:00:00Z",
      "updated_at": "2025-01-16T14:30:00Z",
      "pinned": false,
      "expires_at": null
    },
    {
      "id": "adapter-002",
      "adapter_id": "rust-optimizer",
      "name": "tenant-a/engineering/rust-perf/r002",
      "hash_b3": "b3:def456abc789012345678901234567890123456789012345678901234567890123",
      "rank": 12,
      "tier": 1,
      "current_state": "warm",
      "languages": ["rust"],
      "framework": "framework-specific",
      "category": "optimization",
      "memory_usage_mb": 128.2,
      "activation_percentage": 42.1,
      "created_at": "2025-01-10T08:00:00Z",
      "updated_at": "2025-01-16T12:00:00Z",
      "pinned": true,
      "expires_at": null
    },
    {
      "id": "adapter-003",
      "adapter_id": "temp-experiment",
      "name": "tenant-b/research/experiment-a/r001",
      "hash_b3": "b3:789012345abc678901234567890123456789012345678901234567890123def456",
      "rank": 8,
      "tier": 0,
      "current_state": "cold",
      "languages": ["python"],
      "framework": null,
      "category": "research",
      "memory_usage_mb": 64.0,
      "activation_percentage": 5.2,
      "created_at": "2025-01-16T16:00:00Z",
      "updated_at": "2025-01-16T16:30:00Z",
      "pinned": false,
      "expires_at": "2025-01-23T23:59:59Z"
    }
  ]
}
EOF

# Generate Adapter Stack mocks
echo -e "${CYAN}Generating adapter stack mocks...${NC}"
cat > "$MOCKS_DIR/stacks/stack-versions.json" << EOF
{
  "stacks": [
    {
      "id": "stack-001",
      "name": "stack.production-code-review",
      "version": "v1.2.0",
      "adapter_ids": ["code-review-specialist", "rust-optimizer"],
      "workflow_type": "UpstreamDownstream",
      "effective_stack_hash": "b3:aabbccdd00112233445566778899aabbccddeeff00112233445566778899aabb",
      "created_at": "$(now_iso)",
      "is_active": true,
      "description": "Production stack for code review with Rust optimization"
    },
    {
      "id": "stack-002",
      "name": "stack.experimental-research",
      "version": "v0.1.0",
      "adapter_ids": ["temp-experiment"],
      "workflow_type": "Sequential",
      "effective_stack_hash": "b3:112233445566778899aabbccddeeff00112233445566778899aabbccddee00",
      "created_at": "$(now_iso)",
      "is_active": false,
      "description": "Experimental research stack"
    }
  ]
}
EOF

# Generate RouterDecisionEvent mocks
echo -e "${CYAN}Generating router decision mocks...${NC}"
cat > "$MOCKS_DIR/router/router-decisions.json" << 'EOF'
{
  "decisions": [
    {
      "decision_id": "dec-001",
      "step": 0,
      "input_token_id": 1234,
      "candidate_adapters": [
        {
          "adapter_idx": 0,
          "raw_score": 0.87,
          "gate_q15": 28508
        },
        {
          "adapter_idx": 1,
          "raw_score": 0.45,
          "gate_q15": 14745
        }
      ],
      "selected_adapters": [0],
      "entropy": 1.234,
      "tau": 0.1,
      "entropy_floor": 0.01,
      "stack_hash": "b3:aabbccdd00112233445566778899aabbccddeeff00112233445566778899aabb",
      "timestamp": "2025-01-16T14:35:22.123Z"
    }
  ]
}
EOF

# Generate Inference Trace mocks
echo -e "${CYAN}Generating inference trace mocks...${NC}"
cat > "$MOCKS_DIR/inference/inference-traces.json" << 'EOF'
{
  "traces": [
    {
      "trace_id": "trace-001",
      "request_id": "req-abc123",
      "adapters_used": ["code-review-specialist", "rust-optimizer"],
      "router_decisions": [
        {
          "step": 0,
          "input_token_id": 1234,
          "candidate_adapters": [
            {"adapter_idx": 0, "raw_score": 0.87, "gate_q15": 28508}
          ],
          "entropy": 1.234,
          "tau": 0.1,
          "entropy_floor": 0.01,
          "stack_hash": "b3:aabbccdd"
        }
      ],
      "latency_ms": 245,
      "tokens_generated": 512,
      "finish_reason": "length",
      "timestamp": "2025-01-16T14:35:00Z"
    }
  ]
}
EOF

# Generate Telemetry Bundle mocks
echo -e "${CYAN}Generating telemetry bundle mocks...${NC}"
cat > "$MOCKS_DIR/telemetry/telemetry-bundles.json" << EOF
{
  "bundles": [
    {
      "bundle_id": "bundle-$(date +%Y%m%d-%H%M%S)",
      "bundle_hash": "b3:$(openssl rand -hex 32)",
      "merkle_root": "b3:$(openssl rand -hex 32)",
      "event_count": 1247,
      "size_bytes": 524288,
      "created_at": "$(now_iso)",
      "signature": "$(openssl rand -hex 64)",
      "public_key": "$(openssl rand -hex 32)",
      "key_id": "$(openssl rand -hex 8)",
      "schema_version": 1,
      "tenant_id": "tenant-a",
      "cpid": "cpid-001",
      "sequence_no": 42
    }
  ],
  "events": [
    {
      "event_type": "adapter.load",
      "timestamp": "$(now_iso)",
      "tenant_id": "tenant-a",
      "adapter_id": "code-review-specialist",
      "metadata": {
        "state_transition": "cold_to_warm",
        "memory_allocated_mb": 256,
        "load_duration_ms": 1234
      }
    },
    {
      "event_type": "router.decision",
      "timestamp": "$(now_iso)",
      "tenant_id": "tenant-a",
      "metadata": {
        "selected_count": 2,
        "avg_gate_value": 0.72,
        "entropy": 1.234
      }
    }
  ]
}
EOF

# Generate Training Job mocks
echo -e "${CYAN}Generating training job mocks...${NC}"
cat > "$MOCKS_DIR/training/training-jobs.json" << EOF
{
  "jobs": [
    {
      "job_id": "train-001",
      "dataset_id": "dataset-abc123",
      "adapter_id": "new-adapter-001",
      "status": "running",
      "progress_pct": 67.5,
      "current_loss": 0.0234,
      "tokens_processed": 1500000,
      "tokens_per_sec": 12500,
      "rank": 16,
      "template": "general-code",
      "created_at": "2025-01-16T10:00:00Z",
      "started_at": "2025-01-16T10:05:00Z",
      "estimated_completion": "2025-01-16T16:30:00Z"
    },
    {
      "job_id": "train-002",
      "dataset_id": "dataset-def456",
      "adapter_id": "completed-adapter-002",
      "status": "completed",
      "progress_pct": 100.0,
      "current_loss": 0.0089,
      "tokens_processed": 3000000,
      "tokens_per_sec": 0,
      "rank": 12,
      "template": "framework-specific",
      "created_at": "2025-01-15T08:00:00Z",
      "started_at": "2025-01-15T08:05:00Z",
      "completed_at": "2025-01-15T14:22:00Z"
    }
  ]
}
EOF

# Generate Policy Preview mocks
echo -e "${CYAN}Generating policy preview mocks...${NC}"
cat > "$MOCKS_DIR/policies/policy-previews.json" << 'EOF'
{
  "policies": [
    {
      "policy_id": "policy-egress",
      "name": "Egress Policy",
      "version": "1.0.0",
      "enabled": true,
      "rules": [
        {
          "rule": "production_mode_requires_uds",
          "description": "Production mode must use Unix domain sockets only",
          "severity": "critical",
          "violated": false
        },
        {
          "rule": "no_tcp_listening",
          "description": "No TCP listening sockets allowed in production",
          "severity": "critical",
          "violated": false
        }
      ]
    },
    {
      "policy_id": "policy-determinism",
      "name": "Determinism Policy",
      "version": "1.0.0",
      "enabled": true,
      "rules": [
        {
          "rule": "seeded_randomness",
          "description": "All randomness must use HKDF-derived seeds",
          "severity": "critical",
          "violated": false
        },
        {
          "rule": "no_thread_rng",
          "description": "rand::thread_rng() is forbidden",
          "severity": "high",
          "violated": true,
          "violation_details": "Found 2 instances in crates/adapteros-lora-worker/src/sampling.rs"
        }
      ]
    }
  ]
}
EOF

# Generate index file
cat > "$MOCKS_DIR/index.json" << EOF
{
  "generated_at": "$(now_iso)",
  "mock_categories": [
    {
      "category": "adapters",
      "files": ["adapter-meta.json"],
      "count": 3
    },
    {
      "category": "stacks",
      "files": ["stack-versions.json"],
      "count": 2
    },
    {
      "category": "router",
      "files": ["router-decisions.json"],
      "count": 1
    },
    {
      "category": "inference",
      "files": ["inference-traces.json"],
      "count": 1
    },
    {
      "category": "telemetry",
      "files": ["telemetry-bundles.json"],
      "count": 1
    },
    {
      "category": "training",
      "files": ["training-jobs.json"],
      "count": 2
    },
    {
      "category": "policies",
      "files": ["policy-previews.json"],
      "count": 2
    }
  ]
}
EOF

echo -e "${GREEN}✓ Mock data generated in $MOCKS_DIR${NC}"
echo ""
echo "Generated files:"
ls -lh "$MOCKS_DIR"/*/*.json | awk '{print "  - " $9 " (" $5 ")"}'
echo ""
echo "To use mocks in UI development:"
echo "  import adapters from '../../../mocks/adapters/adapter-meta.json'"
echo ""
echo "To regenerate: $0"
