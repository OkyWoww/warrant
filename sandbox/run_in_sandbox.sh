#!/usr/bin/env bash
# ==============================================================================
# WARRANT Sandboxed Execution Runner
# Strictly enforces zero-network, read-only root, memory/pids bounds.
# ==============================================================================
set -euo pipefail

IMAGE_NAME="${WARRANT_DOCKER_IMAGE:-warrant-sandbox:latest}"
TARGET_DIR="${1:-}"

if [[ -z "${TARGET_DIR}" ]]; then
    echo "Usage: $0 <target_crate_dir> [cargo_miri_args...]" >&2
    exit 2
fi

if [[ ! -d "${TARGET_DIR}" ]]; then
    echo "Error: target directory '${TARGET_DIR}' does not exist" >&2
    exit 2
fi

shift || true

# Absolute path resolution
TARGET_ABS="$(cd "${TARGET_DIR}" && pwd)"

echo "[WARRANT-SANDBOX] Launching Miri verification in hardened container..."
echo "[WARRANT-SANDBOX] Target: ${TARGET_ABS}"
echo "[WARRANT-SANDBOX] Enforcing flags: --network none --read-only --tmpfs /tmp:rw,noexec,nosuid --pids-limit 64 --memory 2g"

# Run container with strict resource limits and network isolation
exec docker run --rm -i \
    --network none \
    --read-only \
    --tmpfs /tmp:rw,noexec,nosuid \
    --pids-limit 64 \
    --memory 2g \
    --user 1000:1000 \
    -v "${TARGET_ABS}:/tmp/warrant-work:ro" \
    -e RUST_BACKTRACE=1 \
    -e MIRI_AUTO_OPS=0 \
    -w /tmp/warrant-work \
    "${IMAGE_NAME}" \
    run "$@"
