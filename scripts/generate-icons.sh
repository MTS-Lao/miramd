#!/bin/zsh
set -euo pipefail

SCRIPT_DIR=${0:A:h}
PROJECT_ROOT=${SCRIPT_DIR:h}
SOURCE="$PROJECT_ROOT/miramd-icon-transparent-logo.png"
MAC_OUTPUT="$PROJECT_ROOT/macos/Resources/MiraMD.icns"
WINDOWS_OUTPUT="$PROJECT_ROOT/windows/Assets/MiraMD.ico"

if [[ ! -f "$SOURCE" ]]; then
    echo "Missing logo: $SOURCE" >&2
    exit 1
fi

ICON_TEMP=$(mktemp -d /tmp/miramd-iconset.XXXXXX)
ICONSET="$ICON_TEMP/MiraMD.iconset"
mkdir -p "$ICONSET"
trap 'rm -rf "$ICON_TEMP"' EXIT

mkdir -p "${MAC_OUTPUT:h}" "${WINDOWS_OUTPUT:h}"

resize_icon() {
    local size=$1
    local name=$2
    sips -z "$size" "$size" "$SOURCE" --out "$ICONSET/$name" >/dev/null
}

resize_icon 16 icon_16x16.png
resize_icon 32 icon_16x16@2x.png
resize_icon 32 icon_32x32.png
resize_icon 64 icon_32x32@2x.png
resize_icon 128 icon_128x128.png
resize_icon 256 icon_128x128@2x.png
resize_icon 256 icon_256x256.png
resize_icon 512 icon_256x256@2x.png
resize_icon 512 icon_512x512.png
resize_icon 1024 icon_512x512@2x.png

iconutil -c icns "$ICONSET" -o "$MAC_OUTPUT"

if command -v magick >/dev/null 2>&1; then
    magick "$SOURCE" -define icon:auto-resize=256,128,64,48,32,16 "$WINDOWS_OUTPUT"
else
    echo "ImageMagick is required to generate the Windows .ico asset." >&2
    exit 1
fi

echo "Generated $MAC_OUTPUT"
echo "Generated $WINDOWS_OUTPUT"
