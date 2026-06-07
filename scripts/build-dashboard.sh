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

# Copy to Python package (for maturin wheel)
PY_DIST="$ROOT/crates/perspective-python/perspective_python/dashboard_dist"
rm -rf "$PY_DIST"
cp -r "$DIST" "$PY_DIST"

# Copy to plugin dir (for hermes plugin)
PLUGIN_DIST="$ROOT/plugins/memory/perspective/dashboard_dist"
rm -rf "$PLUGIN_DIST"
cp -r "$DIST" "$PLUGIN_DIST"

echo "Dashboard built and copied to:"
echo "  $PY_DIST"
echo "  $PLUGIN_DIST"
