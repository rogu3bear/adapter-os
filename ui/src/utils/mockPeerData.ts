import { PeerInfo, PeerListResponse } from '@/api/federation-types';

/**
 * Mock peer data for development/testing when backend API is not fully implemented
 * NOTE: This is clearly marked as mock data for demonstration purposes
 */
export const MOCK_PEER_DATA: PeerListResponse = {
  peers: [
    {
      host_id: '550e8400-e29b-41d4-a716-446655440000',
      pubkey: 'ed25519:AAAA1111BBBB2222CCCC3333DDDD4444EEEE5555FFFF6666',
      hostname: 'node-01.example.com',
      registered_at: new Date(Date.now() - 86400000 * 7).toISOString(), // 7 days ago
      last_seen_at: new Date(Date.now() - 5000).toISOString(), // 5 seconds ago
      last_heartbeat_at: new Date(Date.now() - 3000).toISOString(), // 3 seconds ago
      active: true,
      health_status: 'healthy',
      discovery_status: 'registered',
      failed_heartbeats: 0,
      attestation_metadata: {
        platform: 'darwin',
        secure_enclave_available: true,
        tpm_available: false,
        attestation_timestamp: Date.now() / 1000,
        hardware_id: 'hw-001',
      },
    },
    {
      host_id: '550e8400-e29b-41d4-a716-446655440001',
      pubkey: 'ed25519:BBBB2222CCCC3333DDDD4444EEEE5555FFFF6666AAAA1111',
      hostname: 'node-02.example.com',
      registered_at: new Date(Date.now() - 86400000 * 5).toISOString(), // 5 days ago
      last_seen_at: new Date(Date.now() - 15000).toISOString(), // 15 seconds ago
      last_heartbeat_at: new Date(Date.now() - 12000).toISOString(), // 12 seconds ago
      active: true,
      health_status: 'degraded',
      discovery_status: 'registered',
      failed_heartbeats: 2,
    },
    {
      host_id: '550e8400-e29b-41d4-a716-446655440002',
      pubkey: 'ed25519:CCCC3333DDDD4444EEEE5555FFFF6666AAAA1111BBBB2222',
      hostname: 'node-03.example.com',
      registered_at: new Date(Date.now() - 86400000 * 3).toISOString(), // 3 days ago
      last_seen_at: new Date(Date.now() - 120000).toISOString(), // 2 minutes ago
      last_heartbeat_at: new Date(Date.now() - 90000).toISOString(), // 1.5 minutes ago
      active: false,
      health_status: 'unhealthy',
      discovery_status: 'failed',
      failed_heartbeats: 5,
    },
  ],
  total_count: 3,
  timestamp: new Date().toISOString(),
};

/**
 * Checks if the peers data appears to be mock data
 * This helps UI code gracefully handle mock vs real data
 */
export function isMockPeerData(data: PeerListResponse | null | undefined): boolean {
  if (!data || !data.peers || data.peers.length === 0) return false;

  // Check if any peer has the mock UUID prefix
  return data.peers.some(peer => peer.host_id.startsWith('550e8400-e29b-41d4-a716'));
}
