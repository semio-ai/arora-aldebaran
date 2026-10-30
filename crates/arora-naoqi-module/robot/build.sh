#!/bin/bash
# Builds the library NAOqi loads on a robot, and its package, in dist/arora-naoqi-<arch>/:
#
#   lib/libarora_naoqi.so   the library: Arora, the DCM HAL and its bridge to the DCM;
#   arora_naoqi.json        the configuration of the device (see the crate's readme);
#   autoload.ini            the line to add to NAOqi's autoload.ini.
#
# Usage: build.sh <cross toolchain or C++ SDK of NAOqi> [i686|x86_64]
#
# The cross toolchain of the robot's NAOqi (ctc-linux64-atom-2.1.4.13 for a NAO V4/V5 on NAOqi
# 2.1), or a C++ SDK of the same NAOqi: the bridge is built against the headers of its NAOqi
# (alcommon, alproxies, alvalue, alerror) and its Boost, and links nothing of it: its symbols
# resolve in NAOqi, which has loaded its libraries. With the cross toolchain, the symbols the
# library needs are checked against the robot's own libraries. The architecture is the robot's:
# i686 (NAO V4 and V5, the default) or x86_64.
#
# Needs cargo-zigbuild (the Rust code, for the glibc 2.13 of NAOqi 2.1) and Docker (GCC 4.8 for
# the bridge: an old compiler, whose code needs no more of the robot's libstdc++ than it has).
# Variables: FEATURES (default `studio`), GLIBC (default 2.13), PROFILE (default `robot`, the
# workspace's).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)
SDK=$(cd "${1:?usage: build.sh <cross toolchain or C++ SDK of NAOqi> [i686|x86_64]}" && pwd)
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
# The cross toolchain holds the SDK of the robot in libnaoqi/, and its system in the sysroot.
ROBOT_LIBS=()
if [ -f "$SDK/libnaoqi/include/alcommon/almodule.h" ]; then
  ROBOT_LIBS=("$SDK/libnaoqi/lib" "$(echo "$SDK"/cross/*/sysroot/lib)")
  SDK="$SDK/libnaoqi"
fi
[ -f "$SDK/include/alcommon/almodule.h" ] || { echo "$SDK is not a NAOqi C++ SDK" >&2; exit 2; }
# The SDK's Boost, in include/boost-<version> (the version of the robot's NAOqi), before the
# compiler's own.
BOOST_INCLUDES=$(find "$SDK/include" -maxdepth 1 -name "boost-*" -type d | sort | tail -1)

docker image inspect arora-robot-gxx >/dev/null 2>&1 ||
  docker build --network host -t arora-robot-gxx "$HERE"

TARGET_DIR=$(cd "$ROOT" && cargo metadata --format-version 1 --no-deps |
  sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
export ARORA_ROBOT_MOUNTS="$ROOT:$TARGET_DIR:$SDK:${CARGO_HOME:-$HOME/.cargo}"
export QI_MODULE_NAOQI_SDK="$SDK"
export QI_MODULE_NAOQI_LIBS=""
export QI_MODULE_NAOQI_INCLUDES="$BOOST_INCLUDES"
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

# What the library needs of the robot: glibc symbols by version, and the symbols it leaves to
# NAOqi's process, in the robot's libraries (else in GCC 4.8's libstdc++, NAOqi's or Boost's).
"$HERE/check-symbols.sh" "$LIBRARY" "$M" "${ROBOT_LIBS[@]}"
echo "packaged in $OUT"
