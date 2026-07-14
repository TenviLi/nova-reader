#!/usr/bin/env bash
# Nova Reader backup script
# Backs up PostgreSQL database and library files
set -euo pipefail

BACKUP_DIR="${BACKUP_DIR:-/data/backups}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_NAME="nova_backup_${TIMESTAMP}"

# Ensure backup directory exists
mkdir -p "${BACKUP_DIR}"

echo "📦 Starting Nova Reader backup: ${BACKUP_NAME}"

# 1. PostgreSQL dump
echo "  → Dumping database..."
pg_dump "${DATABASE_URL}" \
    --format=custom \
    --no-owner \
    --no-privileges \
    --file="${BACKUP_DIR}/${BACKUP_NAME}.pgdump"

echo "  ✓ Database backed up ($(du -h "${BACKUP_DIR}/${BACKUP_NAME}.pgdump" | cut -f1))"

# 2. Library files (covers, processed data)
LIBRARY_DIR="${LIBRARY_DIR:-/data/library}"
if [ -d "${LIBRARY_DIR}" ]; then
    echo "  → Archiving library files..."
    tar -czf "${BACKUP_DIR}/${BACKUP_NAME}_files.tar.gz" \
        -C "$(dirname "${LIBRARY_DIR}")" \
        "$(basename "${LIBRARY_DIR}")"
    echo "  ✓ Library files archived ($(du -h "${BACKUP_DIR}/${BACKUP_NAME}_files.tar.gz" | cut -f1))"
fi

# 3. Cleanup old backups (keep last 7 days)
echo "  → Cleaning up old backups..."
find "${BACKUP_DIR}" -name "nova_backup_*" -mtime +7 -delete 2>/dev/null || true

echo "✅ Backup complete: ${BACKUP_DIR}/${BACKUP_NAME}"
echo "   Files:"
ls -lh "${BACKUP_DIR}/${BACKUP_NAME}"* 2>/dev/null
