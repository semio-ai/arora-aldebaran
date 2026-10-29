#!/bin/bash
# The C++ compiler build.sh gives the bridge: the g++ of the image of the Dockerfile beside
# this script, run on the same paths. ARORA_ROBOT_MOUNTS lists the directories to mount, separated
# by ':' (the sources, the SDK and the build directories); ARORA_ROBOT_GXX_IMAGE names the image.
mounts=()
IFS=':' read -ra dirs <<< "$ARORA_ROBOT_MOUNTS"
for dir in "${dirs[@]}"; do
  [ -n "$dir" ] && mounts+=(-v "$dir:$dir")
done
exec docker run --rm --network none -u "$(id -u):$(id -g)" "${mounts[@]}" -w "$PWD" \
  "${ARORA_ROBOT_GXX_IMAGE:-arora-robot-gxx}" g++ "$@"
