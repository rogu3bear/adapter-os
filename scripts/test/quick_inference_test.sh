#!/bin/bash
# Quick inference pipeline test - minimal curl commands

SERVER="${AOS_SERVER_URL:-http://localhost:8080}"

echo "Testing AdapterOS Inference Pipeline"
echo "======================================"
echo ""

# Test 1: Health
echo "1. Health Check:"
curl -s "$SERVER/healthz" | head -c 100
echo ""
echo ""

# Test 2: Login (if server is up)
echo "2. Attempting login:"
LOGIN=$(curl -s -X POST "$SERVER/v1/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin"}')
echo "$LOGIN" | head -c 200
echo ""
echo ""

# Extract token
TOKEN=$(echo "$LOGIN" | grep -o '"token":"[^"]*' | cut -d'"' -f4)

if [ -n "$TOKEN" ]; then
    echo "3. Testing inference with token:"
    curl -s -X POST "$SERVER/v1/infer" \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer $TOKEN" \
      -d '{"prompt":"Hello","max_tokens":10}' | head -c 300
    echo ""
else
    echo "3. Skipping inference (no token)"
fi

echo ""
echo "Done!"

