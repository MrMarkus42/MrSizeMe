#!/bin/sh
# Checks a published release: checksums, signatures (with the key built into
# the program) and that it is the "latest" release.
# Usage (Linux, in the project folder): scripts/verify-release.sh v1.0.0
set -eu

TAG="$1"
REPO="MrMarkus42/MrSizeMe"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

cargo build --release --locked
BIN="$PWD/target/release/mrsizeme"

gh release download "$TAG" --repo "$REPO" --dir "$WORK"
cd "$WORK"
echo "== checksums"
sha256sum -c SHA256SUMS.txt
echo "== signatures"
for f in *; do
    case "$f" in *.minisig) continue ;; esac
    [ -f "$f.minisig" ] || { echo "missing signature: $f"; exit 1; }
    "$BIN" --verify-update "$f" "$f.minisig"
done
echo "== latest"
latest=$(gh api "repos/$REPO/releases/latest" --jq .tag_name)
[ "$latest" = "$TAG" ] || { echo "latest is $latest, not $TAG"; exit 1; }
echo "OK: $TAG"
