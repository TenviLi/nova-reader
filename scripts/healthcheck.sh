#!/bin/sh
# Health check script for Docker — waits for all dependencies to be ready.
# Usage: ./scripts/healthcheck.sh [timeout_seconds]

set -e

TIMEOUT=${1:-30}
API_URL="${NOVA_API_URL:-http://localhost:3000}"

echo "⏳ Waiting for Nova Reader API (timeout: ${TIMEOUT}s)..."

elapsed=0
while [ $elapsed -lt $TIMEOUT ]; do
    if curl -sf "${API_URL}/api/health/ready" > /dev/null 2>&1; then
        echo "✅ Nova Reader API is healthy"
        exit 0
    fi
    sleep 1
    elapsed=$((elapsed + 1))
done

echo "❌ Health check timed out after ${TIMEOUT}s"
exit 1
