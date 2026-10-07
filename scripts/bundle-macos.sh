#!/bin/sh
# Builds "Jira Tickets.app" and installs it in /Applications.
# Usage: scripts/bundle-macos.sh [install-dir]   (default: /Applications)
set -eu
cd "$(dirname "$0")/.."
CARGO="${CARGO:-nix develop --command cargo}"
DEST="${1:-/Applications}"
NAME="Jira Tickets"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
APP="target/release/$NAME.app"

$CARGO build --release
BIN="target/release/jira-tickets"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/jira-tickets"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$NAME</string>
  <key>CFBundleDisplayName</key><string>$NAME</string>
  <key>CFBundleIdentifier</key><string>dev.jira-tickets</string>
  <key>CFBundleExecutable</key><string>jira-tickets</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
codesign --force --sign - "$APP"
rm -rf "$DEST/$NAME.app"
cp -R "$APP" "$DEST/"
echo "Installed $DEST/$NAME.app"
