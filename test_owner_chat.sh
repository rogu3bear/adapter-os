#!/bin/bash
set -e

# Get auth token
echo "Getting auth token..."
TOKEN=$(curl -s -X POST http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@aos.local","password":"password"}' | jq -r .token)
echo "Token: ${TOKEN:0:30}..."

# Test owner chat
echo ""
echo "Testing owner chat..."
curl -s -X POST http://localhost:8080/api/v1/chat/owner-system \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"messages":[{"role":"user","content":"Hello"}]}' \
  --max-time 120 | jq .
