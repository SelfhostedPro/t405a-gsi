#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release -p t405a-gsi -p t405a-gui
mkdir -p dist/linux
cp target/release/t405a-gsi dist/linux/t405a-gsi
cp target/release/t405a-gsi-gui dist/linux/t405a-gsi-gui
echo "built:"
echo "  dist/linux/t405a-gsi-gui"
echo "  dist/linux/t405a-gsi"
