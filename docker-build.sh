#!/usr/bin/env bash
# Builds the taginfo-api:local image. Run from this directory.
#
# Uses a named build context for the sibling osmflat-ext repo, consumed as a
# Cargo path dependency -- see the Dockerfile's top-of-file comment.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

podman build \
    --build-context osmflat-ext=../osmflat-ext \
    -t taginfo-api:local \
    .
