#!/usr/bin/env bash
set -euo pipefail

assets=(
  gh-ghui-darwin-amd64
  gh-ghui-darwin-arm64
  gh-ghui-linux-amd64
  gh-ghui-linux-arm64
  gh-ghui-windows-amd64.exe
  gh-ghui-windows-arm64.exe
)

for asset in "${assets[@]}"; do
  if [[ ! -f "dist/$asset" ]]; then
    echo "error: missing release asset dist/$asset" >&2
    exit 1
  fi
done
