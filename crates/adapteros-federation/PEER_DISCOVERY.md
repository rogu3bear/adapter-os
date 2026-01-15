# Peer Discovery & Consensus in adapterOS Federation

Comprehensive implementation of peer discovery, health checking, consensus voting, and network partition handling for the adapterOS federation system.

## Overview

The peer discovery system provides:

1. **Peer Discovery Protocol** - Bootstrap and cascading discovery from seed nodes
2. **Health Checking** - Continuous heartbeat monitoring with automatic status transitions
3. **Consensus Mechanisms** - Quorum-based voting for peer state changes
4. **Network Partition Handling** - Detection and recovery from network splits
5. **Multi-Peer Coordination** - Distributed decision-making across the federation

## Core Components

### 1. Peer Information & Status

```rust
pub struct PeerInfo {
    pub host_id: String,
    pub pubkey: PublicKey,
    pub hostname: Option<String>,
    pub registered_at: String,
    pub last_seen_at: Option<String>,
    pub last_heartbeat_at: Option<String>,
    pub attestation_metadata: Option<AttestationMetadata>,
    pub active: bool,
    pub health_status: PeerHealthStatus,      // NEW
    pub discovery_status: DiscoveryStatus,    // NEW
    pub failed_heartbeats: u32,               // NEW
}
```

#### Health Status States

- **Healthy** - Peer is responsive with normal response times (<20ms)
- **Degraded** - Peer is slow (>50ms) or occasional timeouts
- **Unhealthy** - Peer failed max_failed_heartbeats checks
- **Isolated** - Peer is in network partition

#### Discovery Status States

- **Registered** - Peer is known and active
- **Discovering** - Peer discovery in progress
- **Failed** - Discovery process failed

### 2. Peer Discovery

#### Bootstrap Protocol

```rust
pub async fn register_peer(
    &self,
    host_id: String,
    pubkey: PublicKey,
    hostname: Option<String>,
    attestation_metadata: Option<AttestationMetadata>,
) -> Result<()>
```

Register a peer with the federation. Peers can be:
- Seed nodes (known entry points)
- Discovered peers (from announcements)
- New dynamic nodes

#### Discovery Announcements

```rust
pub struct DiscoveryAnnouncement {
    pub sender_id: String,
    pub known_peers: Vec<String>,
    pub announcement_time: u64,
    pub federation_epoch: u64,
}

pub async fn process_discovery_announcement(
    &self,
    announcement: &DiscoveryAnnouncement,
) -> Result<Vec<String>>  // Returns newly discovered peers
```

**Usage Pattern:**
1. Peer announces known peers via gossip/broadcast
2. Registry filters out already-known peers
3. Returns list of newly discovered peer IDs
4. Automatically initiates connection to new peers

**Example: 3-Wave Discovery**

```
Wave 1: Seed node announces {peer1, peer2, peer3}
        → Discover: [peer1, peer2, peer3]

Wave 2: peer1 announces {peer2, peer3, peer4, peer5}
        → Discover: [peer4, peer5] (peer2, peer3 already known)

Wave 3: peer4 announces {peer5, peer6, peer7}
        → Discover: [peer6, peer7] (peer5 already known)
```

### 3. Health Checking

#### Recording Health Checks

```rust
pub async fn record_health_check(
    &self,
    host_id: &str,
    status: PeerHealthStatus,
    response_time_ms: u32,
    error_message: Option<String>,
) -> Result<()>
```

**Features:**
- Records each health check with timestamp and response time
- Tracks failed heartbeat counter
- Automatic state transitions:
  - Healthy → resets failed count
  - Degraded/Unhealthy → increments failed count
  - Reaches threshold → marks as Unhealthy
- Updates peer cache for fast lookups

**Response Time Thresholds:**
- < 20ms: Healthy
- 20-100ms: Degraded
- 100-200ms: Degraded (higher latency)
- 200ms+: Degraded (critical latency)
- Timeout/error: Degraded (recoverable) → Unhealthy (after N failures)

#### Health History

```rust
pub async fn get_health_history(
    &self,
    host_id: &str,
    limit: usize,
) -> Result<Vec<PeerHealthCheck>>
```

Retrieve historical health checks for analysis:
- Chronological ordering (newest first)
- Track trends and patterns
- Identify flapping peers
- Support alerting and diagnostics

### 4. Consensus Voting

#### Initiating Consensus

```rust
pub async fn initiate_consensus(
    &self,
    peer_id: &str,
    action: String,           // "evict_peer", "elect_new_leader", etc.
    participating_hosts: Vec<String>,
) -> Result<String>  // Returns decision ID
```

**Quorum Calculation:**
```
required_votes = (total_participants / 2) + 1
```

For 5 peers: (5/2) + 1 = 3 votes required
For 3 peers: (3/2) + 1 = 2 votes required

#### Recording Votes

```rust
pub async fn record_consensus_vote(
    &self,
    decision_id: &str,
    voting_host: &str,
    approved: bool,
) -> Result<bool>  // Returns true if quorum reached
```

**Usage Example: Peer Eviction Vote**

```rust
// Register 5 voting peers
let voters = vec!["host1", "host2", "host3", "host4", "host5"];

// Initiate: need 3 votes to evict peer
let decision_id = registry.initiate_consensus(
    "bad-peer",
    "evict_from_federation".to_string(),
    voters.iter().map(|v| v.to_string()).collect(),
).await?;

// Record votes (majority wins)
registry.record_consensus_vote(&decision_id, "host1", true).await?;  // 1/3
registry.record_consensus_vote(&decision_id, "host2", true).await?;  // 2/3
let quorum = registry.record_consensus_vote(&decision_id, "host3", true).await?;  // 3/3 ✓

assert!(quorum == true);  // Decision approved!
```

### 5. Network Partition Handling

#### Partition Detection

```rust
pub async fn detect_partition(
    &self,
    reachable_peers: HashSet<String>,
) -> Result<Option<PartitionEvent>>
```

**Algorithm:**
1. Get all known peers
2. Compare against reachable peers
3. Identify isolated peers (not reachable)
4. If isolated_peers.is_empty() → return None (no partition)
5. Otherwise → create PartitionEvent with:
   - Partition ID (UUID)
   - Isolated peer list
   - Reachable peer list
   - Quorum leader (for split-brain resolution)
   - Detection timestamp

**Example: 5-Peer Partition**

```rust
// Setup: 5 peers total
let all_peers = vec!["h1", "h2", "h3", "h4", "h5"];

// Simulate: Eastern region isolated
let reachable = vec!["h1", "h2", "h3"];  // Western region

// Detect partition
let partition = registry.detect_partition(reachable).await?;

// Result:
// partition.isolated_peers = ["h4", "h5"]
// partition.reachable_peers = ["h1", "h2", "h3"]
// partition.quorum_leader = Some("h1")  // Elected from reachable

// Isolated peers marked as PeerHealthStatus::Isolated
for peer_id in &["h4", "h5"] {
    let peer = registry.get_peer(peer_id).await?.unwrap();
    assert_eq!(peer.health_status, PeerHealthStatus::Isolated);
}
```

#### Partition Recovery

```rust
pub async fn resolve_partition(&self, partition_id: &str) -> Result<()>
```

**Recovery Process:**
1. Fetch partition record
2. Get isolated peers list
3. For each isolated peer:
   - Mark as Healthy
   - Reset failed heartbeat counter
4. Mark partition as resolved

**Multi-Wave Recovery:**

```rust
// Wave 1: Partial recovery (3 out of 5)
let wave1_reachable = vec!["h1", "h2", "h3"];
let partition1 = registry.detect_partition(wave1_reachable).await?;
// Still partitioned: h4, h5 isolated

// Wave 2: More recovery (5 out of 5)
let wave2_reachable = vec!["h1", "h2", "h3", "h4", "h5"];
let partition2 = registry.detect_partition(wave2_reachable).await?;
// No partition detected (all connected!)
assert_eq!(partition2, None);
```

### 6. Multi-Peer Queries

#### List Peers by Health Status

```rust
pub async fn list_peers_by_health(
    &self,
    status: PeerHealthStatus,
) -> Result<Vec<PeerInfo>>
```

Efficiently filter and query peers by health status:
- Get all healthy peers for distribution
- Get unhealthy peers for remediation
- Get isolated peers during partitions

#### Get All Peer IDs

```rust
pub async fn get_all_peer_ids(&self) -> Result<Vec<String>>
```

Fast retrieval of federation member list.

### 7. Configuration

```rust
pub fn with_config(
    db: Arc<Db>,
    consensus_quorum_size: usize,      // Min peers for quorum (default 2)
    heartbeat_timeout_secs: u64,       // Timeout for checks (default 30)
    max_failed_heartbeats: u32,        // Threshold for unhealthy (default 3)
) -> Self
```

**Default Configuration:**
- Quorum size: 2 (minimum for consensus)
- Heartbeat timeout: 30 seconds
- Max failed heartbeats: 3 (before marking unhealthy)

## Database Schema

The implementation uses 4 new database tables:

### `federation_peers` (Enhanced)
```sql
ALTER TABLE federation_peers ADD COLUMN last_heartbeat_at TEXT;
ALTER TABLE federation_peers ADD COLUMN health_status TEXT DEFAULT 'healthy';
ALTER TABLE federation_peers ADD COLUMN discovery_status TEXT DEFAULT 'registered';
ALTER TABLE federation_peers ADD COLUMN failed_heartbeats INTEGER DEFAULT 0;
```

### `peer_health_checks` (New)
```sql
CREATE TABLE peer_health_checks (
    id TEXT PRIMARY KEY,
    host_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    status TEXT NOT NULL,
    response_time_ms INTEGER,
    error_message TEXT,
    FOREIGN KEY (host_id) REFERENCES federation_peers(host_id)
);
```

### `consensus_decisions` (New)
```sql
CREATE TABLE consensus_decisions (
    id TEXT PRIMARY KEY,
    peer_id TEXT NOT NULL,
    action TEXT NOT NULL,
    participating_hosts_json TEXT,
    required_votes INTEGER,
    collected_votes INTEGER DEFAULT 0,
    approved BOOLEAN DEFAULT 0,
    timestamp TEXT NOT NULL
);
```

### `partition_events` (New)
```sql
CREATE TABLE partition_events (
    partition_id TEXT PRIMARY KEY,
    detected_at TEXT NOT NULL,
    isolated_peers_json TEXT,
    reachable_peers_json TEXT,
    quorum_leader TEXT,
    resolved BOOLEAN DEFAULT 0
);
```

## Integration Tests

Comprehensive integration tests are provided in `tests/federation_peer_discovery_integration.rs`:

1. **test_bootstrap_from_seed_nodes** - Basic bootstrap
2. **test_cascading_peer_discovery** - Multi-wave discovery
3. **test_multi_peer_health_checks** - Health across federation
4. **test_consensus_peer_eviction** - Quorum voting
5. **test_network_partition_with_quorum** - Partition detection
6. **test_multiple_partition_events** - Complex partitions
7. **test_failover_and_leader_election** - Leadership transitions
8. **test_peer_announcement_with_acknowledgment** - Discovery batch
9. **test_health_history_across_federation** - Historical analysis
10. **test_federation_recovery** - Catastrophic partition recovery
11. **test_peer_discovery_status_transitions** - Status changes

### Running Tests

```bash
# Run all federation peer discovery tests
cargo test --test federation_peer_discovery_integration

# Run specific test
cargo test --test federation_peer_discovery_integration test_consensus_peer_eviction

# Run with output
cargo test --test federation_peer_discovery_integration -- --nocapture
```

## Usage Examples

### Example 1: Basic Federation Bootstrap

```rust
let registry = PeerRegistry::new(Arc::new(db));

// Register seed nodes
registry.register_peer(
    "seed1".to_string(),
    keypair1.public_key(),
    Some("seed1.example.com".to_string()),
    None,
).await?;

registry.register_peer(
    "seed2".to_string(),
    keypair2.public_key(),
    Some("seed2.example.com".to_string()),
    None,
).await?;

// Discovery announcement
let announcement = DiscoveryAnnouncement {
    sender_id: "seed1".to_string(),
    known_peers: vec!["peer1".to_string(), "peer2".to_string()],
    announcement_time: 1000,
    federation_epoch: 1,
};

let discovered = registry.process_discovery_announcement(&announcement).await?;
println!("Discovered {} new peers", discovered.len());
```

### Example 2: Health Check Monitoring

```rust
// Periodic health check (simulated)
loop {
    for peer in registry.list_active_peers().await? {
        match check_peer_health(&peer).await {
            Ok(response_time) => {
                registry.record_health_check(
                    &peer.host_id,
                    PeerHealthStatus::Healthy,
                    response_time,
                    None,
                ).await?;
            }
            Err(e) => {
                registry.record_health_check(
                    &peer.host_id,
                    PeerHealthStatus::Degraded,
                    0,
                    Some(e.to_string()),
                ).await?;
            }
        }
    }
    tokio::time::sleep(Duration::from_secs(10)).await;
}
```

### Example 3: Consensus Voting

```rust
// Identify unhealthy peers
let unhealthy = registry.list_peers_by_health(PeerHealthStatus::Unhealthy).await?;

for peer in unhealthy {
    // Get all voting participants
    let voters: Vec<String> = registry.list_active_peers().await?
        .iter()
        .map(|p| p.host_id.clone())
        .collect();

    // Initiate vote to evict
    let decision_id = registry.initiate_consensus(
        &peer.host_id,
        "evict_from_federation".to_string(),
        voters,
    ).await?;

    // Peers vote asynchronously...
    // Decision is approved once quorum is reached
}
```

### Example 4: Network Partition Recovery

```rust
// Monitor network connectivity
let reachable_peers: HashSet<String> = monitor_connectivity().await?;

// Detect partition
if let Some(partition) = registry.detect_partition(reachable_peers).await? {
    eprintln!(
        "Network partition detected: {} isolated, {} reachable, leader: {:?}",
        partition.isolated_peers.len(),
        partition.reachable_peers.len(),
        partition.quorum_leader,
    );

    // Monitor for recovery
    loop {
        let current_reachable = monitor_connectivity().await?;
        if let None = registry.detect_partition(current_reachable).await? {
            // All peers connected again!
            registry.resolve_partition(&partition.partition_id).await?;
            println!("Network partition resolved!");
            break;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
```

## Architecture Patterns

### Pattern 1: Cascading Discovery

Peers announce their known peers, enabling exponential discovery without central registry:

```
Seed nodes → Wave 1 (3 peers)
          → Wave 2 (5 peers)
          → Wave 3 (8 peers)
          → etc.
```

### Pattern 2: Quorum-Based Decisions

Majority vote prevents split-brain scenarios:
- Need (N/2)+1 votes for consensus
- Quorum leader elected from reachable partition during splits
- Automatic recovery when partitions heal

### Pattern 3: Health-Driven Actions

Health status triggers automated remediation:
- Healthy (3 checks) → Maintain
- Degraded (threshold) → Monitor closely
- Unhealthy (max failures) → Initiate eviction vote
- Isolated (partition) → Resume upon connection

## Policy Compliance

Implements policies per AGENTS.md:

- **Isolation Ruleset (#8)**: Per-peer status tracking
- **Determinism Ruleset (#2)**: Consensus voting prevents split-brain
- **Evidence Ruleset (#6)**: Health checks create audit trail
- **Naming Ruleset (#5)**: Semantic host IDs for federation

## Performance Characteristics

- **Peer Registration**: O(1) - direct database insert + cache
- **Discovery**: O(n) - linear in peer list size
- **Health Check**: O(1) - direct update + cache
- **Consensus Vote**: O(1) - counter increment
- **Partition Detection**: O(n) - set difference operation
- **Health History**: O(log n) - indexed query with limit

## Future Enhancements

1. **Persistent Discovery State** - Cross-session peer lists
2. **Peer Reputation** - Track reliability scores
3. **Adaptive Timeouts** - Adjust thresholds based on network conditions
4. **Cross-Region Failover** - Geo-distributed consensus
5. **Peer Ranking** - Prefer faster peers for new connections
6. **Auto-Remediation** - Automatic peer eviction without voting
7. **Metrics Export** - Prometheus metrics for federation health
