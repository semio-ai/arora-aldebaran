#!/bin/bash
# Runs the 32-bit fake-dcm of build-i686.sh in the image it was built in, as fake-dcm runs:
# `fake-dcm-i686.sh <library> <seconds>`, with the variables of the tests (ARORA_NAOQI_CONFIG,
# QI_URL, HOME, RUST_LOG). The library's directory, this one and /tmp are mounted.
HERE=$(cd "$(dirname "$0")" && pwd)
LIBRARY=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
shift
exec docker run --rm --network host -v /tmp:/tmp -v "$HERE:$HERE" \
  -v "$(dirname "$LIBRARY"):$(dirname "$LIBRARY")" \
  -e ARORA_NAOQI_CONFIG -e QI_URL -e HOME -e RUST_LOG \
  arora-robot-gxx "$HERE/build/i686/fake-dcm" "$LIBRARY" "$@"
