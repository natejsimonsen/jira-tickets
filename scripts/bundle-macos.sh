#!/bin/sh
# Builds "Jira Tickets.app", ad-hoc signs it, and installs it in /Applications.
# Usage: scripts/bundle-macos.sh [--no-install] [install-dir]   (default: /Applications)
#   --no-install  build and sign the bundle under target/release, print its path, do not copy it
# Environment:
#   CARGO  the cargo command (default: nix develop --command cargo)
#   BIN    a prebuilt binary to bundle (default: target/release/jira-tickets);
#          when set, the cargo build is skipped (used for the universal CI build)
set -eu
cd "$(dirname "$0")/.."
CARGO="${CARGO:-nix develop --command cargo}"
INSTALL=1
if [ "${1:-}" = "--no-install" ]; then
    INSTALL=0
    shift
fi
DEST="${1:-/Applications}"
NAME="Jira Tickets"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
APP="target/release/$NAME.app"

if [ -z "${BIN:-}" ]; then
    $CARGO build --release
    BIN="target/release/jira-tickets"
fi

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
if [ "$INSTALL" = 0 ]; then
    echo "Built $PWD/$APP"
    exit 0
fi
rm -rf "${DEST:?}/$NAME.app"
cp -R "$APP" "$DEST/"
echo "Installed $DEST/$NAME.app"
