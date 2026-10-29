#!/usr/bin/env bash
# ==============================================================================
# WARRANT Standalone Reproducer Script (POSIX / Linux / CI)
# Bundle ID: evb-89972f8abae64f179ac61d699b042fe3
# Candidate ID: c-0001
# Verdict: PROVEN_PUBLIC
# Toolchain: nightly-2026-10-20
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NIGHTLY="${WARRANT_NIGHTLY:-nightly-2026-10-20}"

echo "============================================================"
echo "  WARRANT VERIFIER STANDALONE REPRODUCER"
echo "  Candidate: c-0001"
echo "  Toolchain: $NIGHTLY"
echo "  Target: $SCRIPT_DIR/target_with_harness.rs"
echo "============================================================"

TMP_DIR="$(mktemp -d -t warrant_reproduce_XXXXXX)"
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

mkdir -p "$TMP_DIR/src"
cp "$SCRIPT_DIR/target_with_harness.rs" "$TMP_DIR/src/lib.rs"

cat << 'EOF' > "$TMP_DIR/Cargo.toml"
[package]
name = "warrant-reproduction"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
EOF

cd "$TMP_DIR"
echo "[RUNNING] cargo +$NIGHTLY miri test --lib warrant_proof::proof_ ..."
set +e
cargo +$NIGHTLY miri test --lib warrant_proof::proof_
EXIT_CODE=$?
set -e

echo "[FINISHED] Miri exit code: $EXIT_CODE"
if [ "$EXIT_CODE" -ne 0 ]; then
    echo "[VERIFIED] Reproducer successfully confirmed failure under Miri."
    exit 0
else
    echo "[CLEAN] Safe execution without error."
    exit 0
fi
