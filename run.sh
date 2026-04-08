#!/bin/bash
set -e

BUILD_DIR="$(pwd)/.build"
CLEAN_RESTART=0

usage() {
  echo "Usage: $0 [--clean-restart]"
  echo "  --clean-restart    Remove local build cache before rebuilding"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --clean-restart)
      CLEAN_RESTART=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ "$CLEAN_RESTART" -eq 1 ]]; then
  echo "==> Performing clean restart (clearing .build)..."
  rm -rf "$BUILD_DIR"
fi

echo "==> Building amux..."
xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug \
  -derivedDataPath "$BUILD_DIR" \
  build 2>&1 | tail -1

APP="$BUILD_DIR/Build/Products/Debug/amux.app"

echo "==> Killing existing amux..."
killall amux 2>/dev/null || true
sleep 1

echo "==> Launching amux (Ctrl+C to quit)..."
"$APP/Contents/MacOS/amux"
