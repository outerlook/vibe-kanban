#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

echo "Building frontend..."
cd frontend
pnpm install
pnpm run build
cd ..

echo "Building Tauri app..."
cargo tauri build

echo ""
echo "Build complete! Installers are in target/release/bundle/"
ls -la target/release/bundle/*/ 2>/dev/null || true
