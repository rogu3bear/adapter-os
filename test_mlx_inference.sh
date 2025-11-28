#!/bin/bash
set -e

# Start server
export AOS_WORKER_SOCKET=/tmp/aos-worker.sock
./target/debug/adapteros-server --skip-pf-check --skip-drift-check &
SERVER_PID=$!
echo "Server started with PID: $SERVER_PID"

sleep 5

# Login and get token
echo "Getting auth token..."
TOKEN=$(curl -s -X POST http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@aos.local","password":"password"}' | jq -r .token)
echo "Token: ${TOKEN:0:30}..."

# Test owner chat
echo ""
echo "=== Testing owner chat (with MLX model) ==="
curl -s -X POST http://localhost:8080/api/v1/chat/owner-system \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Hello!"}]}' --max-time 120 | jq .
