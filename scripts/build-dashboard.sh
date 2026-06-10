#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "Building React dashboard..."
cd "$ROOT/dashboard"
npm ci --silent
npm run build

DIST="$ROOT/dashboard/dist"
if [ ! -d "$DIST" ]; then
  echo "ERROR: dashboard/dist not found after build" >&2
  exit 1
fi

# Copy to perspective-core crate (where the HTTP server serves from)
CRATE_DIST="$ROOT/crates/perspective-core/dashboard_dist"
rm -rf "$CRATE_DIST"
cp -r "$DIST" "$CRATE_DIST"

echo "Dashboard built and copied to:"
echo "  $CRATE_DIST"
