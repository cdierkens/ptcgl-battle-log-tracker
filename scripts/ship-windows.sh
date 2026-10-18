#!/usr/bin/env bash
# Ship the Windows build to a stable output dir for the UTM guest.
#
# Usage: ./scripts/ship-windows.sh
#   - Cross-compiles the Windows release .exe (pinned 1.86 toolchain via cargo-xwin)
#   - Stages ptcgl-battle-log-capture.exe + a zip into ship-windows/
#   - Point your UTM "Shared Directory" at ship-windows/ so the Windows guest can see it
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
LLVM_PREFIX="$(brew --prefix llvm)"

SHIP_DIR="$REPO_DIR/ship-windows"
EXE_SRC="$REPO_DIR/src-tauri/target/x86_64-pc-windows-msvc/release/ptcgl-battle-log-capture.exe"
ZIP_NAME="PTCGL Tracker_0.1.0_x86_64.zip"
ZIP_SRC="$REPO_DIR/src-tauri/target/x86_64-pc-windows-msvc/release/$ZIP_NAME"

echo "==> Building frontend"
pnpm --dir "$REPO_DIR" build

echo "==> Cross-compiling Windows release (x86_64-pc-windows-msvc)"
PATH="$LLVM_PREFIX/bin:$PATH" \
  cargo xwin build --release --features custom-protocol --target x86_64-pc-windows-msvc \
  --manifest-path "$REPO_DIR/src-tauri/Cargo.toml"

mkdir -p "$SHIP_DIR"

echo "==> Staging .exe"
cp -f "$EXE_SRC" "$SHIP_DIR/ptcgl-battle-log-capture.exe"

echo "==> Staging zip"
if [ -f "$ZIP_SRC" ]; then
  cp -f "$ZIP_SRC" "$SHIP_DIR/$ZIP_NAME"
else
  (cd "$SHIP_DIR" && zip -r "$ZIP_NAME" ptcgl-battle-log-capture.exe)
fi

echo "==> Done. Ship dir: $SHIP_DIR"
ls -lh "$SHIP_DIR"
