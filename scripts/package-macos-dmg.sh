#!/bin/zsh
set -euo pipefail

SCRIPT_DIR=${0:A:h}
PROJECT_ROOT=${SCRIPT_DIR:h}
APP_DIR="$PROJECT_ROOT/dist/MiraMD.app"
INFO_PLIST="$PROJECT_ROOT/macos/Info.plist"
VERSION=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$INFO_PLIST")
ARCH=${MIRAMD_MACOS_ARCH:-$(uname -m)}
OUTPUT_PATH=${1:-"$PROJECT_ROOT/dist/MiraMD-$VERSION-macOS-$ARCH.dmg"}
VOLUME_NAME="MiraMD Installer"
MOUNT_PATH="/Volumes/$VOLUME_NAME"
ALLOW_UNNOTARIZED=${MIRAMD_ALLOW_UNNOTARIZED:-0}
SKIP_BUILD=${MIRAMD_SKIP_BUILD:-0}
SKIP_FINDER_LAYOUT=${MIRAMD_DMG_SKIP_FINDER_LAYOUT:-0}
NOTARY_PROFILE=${MIRAMD_NOTARY_PROFILE:-}

if [[ "$SKIP_BUILD" != "1" ]]; then
    "$SCRIPT_DIR/build-macos.sh"
fi

if [[ ! -d "$APP_DIR" ]]; then
    print -u2 "MiraMD.app was not found at $APP_DIR"
    exit 1
fi

DEVELOPER_ID=${MIRAMD_CODESIGN_IDENTITY:-}
if [[ -z "$DEVELOPER_ID" ]]; then
    DEVELOPER_ID=$(security find-identity -v -p codesigning 2>/dev/null \
        | awk '/"Developer ID Application:/ { identity=$2 } END { print identity }')
elif ! security find-identity -v -p codesigning 2>/dev/null \
    | awk -v identity="$DEVELOPER_ID" \
        'index($0, identity) && /Developer ID Application:/ { found=1 } END { exit !found }'; then
    DEVELOPER_ID=
fi

if [[ -z "$DEVELOPER_ID" && "$ALLOW_UNNOTARIZED" != "1" ]]; then
    print -u2 "A Developer ID Application certificate and private key are required for a public DMG."
    print -u2 "Install them in Keychain Access, or set MIRAMD_ALLOW_UNNOTARIZED=1 only for local testing."
    exit 1
fi

codesign --verify --deep --strict --verbose=2 "$APP_DIR"

if [[ -n "$DEVELOPER_ID" && -z "$NOTARY_PROFILE" && "$ALLOW_UNNOTARIZED" != "1" ]]; then
    print -u2 "Set MIRAMD_NOTARY_PROFILE to a notarytool keychain profile for a public DMG."
    print -u2 "Create one with: xcrun notarytool store-credentials PROFILE_NAME"
    exit 1
fi

if [[ -n "$DEVELOPER_ID" ]]; then
    APP_AUTHORITY=$(codesign -dv --verbose=4 "$APP_DIR" 2>&1 \
        | awk -F= '/^Authority=/ && !found { print $2; found=1 }')
    if [[ "$APP_AUTHORITY" != "Developer ID Application:"* ]]; then
        print -u2 "MiraMD.app is not signed with Developer ID Application. Rebuild it before packaging."
        exit 1
    fi
fi

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/miramd-dmg.XXXXXX")
STAGING_DIR="$WORK_DIR/staging"
READ_WRITE_DMG="$WORK_DIR/MiraMD-rw.dmg"
MOUNTED=0

cleanup() {
    if [[ "$MOUNTED" == "1" ]]; then
        hdiutil detach "$MOUNT_PATH" -quiet || true
    fi
    rm -rf "$WORK_DIR"
}
trap cleanup EXIT INT TERM

mkdir -p "$STAGING_DIR" "${OUTPUT_PATH:h}"
ditto "$APP_DIR" "$STAGING_DIR/MiraMD.app"
ln -s /Applications "$STAGING_DIR/Applications"
mkdir -p "$STAGING_DIR/.fseventsd"
touch "$STAGING_DIR/.fseventsd/no_log"

hdiutil create \
    -volname "$VOLUME_NAME" \
    -srcfolder "$STAGING_DIR" \
    -format UDRW \
    -ov \
    "$READ_WRITE_DMG" >/dev/null

if [[ -e "$MOUNT_PATH" ]]; then
    print -u2 "$MOUNT_PATH is already mounted. Eject it and run the command again."
    exit 1
fi

hdiutil attach "$READ_WRITE_DMG" \
    -readwrite \
    -noverify \
    -noautoopen \
    -mountpoint "$MOUNT_PATH" >/dev/null
MOUNTED=1

if [[ "$SKIP_FINDER_LAYOUT" != "1" ]]; then
    if ! osascript <<APPLESCRIPT
tell application "Finder"
    tell disk "$VOLUME_NAME"
        open
        set current view of container window to icon view
        set toolbar visible of container window to false
        set statusbar visible of container window to false
        set the bounds of container window to {120, 120, 720, 500}
        set arrangement of icon view options of container window to not arranged
        set icon size of icon view options of container window to 128
        set text size of icon view options of container window to 14
        set position of item "MiraMD.app" of container window to {165, 190}
        set position of item "Applications" of container window to {435, 190}
        update without registering applications
        delay 2
        close
    end tell
end tell
APPLESCRIPT
    then
        print -u2 "Warning: Finder layout could not be applied; the DMG still contains the app and Applications shortcut."
    fi
fi

sync
hdiutil detach "$MOUNT_PATH" -quiet
MOUNTED=0

hdiutil convert "$READ_WRITE_DMG" \
    -format UDZO \
    -imagekey zlib-level=9 \
    -ov \
    -o "$OUTPUT_PATH" >/dev/null

if [[ -n "$DEVELOPER_ID" ]]; then
    codesign --force --timestamp --sign "$DEVELOPER_ID" "$OUTPUT_PATH"
fi

if [[ -n "$DEVELOPER_ID" && -n "$NOTARY_PROFILE" ]]; then
    xcrun notarytool submit "$OUTPUT_PATH" \
        --keychain-profile "$NOTARY_PROFILE" \
        --wait
    xcrun stapler staple "$OUTPUT_PATH"
    xcrun stapler validate "$OUTPUT_PATH"
    spctl --assess --type open --context context:primary-signature --verbose=2 "$OUTPUT_PATH"
elif [[ "$ALLOW_UNNOTARIZED" == "1" ]]; then
    print -u2 "Warning: this DMG is not notarized and must not be published as a public release."
fi

hdiutil verify "$OUTPUT_PATH" >/dev/null

print "Created $OUTPUT_PATH"
print "The DMG contains MiraMD.app and an Applications shortcut."
