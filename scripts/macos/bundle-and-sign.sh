#!/usr/bin/env bash
# Build rdc, wrap it in rdc.app and sign it with a stable identity so macOS TCC grants
# (Screen Recording, Accessibility) survive rebuilds.
#
# One-time setup on the Mac (GUI, not over SSH):
#   Keychain Access → Certificate Assistant → Create a Certificate…
#     Name: rdc-dev   Identity Type: Self Signed Root   Certificate Type: Code Signing
#   Then run this script once from Terminal.app and click "Always Allow" when codesign asks
#   for keychain access. After that it works over SSH.
#
# Usage: scripts/macos/bundle-and-sign.sh [identity] [install-dir]
set -euo pipefail
IDENTITY="${1:-${RDC_SIGN_IDENTITY:-rdc-dev}}"
INSTALL_DIR="${2:-$HOME/Applications}"
cd "$(dirname "$0")/../.."
export PATH="$HOME/.cargo/bin:$PATH"

cargo build --release
VERSION=$(cargo pkgid | sed 's/.*[#@]//')
APP="$INSTALL_DIR/rdc.app"
mkdir -p "$APP/Contents/MacOS"
cp target/release/rdc "$APP/Contents/MacOS/rdc"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>dev.bscott.rdc</string>
  <key>CFBundleName</key><string>rdc</string>
  <key>CFBundleExecutable</key><string>rdc</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSUIElement</key><true/>
  <key>LSBackgroundOnly</key><false/>
  <key>NSScreenCaptureUsageDescription</key><string>rdc lets an authorised agent on your tailnet see this screen.</string>
  <key>NSAppleEventsUsageDescription</key><string>rdc focuses windows on behalf of an authorised agent.</string>
</dict>
</plist>
PLIST

if security find-identity -v -p codesigning | grep -q "\"$IDENTITY\""; then
  codesign --force --deep --options runtime --timestamp=none \
    --identifier dev.bscott.rdc --sign "$IDENTITY" "$APP"
  echo "signed with $IDENTITY"
else
  echo "warning: no code-signing identity '$IDENTITY' in keychain; falling back to ad-hoc." >&2
  echo "         TCC grants will reset on every rebuild. See the header of this script." >&2
  codesign --force --deep --identifier dev.bscott.rdc --sign - "$APP"
fi
codesign --verify --verbose=2 "$APP" 2>&1 | tail -2
echo "bundle: $APP"
echo "next:   $APP/Contents/MacOS/rdc doctor --request-permissions   (from the Mac's GUI session)"
echo "then:   $APP/Contents/MacOS/rdc service install"
