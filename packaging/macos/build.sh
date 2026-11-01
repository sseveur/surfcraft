#!/bin/zsh
# Build SurfCraft.app and SurfCraft-<version>-macos.dmg, sign with the local Apple Development certificate
# (ad-hoc if none), and install to /Applications.
#
#   packaging/macos/build.sh            build, sign, dmg, install
#   packaging/macos/build.sh --no-install
#
# Universal (arm64 + x86_64) when the x86_64-apple-darwin Rust target is installed, else native only.
set -euo pipefail
cd "${0:A:h}/../.."

INSTALL=1
[[ "${1:-}" == "--no-install" ]] && INSTALL=0

VERSION=$(cargo metadata --no-deps --format-version 1 | python3 -I -c 'import json,sys; print([p for p in json.load(sys.stdin)["packages"] if p["name"]=="surfcraft-app"][0]["version"])')
OUT=target/package/macos
APP="$OUT/SurfCraft.app"

TARGETS=(aarch64-apple-darwin)
if rustup target list --installed | grep -q '^x86_64-apple-darwin$'; then
  TARGETS+=(x86_64-apple-darwin)
fi
for t in $TARGETS; do
  cargo build --release -p surfcraft-app -p surfcraft-mcp --target "$t"
done

rm -rf "$OUT"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
# The app and the MCP server (for AI assistants) live side by side.
for bin in surfcraft surfcraft-mcp; do
  PARTS=()
  for t in $TARGETS; do PARTS+=("target/$t/release/$bin"); done
  if (( ${#PARTS} > 1 )); then
    lipo -create -output "$APP/Contents/MacOS/$bin" $PARTS
  else
    cp "${PARTS[1]}" "$APP/Contents/MacOS/$bin"
  fi
done
# Apple wants purely numeric bundle versions: 4.0.0-alpha.1 -> 4.0.0 (the full version is kept in SurfCraftVersion).
SHORT_VERSION=${VERSION%%-*}
sed -e "s/__SHORT_VERSION__/$SHORT_VERSION/g" -e "s/__VERSION__/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"

# Icon set from the 1024 px artwork (packaging/icon-1024.png).
ICONSET="$OUT/SurfCraft.iconset"
mkdir -p "$ICONSET"
SRC=packaging/icon-1024.png
for s in 16 32 64 128 256 512 1024; do
  sips -z $s $s "$SRC" --out "$ICONSET/icon_${s}x${s}.png" > /dev/null
done
cp "$ICONSET/icon_32x32.png" "$ICONSET/icon_16x16@2x.png"
cp "$ICONSET/icon_64x64.png" "$ICONSET/icon_32x32@2x.png"
cp "$ICONSET/icon_256x256.png" "$ICONSET/icon_128x128@2x.png"
cp "$ICONSET/icon_512x512.png" "$ICONSET/icon_256x256@2x.png"
mv "$ICONSET/icon_1024x1024.png" "$ICONSET/icon_512x512@2x.png"
rm "$ICONSET/icon_64x64.png"
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/SurfCraft.icns"
rm -r "$ICONSET"

IDENTITY=$(security find-identity -v -p codesigning | awk -F'"' '/Apple Development|Developer ID Application/ {print $2; exit}')
codesign --force --options runtime --timestamp=none --identifier org.surfcraft.SurfCraft.mcp \
  --sign "${IDENTITY:--}" "$APP/Contents/MacOS/surfcraft-mcp"
codesign --force --options runtime --timestamp=none --sign "${IDENTITY:--}" "$APP"
codesign --verify --strict "$APP"

DMG="target/package/SurfCraft-$VERSION-macos.dmg"
STAGE="$OUT/dmg"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
hdiutil create -quiet -volname "SurfCraft $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
rm -r "$STAGE"

if (( INSTALL )); then
  pkill -x surfcraft || true
  rm -rf /Applications/SurfCraft.app
  cp -R "$APP" /Applications/
  echo "Installed /Applications/SurfCraft.app"
fi
echo "Built $APP and $DMG (archs: $TARGETS; signed: ${IDENTITY:-ad-hoc})"
