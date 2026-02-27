#!/usr/bin/env bash
# pmux run script - ensures macOS build dependencies before cargo run
set -e

# Ensure Xcode developer tools are in PATH for Metal shader compilation
export PATH="/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin:$PATH"
export SDKROOT="${SDKROOT:-$(xcrun --sdk macosx --show-sdk-path)}"

# Xcode 26+ requires Metal Toolchain to be installed separately
if ! xcodebuild -showComponent MetalToolchain &>/dev/null; then
    echo "Metal Toolchain not installed. Downloading (one-time, ~700MB)..."
    xcodebuild -downloadComponent MetalToolchain
fi

exec cargo run "$@"
