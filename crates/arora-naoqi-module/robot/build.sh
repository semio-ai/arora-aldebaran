#!/bin/bash
# Builds the library NAOqi loads on a robot, and its package, in dist/arora-naoqi-<arch>/:
#
#   lib/libarora_naoqi.so   the library: Arora, the DCM HAL and its bridge to the DCM;
#   arora_naoqi.json        the configuration of the device (see the crate's readme);
#   autoload.ini            the line to add to NAOqi's autoload.ini.
#
# Usage: build.sh <NAOqi C++ SDK> [i686|x86_64]
#
# The SDK is the C++ SDK of the robot's NAOqi (naoqi-sdk-2.1.4.13-linux32 for a NAO V4/V5 on
# NAOqi 2.1): the bridge is built against its headers (alcommon, alproxies, alvalue, alerror and
# its Boost), and links nothing of it: its symbols resolve in NAOqi, which has loaded its
# libraries. The architecture is the robot's: i686 (NAO V4 and V5, the default) or x86_64.
#
# Needs cargo-zigbuild (the Rust code, for the glibc 2.13 of NAOqi 2.1) and Docker (GCC 4.8 for
# the bridge, as old as the libstdc++ of NAOqi 2.1). Variables: FEATURES (default `studio`),
# GLIBC (default 2.13), PROFILE (default `robot`, the workspace's).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)
SDK=$(cd "${1:?usage: build.sh <NAOqi C++ SDK> [i686|x86_64]}" && pwd)
ARCH=${2:-i686}
# An empty FEATURES (`FEATURES=`) builds without Studio: only an unset one defaults.
FEATURES=${FEATURES-studio}
GLIBC=${GLIBC:-2.13}
TARGET="$ARCH-unknown-linux-gnu"
case "$ARCH" in
  i686) M=-m32 ;;
  x86_64) M=-m64 ;;
  *) echo "unknown architecture $ARCH" >&2; exit 2 ;;
esac
[ -f "$SDK/include/alcommon/almodule.h" ] || { echo "$SDK is not a NAOqi C++ SDK" >&2; exit 2; }

docker image inspect arora-robot-gxx >/dev/null 2>&1 ||
  docker build --network host -t arora-robot-gxx "$HERE"

TARGET_DIR=$(cd "$ROOT" && cargo metadata --format-version 1 --no-deps |
  sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
export ARORA_ROBOT_MOUNTS="$ROOT:$TARGET_DIR:$SDK:${CARGO_HOME:-$HOME/.cargo}"
export QI_MODULE_NAOQI_SDK="$SDK"
export QI_MODULE_NAOQI_LIBS=""
# Multithreaded, as the SDK: Boost lays its signals out otherwise.
export QI_MODULE_NAOQI_FLAGS="$M -pthread -D_GLIBCXX_USE_CXX11_ABI=0"
export "CXX_${TARGET//-/_}=$HERE/g++.sh"
# No C++ standard library at link time: zig would link its own (libc++), whose exception
# handling cannot catch what NAOqi's libstdc++ throws. The C++ runtime the bridge uses is the
# libstdc++ and libgcc_s NAOqi has loaded.
export "CXXSTDLIB_${TARGET//-/_}="
# The workspace's `robot` profile: release, without debug information, smaller.
PROFILE=${PROFILE:-robot}
(cd "$ROOT" && cargo zigbuild --profile "$PROFILE" -p arora-naoqi-module --features "bridge${FEATURES:+,$FEATURES}" \
  --target "$TARGET.$GLIBC")

OUT="$ROOT/dist/arora-naoqi-$ARCH"
rm -rf "$OUT" && mkdir -p "$OUT/lib"
LIBRARY="$OUT/lib/libarora_naoqi.so"
cp "$TARGET_DIR/$TARGET/$PROFILE/libarora_naoqi.so" "$LIBRARY"
"$HERE/g++.sh" --version >/dev/null
docker run --rm --network none -v "$OUT:$OUT" arora-robot-gxx strip --strip-unneeded "$LIBRARY"
cp "$HERE/arora_naoqi.json" "$OUT/"
echo "/home/nao/arora-naoqi-$ARCH/lib/libarora_naoqi.so" > "$OUT/autoload.ini"

# What the library needs of the robot: glibc symbols by version, and the C++ symbols it leaves
# to NAOqi's libraries, which must be in the libstdc++ of GCC 4.8 (or NAOqi's own).
"$HERE/check-symbols.sh" "$LIBRARY" "$M"
echo "packaged in $OUT"
