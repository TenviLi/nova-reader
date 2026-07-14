#!/bin/bash
# Setup Meilisearch index and Qdrant collection for Nova Reader
# Run this once after docker compose up, or after resetting services.

set -e

MEILI_URL="${MEILI_URL:-http://localhost:7700}"
MEILI_KEY="${MEILI_MASTER_KEY:-nova_search_key_2024}"
QDRANT_URL="${QDRANT_URL:-http://localhost:6333}"
EMBEDDING_API_KEY="${EMBEDDING_API_KEY:-E99GJYXDH1TP5MMJN5FHWE7PCUHT0HLXNPXWMT1W}"
EMBEDDING_DIM="${EMBEDDING_DIMENSIONS:-2560}"

echo "=== Nova Reader Search Infrastructure Setup ==="

# ─── Meilisearch ──────────────────────────────────────────────────────────────
echo ""
echo "→ Configuring Meilisearch..."

# Create chunks index
curl -s -X POST "$MEILI_URL/indexes" \
  -H "Authorization: Bearer $MEILI_KEY" \
  -H "Content-Type: application/json" \
  -d '{"uid": "chunks", "primaryKey": "id"}' > /dev/null 2>&1 || true

sleep 1

# Configure settings (searchable, filterable, embedder)
TASK=$(curl -s -X PATCH "$MEILI_URL/indexes/chunks/settings" \
  -H "Authorization: Bearer $MEILI_KEY" \
  -H "Content-Type: application/json" \
  -d "{
    \"searchableAttributes\": [\"content\", \"book_title\", \"chapter_title\"],
    \"filterableAttributes\": [\"book_id\", \"chapter_index\"],
    \"sortableAttributes\": [\"chapter_index\"],
    \"embedders\": {
      \"qwen3\": {
        \"source\": \"rest\",
        \"url\": \"https://ai.gitee.com/v1/embeddings\",
        \"apiKey\": \"$EMBEDDING_API_KEY\",
        \"dimensions\": $EMBEDDING_DIM,
        \"documentTemplate\": \"{{doc.book_title}} - {{doc.chapter_title}}: {{doc.content}}\",
        \"request\": {
          \"model\": \"Qwen3-Embedding-4B\",
          \"dimensions\": $EMBEDDING_DIM,
          \"input\": [\"{{text}}\"]
        },
        \"response\": {
          \"data\": [{\"embedding\": \"{{embedding}}\"}]
        }
      }
    }
  }" | python3 -c "import sys,json; print(json.load(sys.stdin).get('taskUid','?'))")

echo "  ✓ Meilisearch settings update enqueued (task $TASK)"
echo "    - Index: chunks"
echo "    - Embedder: Gitee AI / Qwen3-Embedding-4B (${EMBEDDING_DIM}d)"
echo "    - Hybrid search: enabled"

# ─── Qdrant ───────────────────────────────────────────────────────────────────
echo ""
echo "→ Configuring Qdrant..."

# Check if collection exists
EXISTS=$(curl -s "$QDRANT_URL/collections/nova_chunks" | python3 -c "import sys,json; print(json.load(sys.stdin).get('status','error'))" 2>/dev/null)

if [ "$EXISTS" = "ok" ]; then
  echo "  ✓ Collection nova_chunks already exists"
else
  curl -s -X PUT "$QDRANT_URL/collections/nova_chunks" \
    -H "Content-Type: application/json" \
    -d "{
      \"vectors\": {
        \"size\": $EMBEDDING_DIM,
        \"distance\": \"Cosine\"
      },
      \"optimizers_config\": {
        \"indexing_threshold\": 20000
      },
      \"on_disk_payload\": true
    }" > /dev/null
  echo "  ✓ Created collection nova_chunks (${EMBEDDING_DIM}d, Cosine)"
fi

# Create payload index for fast filtering
curl -s -X PUT "$QDRANT_URL/collections/nova_chunks/index?wait=true" \
  -H "Content-Type: application/json" \
  -d '{"field_name": "book_id", "field_schema": "keyword"}' > /dev/null 2>&1 || true

curl -s -X PUT "$QDRANT_URL/collections/nova_chunks/index?wait=true" \
  -H "Content-Type: application/json" \
  -d '{"field_name": "chapter_index", "field_schema": "integer"}' > /dev/null 2>&1 || true

curl -s -X PUT "$QDRANT_URL/collections/nova_chunks/index?wait=true" \
  -H "Content-Type: application/json" \
  -d '{"field_name": "book_source_content_hash", "field_schema": "keyword"}' > /dev/null 2>&1 || true

curl -s -X PUT "$QDRANT_URL/collections/nova_chunks/index?wait=true" \
  -H "Content-Type: application/json" \
  -d '{"field_name": "embedding_model", "field_schema": "keyword"}' > /dev/null 2>&1 || true

curl -s -X PUT "$QDRANT_URL/collections/nova_chunks/index?wait=true" \
  -H "Content-Type: application/json" \
  -d '{"field_name": "embedding_dimensions", "field_schema": "integer"}' > /dev/null 2>&1 || true

curl -s -X PUT "$QDRANT_URL/collections/nova_chunks/index?wait=true" \
  -H "Content-Type: application/json" \
  -d '{"field_name": "embedding_payload_version", "field_schema": "integer"}' > /dev/null 2>&1 || true

echo "  ✓ Payload indices created (identity, chapter, embedding freshness)"

# ─── Summary ─────────────────────────────────────────────────────────────────
echo ""
echo "=== Setup Complete ==="
echo ""
echo "Architecture:"
echo "  Query → Meilisearch (keyword + hybrid/semantic via REST embedder)"
echo "        → Qdrant (vector similarity, ${EMBEDDING_DIM}d)"
echo "        → RRF fusion"
echo "        → Reranker (vllm-mlx / Qwen3-Reranker-4B, port 8000)"
echo "        → Top N results"
echo ""
echo "To ingest a book:"
echo "  curl -X POST http://localhost:3000/api/ai/ingest-embeddings \\"
echo "    -H 'Content-Type: application/json' \\"
echo "    -H 'Cookie: nova_token=<jwt>' \\"
echo "    -d '{\"book_id\": \"<uuid>\"}'"
