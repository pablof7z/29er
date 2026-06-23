#!/usr/bin/env zsh
# Build the NMP C-ABI static library for 29er and wrap it as an xcframework
# at ios/29er/Vendor/NmpCore.xcframework.
#
# 29er links one aggregate Rust archive (`-lnmp_app_29er`) that contains
# nmp-core, nmp-ffi, the NIP-46 signer broker, and the 29er per-app glue.
# Mirrors `vendor/nmp/justfile`'s `rust-ios-sim` / `rust-ios-device` recipes
# but produces an xcframework instead of loose archives in `target/`.
#
# Run from the repo root:
#   scripts/build-nmp-xcframework.sh            # debug, sim only (fast)
#   scripts/build-nmp-xcframework.sh --release   # release, sim + device
#   scripts/build-nmp-xcframework.sh --device    # add device slice
#
# Idempotent: re-running rebuilds the archives and recreates the xcframework.

set -euo pipefail

REPO_ROOT="${0:A:h:h}"
PACKAGE="nmp-app-29er"
OUT_DIR="$REPO_ROOT/ios/29er/Vendor"
XCFRAMEWORK="$OUT_DIR/NmpCore.xcframework"
SIM_TARGET="aarch64-apple-ios-sim"
DEVICE_TARGET="aarch64-apple-ios"
PROFILE="debug"
BUILD_DEVICE=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --release)
            PROFILE="release"
            ;;
        --device)
            BUILD_DEVICE=1
            ;;
        *)
            echo "Unknown option: $1" >&2
            echo "Usage: $0 [--release] [--device]" >&2
            exit 2
            ;;
    esac
    shift
done

# Release builds for the device target require IPHONEOS_DEPLOYMENT_TARGET to
# avoid the ___chkstk_darwin linker error introduced by Xcode 26 (matches
# `vendor/nmp/justfile` `rust-ios-device`).
if [[ "$PROFILE" == "release" && "$BUILD_DEVICE" == "1" ]]; then
    export IPHONEOS_DEPLOYMENT_TARGET=17.0
fi

echo "==> Building $PACKAGE ($PROFILE) for $SIM_TARGET"
cd "$REPO_ROOT"
if [[ "$PROFILE" == "release" ]]; then
    cargo build --release -p "$PACKAGE" --target "$SIM_TARGET"
else
    cargo build -p "$PACKAGE" --target "$SIM_TARGET"
fi

SIM_LIB="$REPO_ROOT/target/$SIM_TARGET/$PROFILE/lib${PACKAGE//-/_}.a"
if [[ ! -f "$SIM_LIB" ]]; then
    echo "error: expected $SIM_LIB to exist after cargo build" >&2
    exit 1
fi

# Build the sim library wrapper (a one-slice framework). We always have a sim
# slice; the device slice is optional (--device).
SIM_FRAMEWORK="$OUT_DIR/NmpCore-sim.xcframework"
rm -rf "$SIM_FRAMEWORK"
mkdir -p "$OUT_DIR"

# xcodebuild -create-xcframework expects a .a wrapped in a "library" with an
# Info.plist, OR a .framework directory. The simplest path is to wrap each
# slice as a .framework and let -create-xcframework assemble the multi-slice
# bundle. We build the framework layout manually so we don't need a separate
# xcodebuild project.
make_framework() {
    local name="$1" lib="$2" out="$3" arch_dir="$4"
    rm -rf "$out"
    mkdir -p "$out/$name.framework/Headers"
    mkdir -p "$out/$name.framework/Modules"
    cp "$lib" "$out/$name.framework/$name"
    # Minimal modulemap so Clang treats it as a framework with a linkage unit.
    cat > "$out/$name.framework/Modules/module.modulemap" <<EOF
framework module $name {
    umbrella header "$name.h"
    export *
    module * { export * }
}
EOF
    # Bridging header (umbrella). The Swift side imports NmpCore via the
    # bridging header at Bridge/NmpCore.h; we symlink it here so the module
    # resolves. The header lives in the app source tree, so we copy it.
    if [[ -f "$REPO_ROOT/ios/29er/29er/Bridge/NmpCore.h" ]]; then
        cp "$REPO_ROOT/ios/29er/29er/Bridge/NmpCore.h" "$out/$name.framework/Headers/$name.h"
    else
        # Fall back to a stub so the framework is valid before the header is
        # written (xcodegen reference path).
        echo "#ifndef NMP_CORE_H" > "$out/$name.framework/Headers/$name.h"
        echo "#define NMP_CORE_H" >> "$out/$name.framework/Headers/$name.h"
        echo "#include <stdint.h>" >> "$out/$name.framework/Headers/$name.h"
        echo "#endif" >> "$out/$name.framework/Headers/$name.h"
    fi
    # Info.plist: mark as a static-archive framework (LBPLIB). xcodebuild's
    # -create-xcframework reads this to classify the slice.
    cat > "$out/$name.framework/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>$name</string>
    <key>CFBundleIdentifier</key>
    <string>io.f7z.app29er.$name</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>$name</string>
    <key>CFBundlePackageType</key>
    <string>FMWK</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>DTPlatformName</key>
    <string>$arch_dir</string>
</dict>
</plist>
EOF
}

rm -rf "$XCFRAMEWORK"
if [[ "$BUILD_DEVICE" == "1" ]]; then
    echo "==> Building $PACKAGE ($PROFILE) for $DEVICE_TARGET"
    if [[ "$PROFILE" == "release" ]]; then
        cargo build --release -p "$PACKAGE" --target "$DEVICE_TARGET"
    else
        cargo build -p "$PACKAGE" --target "$DEVICE_TARGET"
    fi
    DEVICE_LIB="$REPO_ROOT/target/$DEVICE_TARGET/$PROFILE/lib${PACKAGE//-/_}.a"
    if [[ ! -f "$DEVICE_LIB" ]]; then
        echo "error: expected $DEVICE_LIB to exist after cargo build" >&2
        exit 1
    fi
    SIM_FW="$OUT_DIR/NmpCore-iossim.framework"
    DEVICE_FW="$OUT_DIR/NmpCore-iosdevice.framework"
    make_framework "NmpCore" "$SIM_LIB" "$OUT_DIR" "iphonesimulator"
    mv "$OUT_DIR/NmpCore.framework" "$SIM_FW"
    make_framework "NmpCore" "$DEVICE_LIB" "$OUT_DIR" "iphoneos"
    mv "$OUT_DIR/NmpCore.framework" "$DEVICE_FW"
    echo "==> Assembling $XCFRAMEWORK (sim + device)"
    xcodebuild -create-xcframework \
        -library "$SIM_FW/NmpCore" \
        -headers "$SIM_FW/Headers" \
        -library "$DEVICE_FW/NmpCore" \
        -headers "$DEVICE_FW/Headers" \
        -output "$XCFRAMEWORK" 2>&1 || {
            # Older xcodebuild rejects -library pointing at a framework binary;
            # fall back to the loose-archive form.
            xcodebuild -create-xcframework \
                -library "$SIM_LIB" \
                -library "$DEVICE_LIB" \
                -output "$XCFRAMEWORK"
        }
    rm -rf "$SIM_FW" "$DEVICE_FW"
else
    echo "==> Wrapping $XCFRAMEWORK (sim only)"
    SIM_FW="$OUT_DIR/NmpCore-iossim.framework"
    make_framework "NmpCore" "$SIM_LIB" "$OUT_DIR" "iphonesimulator"
    mv "$OUT_DIR/NmpCore.framework" "$SIM_FW"
    xcodebuild -create-xcframework \
        -library "$SIM_FW/NmpCore" \
        -headers "$SIM_FW/Headers" \
        -output "$XCFRAMEWORK" 2>&1 || {
            xcodebuild -create-xcframework \
                -library "$SIM_LIB" \
                -output "$XCFRAMEWORK"
        }
    rm -rf "$SIM_FW"
fi

echo "==> Done: $XCFRAMEWORK"