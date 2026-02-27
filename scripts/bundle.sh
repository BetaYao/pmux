#!/usr/bin/env bash
# Bundle pmux as .app (macOS) - generates icon and runs cargo bundle
# Usage:
#   ./scripts/bundle.sh         # Release build with standard icon
#   ./scripts/bundle.sh --dev   # Release build with DEV badge icon
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
RESOURCES="$PROJECT_ROOT/resources"

cd "$PROJECT_ROOT"

# Generate icon (use --dev for dev mode variant)
# Prefer project venv if exists (pip install -r resources/requirements.txt)
echo "Generating app icon..."
PYTHON="python3"
if [[ -x "$PROJECT_ROOT/.venv-icon/bin/python" ]]; then
    PYTHON="$PROJECT_ROOT/.venv-icon/bin/python"
fi
if [[ "$1" == "--dev" ]]; then
    $PYTHON "$RESOURCES/generate_icon.py" --dev
    shift
else
    $PYTHON "$RESOURCES/generate_icon.py"
fi

# Ensure Xcode tools for Metal (macOS)
if [[ "$(uname)" == "Darwin" ]]; then
    export PATH="/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin:$PATH"
    export SDKROOT="${SDKROOT:-$(xcrun --sdk macosx --show-sdk-path 2>/dev/null)}"
fi

# Bundle (requires cargo-bundle: cargo install cargo-bundle)
if cargo bundle --help &>/dev/null; then
    cargo bundle --release "$@"
    echo ""
    echo "Bundle complete. Output: target/release/bundle/macos/pmux.app"
else
    echo "cargo-bundle not found. Install with: cargo install cargo-bundle"
    echo "Falling back to cargo build --release..."
    cargo build --release "$@"
fi
