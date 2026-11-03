#!/bin/zsh
set -euo pipefail

SCRIPT_DIR=${0:A:h}
PROJECT_ROOT=${SCRIPT_DIR:h}
APP_DIR="$PROJECT_ROOT/dist/MiraMD.app"
CONTENTS="$APP_DIR/Contents"

cd "$PROJECT_ROOT"
if [[ ! -f macos/Resources/MiraMD.icns || miramd-icon-transparent-logo.png -nt macos/Resources/MiraMD.icns ]]; then
    "$SCRIPT_DIR/generate-icons.sh"
fi
cargo build --release -p miramd-core -p miramd-mcp
MIRAMD_CORE_LIB_DIR="$PROJECT_ROOT/target/release" swift build --package-path macos -c release

rm -rf "$APP_DIR"
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Frameworks" "$CONTENTS/Resources"
cp macos/Info.plist "$CONTENTS/Info.plist"
cp macos/Resources/MiraMD.icns "$CONTENTS/Resources/MiraMD.icns"
cp macos/.build/release/MiraMD "$CONTENTS/MacOS/MiraMD"
cp target/release/miramd-mcp "$CONTENTS/MacOS/miramd-mcp"
cp target/release/libmiramd_core.dylib "$CONTENTS/Frameworks/libmiramd_core.dylib"
if [[ -d macos/.build/release/MiraMD_MiraMD.bundle ]]; then
    cp -R macos/.build/release/MiraMD_MiraMD.bundle "$CONTENTS/Resources/MiraMD_MiraMD.bundle"
fi

CORE_LINK=$(otool -L "$CONTENTS/MacOS/MiraMD" | awk '/libmiramd_core.dylib/ {print $1; exit}')
install_name_tool -change "$CORE_LINK" "@rpath/libmiramd_core.dylib" "$CONTENTS/MacOS/MiraMD"
install_name_tool -id "@rpath/libmiramd_core.dylib" "$CONTENTS/Frameworks/libmiramd_core.dylib"

SIGNING_IDENTITY=${MIRAMD_CODESIGN_IDENTITY:-}
if [[ -z "$SIGNING_IDENTITY" ]]; then
    SIGNING_IDENTITY=$(security find-identity -v -p codesigning 2>/dev/null \
        | awk '/"Developer ID Application:/ { identity=$2 } END { print identity }')
fi
if [[ -z "$SIGNING_IDENTITY" ]]; then
    SIGNING_IDENTITY=$(security find-identity -v -p codesigning 2>/dev/null \
        | awk '/"Apple Development:/ { identity=$2 } END { print identity }')
fi
SIGNING_IDENTITY=${SIGNING_IDENTITY:--}

SIGN_ARGUMENTS=(--force --options runtime --sign "$SIGNING_IDENTITY")
if [[ "$SIGNING_IDENTITY" == "Developer ID Application:"* ]] \
    || security find-identity -v -p codesigning 2>/dev/null \
        | awk -v identity="$SIGNING_IDENTITY" \
            'index($0, identity) && /Developer ID Application:/ { found=1 } END { exit !found }'; then
    SIGN_ARGUMENTS+=(--timestamp)
fi

codesign $SIGN_ARGUMENTS "$CONTENTS/Frameworks/libmiramd_core.dylib"
codesign $SIGN_ARGUMENTS "$CONTENTS/MacOS/miramd-mcp"
codesign $SIGN_ARGUMENTS "$CONTENTS/MacOS/MiraMD"
codesign $SIGN_ARGUMENTS "$APP_DIR"

echo "Built $APP_DIR"
echo "Signed with $SIGNING_IDENTITY"
