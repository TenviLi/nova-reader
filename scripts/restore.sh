#!/usr/bin/env bash
# Nova Reader restore script
# Restores PostgreSQL database and library files from a backup
set -euo pipefail

BACKUP_DIR="${BACKUP_DIR:-/data/backups}"

if [ $# -lt 1 ]; then
    echo "Usage: $0 <backup_name>"
    echo ""
    echo "Available backups:"
    ls -1 "${BACKUP_DIR}"/nova_backup_*.pgdump 2>/dev/null | sed 's/.*nova_backup_/  /;s/.pgdump//'
    exit 1
fi

BACKUP_NAME="$1"
PGDUMP="${BACKUP_DIR}/nova_backup_${BACKUP_NAME}.pgdump"
FILES_ARCHIVE="${BACKUP_DIR}/nova_backup_${BACKUP_NAME}_files.tar.gz"

if [ ! -f "${PGDUMP}" ]; then
    echo "❌ Database backup not found: ${PGDUMP}"
    exit 1
fi

echo "🔄 Restoring Nova Reader from backup: ${BACKUP_NAME}"

# 1. Restore database
echo "  → Restoring database..."
pg_restore \
    --dbname="${DATABASE_URL}" \
    --clean \
    --if-exists \
    --no-owner \
    --no-privileges \
    "${PGDUMP}"
echo "  ✓ Database restored"

# 2. Restore files if archive exists
if [ -f "${FILES_ARCHIVE}" ]; then
    LIBRARY_DIR="${LIBRARY_DIR:-/data/library}"
    echo "  → Restoring library files..."
    tar -xzf "${FILES_ARCHIVE}" -C "$(dirname "${LIBRARY_DIR}")"
    echo "  ✓ Library files restored"
else
    echo "  ⚠ No file archive found, skipping"
fi

echo "✅ Restore complete from backup: ${BACKUP_NAME}"
