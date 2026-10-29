#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release -p t405a-gsi -p t405a-gui
mkdir -p dist/macos
if [ -d dist/macos/t405a-gsi.app ]; then
  rm -rf dist/macos/t405a-gsi.app
fi
APP="dist/macos/t405a-gsi.app"
mkdir -p "$APP/Contents/MacOS"
cp target/release/t405a-gsi-gui "$APP/Contents/MacOS/t405a-gsi"
cp target/release/t405a-gsi dist/macos/t405a-gsi-cli
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>t405a-gsi</string>
  <key>CFBundleDisplayName</key><string>onn 11 Core Installer</string>
  <key>CFBundleIdentifier</key><string>dev.t405a.gsi</string>
  <key>CFBundleVersion</key><string>0.1.0</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleExecutable</key><string>t405a-gsi</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
echo "built:"
echo "  dist/macos/t405a-gsi.app"
echo "  dist/macos/t405a-gsi-cli"
