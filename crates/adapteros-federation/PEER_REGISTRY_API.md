# PeerRegistry API Reference

Complete API documentation for the `PeerRegistry` struct providing federated peer management.

## Constructor Methods

### `new(db: Arc<Db>) -> Self`

Create a new peer registry with default configuration.

**Configuration Defaults:**
- Consensus quorum size: 2
- Heartbeat timeout: 30 seconds
- Max failed heartbeats: 3

```rust
let registry = PeerRegistry::new(Arc::new(db));
```

### `with_config(db: Arc<Db>, consensus_quorum_size: usize, heartbeat_timeout_secs: u64, max_failed_heartbeats: u32) -> Self`

Create a peer registry with custom configuration.

**Parameters:**
- `consensus_quorum_size` - Minimum peers needed for quorum (typically 2+)
- `heartbeat_timeout_secs` - Timeout for health check responses
- `max_failed_heartbeats` - Number of failed checks before marking unhealthy

```rust
let registry = PeerRegistry::with_config(
    Arc::new(db),
    3,    // Need 3 peers for quorum in 5-peer cluster
    45,   // 45 second timeout for slow networks
    2,    // Mark unhealthy after 2 failures
);
```

## Host ID Management

### `set_local_host_id(host_id: String) -> impl Future<Output = ()>`

Set the local host ID for this peer registry instance.

**Usage:**
```rust
registry.set_local_host_id("my-hostname".to_string()).await;
```

### `get_local_host_id() -> impl Future<Output = String>`

Get the previously set local host ID.

**Returns:** Current local host ID as String

```rust
let my_id = registry.get_local_host_id().await;
println!("Local host: {}", my_id);
```

## Peer Registration

### `register_peer(host_id: String, pubkey: PublicKey, hostname: Option<String>, attestation_metadata: Option<AttestationMetadata>) -> Result<()>`

Register a new peer in the federation.

**Parameters:**
- `host_id` - Unique identifier for the peer
- `pubkey` - Ed25519 public key for the peer
- `hostname` - Optional DNS hostname
- `attestation_metadata` - Optional hardware attestation info

**Behavior:**
- Creates new peer if not exists
- Updates existing peer if already registered
- Initializes health status to Healthy
- Sets discovery status to Registered
- Updates in-memory cache

**Example:**
```rust
let keypair = Keypair::generate();
registry.register_peer(
    "peer-1".to_string(),
    keypair.public_key(),
    Some("peer1.example.com".to_string()),
    None,
).await?;
```

### `get_peer(host_id: &str) -> Result<Option<PeerInfo>>`

Retrieve information about a specific peer.

**Parameters:**
- `host_id` - Peer identifier

**Returns:** `Ok(Some(peer))` if found, `Ok(None)` if not found

**Behavior:**
- Checks in-memory cache first (O(1))
- Falls back to database if not cached
- Updates cache upon retrieval

**Example:**
```rust
if let Some(peer) = registry.get_peer("peer-1").await? {
    println!("Peer found: {:?}", peer.health_status);
} else {
    println!("Peer not found");
}
```

### `deactivate_peer(host_id: &str) -> Result<()>`

Mark a peer as inactive without deleting it.

**Parameters:**
- `host_id` - Peer to deactivate

**Behavior:**
- Sets `active = false` in database
- Removes from in-memory cache
- Preserves all historical data

**Example:**
```rust
registry.deactivate_peer("peer-1").await?;
```

## Peer Discovery

### `process_discovery_announcement(announcement: &DiscoveryAnnouncement) -> Result<Vec<String>>`

Process a peer announcement to discover new federation members.

**Parameters:**
- `announcement` - DiscoveryAnnouncement with sender ID, known peers, timestamp, epoch

**Returns:** Vector of newly discovered peer IDs

**Behavior:**
- Filters out already-known peers
- Returns only new peer identifiers
- Supports cascading discovery from multiple sources

**Example:**
```rust
let announcement = DiscoveryAnnouncement {
    sender_id: "peer-1".to_string(),
    known_peers: vec![
        "peer-2".to_string(),
        "peer-3".to_string(),
        "peer-4".to_string(),
    ],
    announcement_time: 1000,
    federation_epoch: 1,
};

let newly_discovered = registry.process_discovery_announcement(&announcement).await?;
println!("Discovered {} new peers", newly_discovered.len());
for peer_id in newly_discovered {
    println!("  - {}", peer_id);
}
```

### `get_all_peer_ids() -> Result<Vec<String>>`

Get list of all known peer IDs.

**Returns:** Vector of peer IDs

**Behavior:**
- Queries all peers from database
- Ordered by registration time (newest last)

**Example:**
```rust
let all_peers = registry.get_all_peer_ids().await?;
println!("Federation has {} members", all_peers.len());
```

## Peer Listing

### `list_active_peers() -> Result<Vec<PeerInfo>>`

Get all active peers in the federation.

**Returns:** Vector of PeerInfo structs for active peers

**Behavior:**
- Includes all health statuses except Deactivated
- Ordered by last seen time (descending)
- Includes full peer information

**Example:**
```rust
let active_peers = registry.list_active_peers().await?;
println!("Active peers: {}", active_peers.len());
for peer in active_peers {
    println!("  {} - health: {:?}", peer.host_id, peer.health_status);
}
```

### `list_peers_by_health(status: PeerHealthStatus) -> Result<Vec<PeerInfo>>`

Filter peers by health status.

**Parameters:**
- `status` - PeerHealthStatus enum value (Healthy, Degraded, Unhealthy, Isolated)

**Returns:** Vector of peers with specified health status

**Behavior:**
- Returns only active peers with matching status
- Ordered by last heartbeat (descending)
- Efficient indexed query

**Example:**
```rust
// Get all unhealthy peers
let unhealthy = registry.list_peers_by_health(PeerHealthStatus::Unhealthy).await?;
println!("Unhealthy peers: {}", unhealthy.len());

// Get all healthy peers
let healthy = registry.list_peers_by_health(PeerHealthStatus::Healthy).await?;
println!("Healthy peers: {}", healthy.len());

// Get isolated peers in partition
let isolated = registry.list_peers_by_health(PeerHealthStatus::Isolated).await?;
println!("Isolated peers: {}", isolated.len());
```

## Health Checking

### `record_health_check(host_id: &str, status: PeerHealthStatus, response_time_ms: u32, error_message: Option<String>) -> Result<()>`

Record a health check result for a peer.

**Parameters:**
- `host_id` - Peer identifier
- `status` - Health status (Healthy, Degraded, Unhealthy, Isolated)
- `response_time_ms` - Response time in milliseconds
- `error_message` - Optional error message for failures

**Behavior:**
- Inserts record into peer_health_checks table
- Updates peer's health_status and last_heartbeat_at
- Increments failed_heartbeats counter (except for Healthy)
- Auto-transitions to Unhealthy if threshold reached
- Resets failed counter on Healthy status

**Example:**
```rust
// Successful health check
registry.record_health_check(
    "peer-1",
    PeerHealthStatus::Healthy,
    25,  // 25ms response time
    None,
).await?;

// Failed health check
registry.record_health_check(
    "peer-2",
    PeerHealthStatus::Degraded,
    150,
    Some("High latency detected".to_string()),
).await?;
```

### `get_health_history(host_id: &str, limit: usize) -> Result<Vec<PeerHealthCheck>>`

Retrieve historical health checks for a peer.

**Parameters:**
- `host_id` - Peer identifier
- `limit` - Maximum number of records to return

**Returns:** Vector of PeerHealthCheck records

**Behavior:**
- Returns most recent checks first (chronological descending)
- Limited to specified count
- Includes response times and error messages

**Example:**
```rust
// Get last 10 health checks
let history = registry.get_health_history("peer-1", 10).await?;
for check in history {
    println!(
        "{} - {} ({}ms)",
        check.timestamp, check.status, check.response_time_ms
    );
    if let Some(error) = check.error_message {
        println!("  Error: {}", error);
    }
}
```

### `update_last_seen(host_id: &str) -> Result<()>`

Update the last seen timestamp for a peer.

**Parameters:**
- `host_id` - Peer identifier

**Usage:** Called when peer is contacted, even if not a full health check

```rust
registry.update_last_seen("peer-1").await?;
```

## Consensus Voting

### `initiate_consensus(peer_id: &str, action: String, participating_hosts: Vec<String>) -> Result<String>`

Initiate a consensus decision for peer state changes.

**Parameters:**
- `peer_id` - Peer subject of decision
- `action` - Action string (e.g., "evict_from_federation", "elect_new_leader")
- `participating_hosts` - Vector of host IDs eligible to vote

**Returns:** Decision ID (UUID) for tracking the vote

**Behavior:**
- Creates consensus decision record
- Calculates required votes = (participants / 2) + 1
- Initializes vote counter to 0
- Sets approved = false

**Example:**
```rust
let voters = vec!["host1".to_string(), "host2".to_string(), "host3".to_string()];
let decision_id = registry.initiate_consensus(
    "bad-peer",
    "evict_from_federation".to_string(),
    voters,
).await?;
println!("Consensus initiated: {}", decision_id);
```

### `record_consensus_vote(decision_id: &str, voting_host: &str, approved: bool) -> Result<bool>`

Record a vote for an ongoing consensus decision.

**Parameters:**
- `decision_id` - UUID of the decision
- `voting_host` - Host casting the vote
- `approved` - True for approval, false for rejection

**Returns:** true if quorum is reached, false otherwise

**Behavior:**
- Increments vote counter
- Checks if collected >= required votes
- If quorum reached:
  - Sets approved = true
  - Logs quorum achievement
  - Returns true
- Otherwise returns false

**Example:**
```rust
// Vote 1: Not yet quorum
let quorum1 = registry.record_consensus_vote(&decision_id, "host1", true).await?;
assert!(!quorum1); // 1 vote < 2 required

// Vote 2: Quorum reached!
let quorum2 = registry.record_consensus_vote(&decision_id, "host2", true).await?;
assert!(quorum2); // 2 votes >= 2 required
```

## Network Partition Handling

### `detect_partition(reachable_peers: HashSet<String>) -> Result<Option<PartitionEvent>>`

Detect network partitions by comparing reachable vs. all known peers.

**Parameters:**
- `reachable_peers` - Set of peers currently reachable

**Returns:** `Ok(Some(event))` if partition detected, `Ok(None)` if all connected

**Behavior:**
- Gets all known peers from database
- Calculates isolated peers = all - reachable
- If isolated peers empty: returns None
- Otherwise:
  - Creates PartitionEvent with partition ID, timestamps
  - Records partition in database
  - Marks isolated peers with Isolated status
  - Elects quorum leader from reachable peers
  - Logs partition event

**Example:**
```rust
// Simulate network monitoring
let reachable_peers: HashSet<String> = vec![
    "host1".to_string(),
    "host2".to_string(),
    "host3".to_string(),
].into_iter().collect();

if let Some(partition) = registry.detect_partition(reachable_peers).await? {
    eprintln!(
        "Partition detected! Isolated: {:?}, Reachable: {:?}, Leader: {:?}",
        partition.isolated_peers,
        partition.reachable_peers,
        partition.quorum_leader,
    );
} else {
    println!("All peers connected");
}
```

### `resolve_partition(partition_id: &str) -> Result<()>`

Resolve a previously detected network partition.

**Parameters:**
- `partition_id` - Partition ID from PartitionEvent

**Behavior:**
- Retrieves partition from database
- For each isolated peer:
  - Records health check with Healthy status
  - Resets failed_heartbeats counter
- Marks partition as resolved

**Example:**
```rust
if let Some(partition) = registry.detect_partition(reachable).await? {
    let partition_id = partition.partition_id.clone();

    // ... wait for recovery ...

    registry.resolve_partition(&partition_id).await?;
    println!("Partition resolved!");
}
```

## Cache Management

### `load_cache() -> Result<()>`

Load all active peers from database into in-memory cache.

**Usage:** Called on startup to pre-populate cache for fast lookups

**Behavior:**
- Fetches all active peers from database
- Clears existing cache
- Populates cache with current peers

**Example:**
```rust
// On application startup
registry.load_cache().await?;
println!("Cache loaded");

// Subsequent lookups are O(1)
if let Some(peer) = registry.get_peer("known-peer").await? {
    println!("Peer found in cache!");
}
```

## Configuration Access

### `consensus_quorum_size: usize`
Publicly accessible field for quorum size configuration.

### `heartbeat_timeout_secs: u64`
Publicly accessible field for heartbeat timeout in seconds.

### `max_failed_heartbeats: u32`
Publicly accessible field for failed heartbeat threshold.

## Type Reference

### PeerHealthStatus Enum
```rust
pub enum PeerHealthStatus {
    Healthy,      // Responsive, normal response time
    Degraded,     // Slow or occasional timeouts
    Unhealthy,    // Max failed heartbeats reached
    Isolated,     // In network partition
}
```

### DiscoveryStatus Enum
```rust
pub enum DiscoveryStatus {
    Registered,   // Peer is known and active
    Discovering,  // Discovery in progress
    Failed,       // Discovery failed
}
```

### PeerInfo Struct
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
    pub health_status: PeerHealthStatus,
    pub discovery_status: DiscoveryStatus,
    pub failed_heartbeats: u32,
}
```

### PeerHealthCheck Struct
```rust
pub struct PeerHealthCheck {
    pub host_id: String,
    pub timestamp: String,
    pub status: PeerHealthStatus,
    pub response_time_ms: u32,
    pub error_message: Option<String>,
}
```

### DiscoveryAnnouncement Struct
```rust
pub struct DiscoveryAnnouncement {
    pub sender_id: String,
    pub known_peers: Vec<String>,
    pub announcement_time: u64,
    pub federation_epoch: u64,
}
```

### PartitionEvent Struct
```rust
pub struct PartitionEvent {
    pub partition_id: String,
    pub detected_at: String,
    pub isolated_peers: Vec<String>,
    pub reachable_peers: Vec<String>,
    pub quorum_leader: Option<String>,
    pub resolved: bool,
}
```

## Error Handling

All methods return `Result<T>` where errors are:

- **AosError::Database** - Database operation failures
- **AosError::Validation** - Invalid input or state
- **AosError::Crypto** - Cryptographic key errors
- **AosError::Serialization** - JSON serialization failures

Example error handling:
```rust
match registry.record_health_check("peer", PeerHealthStatus::Healthy, 25, None).await {
    Ok(_) => println!("Health check recorded"),
    Err(e) => eprintln!("Failed to record: {}", e),
}
```

## Summary

The `PeerRegistry` API provides a complete federation management system with:
- 20+ methods for peer operations
- Type-safe status tracking
- Quorum-based consensus
- Network partition tolerance
- Comprehensive error handling
- Efficient caching

All methods are async and return `Result<T>` for proper error propagation.

---

## See Also

- [PEER_DISCOVERY.md](PEER_DISCOVERY.md) - Peer discovery protocol documentation
- [../../docs/ARCHITECTURE.md#architecture-components](../../docs/ARCHITECTURE.md#architecture-components) - Architectural patterns including multi-agent coordination
- [../../docs/DETERMINISM.md](../../docs/DETERMINISM.md) - Global tick ledger for federation sync
- [../../docs/DATABASE_REFERENCE.md](../../docs/DATABASE_REFERENCE.md) - Database schema for federation tables
- [../adapteros-db/README.md](../adapteros-db/README.md) - Database layer documentation
- [../../AGENTS.md](../../AGENTS.md) - Developer quick reference guide
