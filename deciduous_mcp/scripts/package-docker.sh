#!/bin/sh
set -eu

# One allowlist is shared by CI and the extracted-bundle acceptance test.
source_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
archive=${1:-deciduous-mcp-docker.tar.gz}
case "$archive" in
  /*) ;;
  *) archive="$PWD/$archive" ;;
esac
stage=$(mktemp -d "${TMPDIR:-/tmp}/deciduous-package.XXXXXX")
trap 'rm -rf "$stage"' EXIT HUP INT TERM
mkdir "$stage/deciduous-mcp-docker"
cp "$source_dir/../LICENSE" "$stage/deciduous-mcp-docker/LICENSE"

for file in Dockerfile Dockerfile.burrito .dockerignore mix.exs mix.lock \
  vendor config lib priv rel STRUCTURE.sql DEPLOY.md compose.yaml compose.local.yaml \
  .env.example; do
  cp -R "$source_dir/$file" "$stage/deciduous-mcp-docker/$file"
done
mkdir "$stage/deciduous-mcp-docker/scripts"
for script in setup.sh docker-entrypoint.sh dump_structure.sh; do
  cp "$source_dir/scripts/$script" "$stage/deciduous-mcp-docker/scripts/$script"
done
mkdir "$stage/deciduous-mcp-docker/certs"
cp "$source_dir/certs/README.md" "$stage/deciduous-mcp-docker/certs/README.md"

tar -czf "$archive" -C "$stage" deciduous-mcp-docker
printf '%s\n' "$archive"
