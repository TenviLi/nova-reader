#!/usr/bin/env bash
# Seed the Nova Reader database with demo data for development
# Run this after `docker compose up` and the API server is running.

set -euo pipefail

API_BASE="${API_BASE:-http://localhost:3000/api}"

echo "🌱 Seeding Nova Reader with demo data..."

# Create a library
echo "Creating demo library..."
curl -s -X POST "$API_BASE/libraries" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "玄幻小说",
    "path": "/data/library/xuanhuan",
    "description": "玄幻仙侠类小说合集"
  }' | jq .

# Create collections
echo "Creating collections..."
curl -s -X POST "$API_BASE/collections" \
  -H "Content-Type: application/json" \
  -d '{"name": "完本精品", "description": "已完结的高质量小说"}' | jq .

curl -s -X POST "$API_BASE/collections" \
  -H "Content-Type: application/json" \
  -d '{"name": "经典仙侠", "description": "仙侠修真类经典之作"}' | jq .

curl -s -X POST "$API_BASE/collections" \
  -H "Content-Type: application/json" \
  -d '{"name": "都市异能", "description": "都市背景的异能小说"}' | jq .

# Create glossary entries
echo "Creating glossary entries..."
for entry in \
  '{"term":"灵气","definition":"修仙世界中的基本能量，可用于修炼和施展法术","category":"修炼体系"}' \
  '{"term":"金丹","definition":"修仙者突破到金丹期时在丹田中凝结的灵力结晶","category":"修炼体系"}' \
  '{"term":"渡劫","definition":"修仙者突破大境界时需要经历的天劫考验","category":"修炼体系"}' \
  '{"term":"神识","definition":"修仙者的精神力外放探测，类似雷达","category":"能力"}' \
  '{"term":"储物袋","definition":"利用空间法则制作的存储法器","category":"法器"}' \
; do
  curl -s -X POST "$API_BASE/glossary" \
    -H "Content-Type: application/json" \
    -d "$entry" > /dev/null
done

echo "✅ Demo data seeded successfully!"
echo ""
echo "You can now visit http://localhost:5173 to see the app with data."
