#!/usr/bin/env bash
# check-uniffi-bindings-drift.sh — Verify the checked-in UniFFI Swift
# bindings for the 29er facade (`nmp-app-29er`) match a fresh
# `uniffi-bindgen` run.
#
# Usage:
#   scripts/check-uniffi-bindings-drift.sh          # fail on any diff
#   scripts/check-uniffi-bindings-drift.sh --regen  # regenerate in place
#
# Mirrors nostr-multi-platform's `ci/check-uniffi-bindings-drift.sh` (see
# that repo for the upstream pattern this is adapted from). 29er only emits
# Swift bindings (iOS is the only UniFFI consumer today; no Kotlin target).
#
# This script only manages the three files `uniffi-bindgen` itself emits:
#   ios/29er/29er/Bridge/Generated/nmp_app_29er.swift
#   ios/29er/29er/Bridge/Generated/nmp_app_29erFFI.h
#   ios/29er/29er/Bridge/Generated/nmp_app_29erFFI.modulemap
# The other `*.generated.swift` / `*_generated.swift` files in that
# directory are FlatBuffers wire-type output (a separate generator) and are
# intentionally left untouched.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REGEN=false

for arg in "$@"; do
    case "$arg" in
        --regen) REGEN=true ;;
        *) echo "Unknown argument: $arg" >&2; exit 1 ;;
    esac
done

PACKAGE="nmp-app-29er"
GENERATED_DIR="${REPO_ROOT}/ios/29er/29er/Bridge/Generated"
UNIFFI_FILES=(nmp_app_29er.swift nmp_app_29erFFI.h nmp_app_29erFFI.modulemap)

# ── Step 1: build the cdylib ─────────────────────────────────────────────────
echo "Building ${PACKAGE} (cdylib)..."
cargo build -p "$PACKAGE" 2>&1

LIB_NAME="lib${PACKAGE//-/_}"
DYLIB="${REPO_ROOT}/target/debug/${LIB_NAME}.dylib"
if [[ ! -f "$DYLIB" ]]; then
    # Linux dev boxes (no iOS toolchain) still need the bindgen path to work.
    DYLIB="${REPO_ROOT}/target/debug/${LIB_NAME}.so"
fi
if [[ ! -f "$DYLIB" ]]; then
    echo "ERROR: could not find ${LIB_NAME}.dylib or .so" >&2
    exit 1
fi

# ── Step 2: run uniffi-bindgen into a temp dir ───────────────────────────────
TMPDIR_SWIFT=$(mktemp -d)
trap 'rm -rf "$TMPDIR_SWIFT"' EXIT

echo "Generating Swift bindings..."
cargo run -p "$PACKAGE" --features bindgen --bin uniffi-bindgen -- \
    generate --library "$DYLIB" --language swift --out-dir "$TMPDIR_SWIFT"

# UniFFI's Swift generator emits trailing whitespace in a few spots.
# Normalize so the drift gate doesn't flag cosmetic-only differences.
find "$TMPDIR_SWIFT" -type f -print0 \
    | xargs -0 perl -0pi -e 's/[ \t]+$//mg; s/\n+\z/\n/'

# ── Step 3: diff (or regen) only the uniffi-owned files ──────────────────────
if [[ "$REGEN" == "true" ]]; then
    echo "Regenerating checked-in bindings..."
    mkdir -p "$GENERATED_DIR"
    for f in "${UNIFFI_FILES[@]}"; do
        cp "$TMPDIR_SWIFT/$f" "$GENERATED_DIR/$f"
    done
    echo "Done. Stage and commit ios/29er/29er/Bridge/Generated/ to update the drift baseline."
    exit 0
fi

echo "Diffing against checked-in bindings..."
DRIFT=0
for f in "${UNIFFI_FILES[@]}"; do
    if [[ ! -f "$GENERATED_DIR/$f" ]]; then
        echo "ERROR: missing checked-in file: $GENERATED_DIR/$f" >&2
        DRIFT=1
        continue
    fi
    if ! diff -u "$GENERATED_DIR/$f" "$TMPDIR_SWIFT/$f"; then
        DRIFT=1
    fi
done

if [[ "$DRIFT" -ne 0 ]]; then
    echo ""
    echo "ERROR: UniFFI bindings are out of date. Regenerate with:"
    echo "  scripts/check-uniffi-bindings-drift.sh --regen"
    exit 1
fi

echo "OK: UniFFI bindings are up to date."
