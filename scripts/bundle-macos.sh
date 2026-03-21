#!/bin/bash
# Build a macOS .app bundle for Reflex.
# Usage: ./scripts/bundle-macos.sh [--release]

set -euo pipefail

PROFILE="debug"
if [[ "${1:-}" == "--release" ]]; then
    PROFILE="release"
    cargo build --release
else
    cargo build
fi

APP_NAME="Reflex.app"
APP_DIR="target/${PROFILE}/${APP_NAME}"
CONTENTS="${APP_DIR}/Contents"
MACOS="${CONTENTS}/MacOS"
RESOURCES="${CONTENTS}/Resources"

rm -rf "${APP_DIR}"
mkdir -p "${MACOS}" "${RESOURCES}"

cp "target/${PROFILE}/reflex" "${MACOS}/reflex"
cp resources/Info.plist "${CONTENTS}/Info.plist"
cp resources/reflex.icns "${RESOURCES}/reflex.icns"

echo "Built ${APP_DIR}"
echo "Run with: open ${APP_DIR}"
