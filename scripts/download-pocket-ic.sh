#!/usr/bin/env bash

set -eEuo pipefail

PLATFORM="$(uname -s | tr '[:upper:]' '[:lower:]')"
echo $PLATFORM

curl -sLO https://github.com/dfinity/pocketic/releases/download/7.0.0/pocket-ic-x86_64-$PLATFORM.gz || exit 1
gzip -df pocket-ic-x86_64-$PLATFORM.gz
mv pocket-ic-x86_64-$PLATFORM pocket-ic
chmod +x pocket-ic
