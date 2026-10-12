#!/usr/bin/env bash
# Sign, notarize and staple the macOS bundle built by `cargo xtask bundle --formats app,dmg`
# (PDA-T-0145). Runs on macOS only. Secrets come from the environment, never from the repo:
#   APPLE_SIGNING_IDENTITY   "Developer ID Application: Name (TEAMID)"
#   APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD   notarytool credentials (app-specific password)
# usage: packaging/macos/sign-notarize.sh [dist/packages]
set -euo pipefail

[[ "$(uname -s)" == "Darwin" ]] || { echo "macOS only" >&2; exit 2; }
: "${APPLE_SIGNING_IDENTITY:?}" "${APPLE_ID:?}" "${APPLE_TEAM_ID:?}" "${APPLE_APP_PASSWORD:?}"

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="${1:-$HERE/../../dist/packages}"
APP="$OUT/Pando Assistant.app"
ENT="$HERE/entitlements.plist"
[[ -d "$APP" ]] || { echo "missing $APP (run cargo xtask bundle --formats app first)" >&2; exit 1; }

# Localised microphone prompt (es / en) inside the bundle.
for l in es en; do
  mkdir -p "$APP/Contents/Resources/$l.lproj"
  cp "$HERE/$l.lproj/InfoPlist.strings" "$APP/Contents/Resources/$l.lproj/"
done

# Inside-out: workers first, then the app. Hardened runtime + secure timestamp are required.
for bin in pando-stt pando-tts pando-assistant; do
  codesign --force --timestamp --options runtime --entitlements "$ENT" \
    --sign "$APPLE_SIGNING_IDENTITY" "$APP/Contents/MacOS/$bin"
done
codesign --force --timestamp --options runtime --entitlements "$ENT" \
  --sign "$APPLE_SIGNING_IDENTITY" "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"

DMG="$OUT/Pando-Assistant.dmg"
rm -f "$DMG"
hdiutil create -volname "Pando Assistant" -srcfolder "$APP" -ov -format UDZO "$DMG"
codesign --force --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$DMG"

xcrun notarytool submit "$DMG" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" \
  --password "$APPLE_APP_PASSWORD" --wait
xcrun stapler staple "$DMG"
xcrun stapler staple "$APP"
spctl --assess --type execute --verbose=2 "$APP"
echo "ok: $DMG"
# Intel macOS (x86_64): no Whistle build exists (docs/research/voice-stack.md); build pando-stt
# with the Parakeet fallback and run this script on that bundle.
