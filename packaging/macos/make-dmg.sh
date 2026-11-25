#!/bin/sh
# Builds MrSizeMe.app (Apple silicon and Intel in one file) and the .dmg.
# Usage (on a Mac, in the project folder): packaging/macos/make-dmg.sh VERSION
set -eu

VERSION="$1"
OUT="dist"
APP="$OUT/MrSizeMe.app"

rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --locked --target aarch64-apple-darwin
cargo build --release --locked --target x86_64-apple-darwin

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create \
    target/aarch64-apple-darwin/release/mrsizeme \
    target/x86_64-apple-darwin/release/mrsizeme \
    -output "$APP/Contents/MacOS/mrsizeme"
sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
cp assets/icon/mrsizeme.icns "$APP/Contents/Resources/"
cp LICENSE-MIT LICENSE-APACHE THIRD_PARTY_NOTICES.md "$APP/Contents/Resources/"
# No Apple developer certificate: an ad-hoc signature keeps Apple silicon happy.
codesign --force --deep --sign - "$APP"

STAGE="$OUT/dmg"
rm -rf "$STAGE"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "MrSizeMe $VERSION" -srcfolder "$STAGE" -ov -format UDZO \
    "$OUT/MrSizeMe-$VERSION-macos.dmg"
rm -rf "$STAGE"
