#!/usr/bin/env bash
# Regenerate test/corpus/commons-codec.tsv from test/corpus/names.txt using
# Apache Commons Codec as the reference implementation.
#
# Requires Java 11+ (single-file source launch). The jar is downloaded from
# Maven Central into a temporary directory; nothing is added to the repo.
set -euo pipefail

VERSION=1.22.1
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CORPUS="$HERE/../corpus"
JAR="$(mktemp -d)/commons-codec-$VERSION.jar"

curl -fsSL -o "$JAR" \
  "https://repo1.maven.org/maven2/commons-codec/commons-codec/$VERSION/commons-codec-$VERSION.jar"

java -cp "$JAR" "$HERE/Oracle.java" "$CORPUS/names.txt" "$CORPUS/commons-codec.tsv"

echo "Wrote $CORPUS/commons-codec.tsv ($(wc -l < "$CORPUS/commons-codec.tsv") lines, commons-codec $VERSION)"
