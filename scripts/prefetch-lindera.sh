#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CACHE_DIR="${1:-$ROOT/.lindera-dict-cache}"
mkdir -p "$CACHE_DIR"
export LINDERA_BUILD_DICTIONARY_CACHE_DIR="$CACHE_DIR"

echo "Prefetching Lindera dictionaries into: $CACHE_DIR"
echo "The first run needs network access (ipadic ~13 MB + ko-dic ~50 MB); later builds reuse this directory."

cargo build --manifest-path "$ROOT/Cargo.toml" --locked -p memo-storage

echo "Cache ready. For offline builds export:"
echo "  export LINDERA_BUILD_DICTIONARY_CACHE_DIR=$CACHE_DIR"
