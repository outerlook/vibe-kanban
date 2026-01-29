#\!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

echo "Building frontend..."
cd frontend
pnpm install
pnpm run build
cd ..

echo "Building server binaries..."
cargo build --release -p server --bin mcp_task_server --bin server

echo "Building Tauri app..."
# Detect OS and build appropriate bundle
if [ -f /etc/fedora-release ] || [ -f /etc/redhat-release ]; then
    echo "Detected Fedora/RHEL - building RPM..."
    cargo tauri build --bundles rpm
elif [ -f /etc/debian_version ]; then
    echo "Detected Debian/Ubuntu - building deb..."
    cargo tauri build --bundles deb
else
    echo "Unknown OS - building default bundles..."
    cargo tauri build
fi

echo ""
echo "Build complete\! Installers are in target/release/bundle/"
ls -la target/release/bundle/*/ 2>/dev/null || true
