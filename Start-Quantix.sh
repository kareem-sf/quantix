#!/usr/bin/env sh
# Source-development launcher for macOS and Linux. See README for one-time setup.
set -eu
cd -- "$(dirname -- "$0")"
exec node scripts/start-desktop.mjs
