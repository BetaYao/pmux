#!/bin/bash
set -e

BUILD_DIR="$(pwd)/.build"

echo "==> Building pmux..."
xcodebuild -project pmux.xcodeproj -scheme pmux -configuration Debug \
  -derivedDataPath "$BUILD_DIR" \
  build 2>&1 | tail -1

APP="$BUILD_DIR/Build/Products/Debug/pmux.app"

echo "==> Killing existing pmux..."
killall pmux 2>/dev/null || true
sleep 1

echo "==> Launching pmux..."
open -n "$APP"
