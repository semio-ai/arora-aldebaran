#!/bin/bash
# Prints the name of the image of the Dockerfile beside this script, built if it is not yet:
# its tag is the hash of the Dockerfile, so that a change of it builds the image again.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
IMAGE="arora-robot-gxx:$(sha256sum "$HERE/Dockerfile" | cut -c1-12)"
docker image inspect "$IMAGE" >/dev/null 2>&1 ||
  docker build -q --network host -t "$IMAGE" "$HERE" >&2
echo "$IMAGE"
