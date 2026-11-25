#!/usr/bin/env bash
set -euo pipefail

# Base bootstrap for the embedded Debian arm64 rootfs.
# Idempotent: safe to run multiple times.

export DEBIAN_FRONTEND=noninteractive

if [ -f /etc/hermes-bootstrapped ]; then
    echo "Already bootstrapped; skipping."
    exit 0
fi

apt-get update
apt-get install -y --no-install-recommends \
    ca-certificates curl wget git \
    python3 python3-pip python3-venv \
    locales tzdata \
    build-essential pkg-config

# locale
if ! grep -q "en_US.UTF-8" /etc/locale.gen 2>/dev/null; then
    echo "en_US.UTF-8 UTF-8" >> /etc/locale.gen
fi
locale-gen || true

# DNS
cat > /etc/resolv.conf <<'EOF'
nameserver 8.8.8.8
nameserver 1.1.1.1
EOF

# certs
update-ca-certificates || true

# pip basics
python3 -m pip install --upgrade pip setuptools wheel || true

touch /etc/hermes-bootstrapped
echo "Bootstrap complete."
