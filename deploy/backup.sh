#!/usr/bin/env bash
# Consistent backup of every deciduous graph database.
#
# Uses SQLite's online .backup, so it is safe to run while the daemon writes.
# Snapshots each graph, gzips it to $BACKUP_DIR on the host, keeps RETAIN_DAYS,
# and (optionally) mirrors offsite with rclone.
#
# Cron (03:15 daily), as a user in the docker group:
#   15 3 * * *  /opt/deciduous/deploy/backup.sh >> /var/log/deciduous-backup.log 2>&1
set -euo pipefail

# <compose-project>_<volume-name>. Project defaults to the compose dir name
# ("deploy"); override with DECIDUOUS_VOLUME if you renamed it.
VOLUME="${DECIDUOUS_VOLUME:-deploy_deciduous-data}"
IMAGE="${DECIDUOUS_IMAGE:-deciduous:latest}"
DEST="${BACKUP_DIR:-/opt/deciduous/backups}"
RETAIN_DAYS="${RETAIN_DAYS:-14}"
RCLONE_REMOTE="${RCLONE_REMOTE:-}"   # e.g. b2:my-bucket/deciduous (optional)

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$DEST"

# Throwaway container shares the live data volume (rw: the online backup opens a
# second connection; SQLite handles the concurrency) and has sqlite3 from the
# runtime image. Runs as root so it can write the host-owned $DEST.
docker run --rm \
  --user 0:0 \
  -v "${VOLUME}:/data" \
  -v "${DEST}:/out" \
  --entrypoint /bin/sh \
  "$IMAGE" -c '
    set -eu
    stamp="'"$stamp"'"
    found=0
    for db in /data/graphs/*/deciduous.db; do
      [ -e "$db" ] || continue
      found=1
      id="$(basename "$(dirname "$db")")"
      sqlite3 "$db" ".backup /out/${id}-${stamp}.db"
      gzip -f "/out/${id}-${stamp}.db"
      echo "backed up ${id}"
    done
    [ "$found" = 1 ] || echo "warning: no graphs found under /data/graphs"
  '

# Prune old local snapshots.
find "$DEST" -name '*.db.gz' -mtime "+${RETAIN_DAYS}" -delete

# Optional offsite copy.
if [ -n "$RCLONE_REMOTE" ]; then
  rclone copy "$DEST" "$RCLONE_REMOTE" --include '*.db.gz'
fi

echo "backup complete: $stamp"
