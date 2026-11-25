#!/bin/sh
# Builds the Linux archive. Usage (in the project folder): packaging/linux/make-tar.sh VERSION
set -eu

VERSION="$1"
NAME="MrSizeMe-$VERSION-linux-x86_64"
STAGE="dist/$NAME"

cargo build --release --locked
rm -rf "$STAGE"
mkdir -p "$STAGE"
cp target/release/mrsizeme "$STAGE/"
cp assets/icon/mrsizeme-256.png "$STAGE/mrsizeme.png"
cp packaging/linux/mrsizeme.desktop README.md LICENSE-MIT LICENSE-APACHE THIRD_PARTY_NOTICES.md "$STAGE/"
tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
rm -rf "$STAGE"
