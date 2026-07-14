#!/bin/bash
# Debug login issue
echo "=== Checking DB users ==="
docker exec nova-postgres psql -U nova -d nova_reader -t -A -c "SELECT username || '|' || LEFT(password_hash, 30) FROM users LIMIT 3;"

echo "=== Checking refresh_tokens table ==="
docker exec nova-postgres psql -U nova -d nova_reader -t -A -c "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name = 'refresh_tokens');"

echo "=== Checking role column ==="
docker exec nova-postgres psql -U nova -d nova_reader -t -A -c "SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_name = 'users' AND column_name = 'role');"

echo "=== Testing login endpoint ==="
curl -s -X POST http://localhost:3000/api/auth/login -H "Content-Type: application/json" -d '{"username":"admin","password":"admin123"}' 2>&1

echo ""
echo "=== Done ==="
