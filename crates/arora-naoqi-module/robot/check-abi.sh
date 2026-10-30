#!/bin/bash
# Checks the declarations of naoqi-2.1/ against the C++ SDK of NAOqi 2.1, in its cross toolchain
# (ctc-linux64-atom-2.1.4.13: see the readme): the DCM bridge is the same built against either.
# In 32 and 64 bits, in the image of the Dockerfile (GCC 4.8, Boost 1.55), without network:
#
# - the classes the bridge uses have the same layout (naoqi-2.1/abi-probe.cpp);
# - the bridge (cpp/naoqi_shim.cpp of libqi-vibe, the version of Cargo.lock), compiled without
#   optimization against either: needs no symbol more than with the SDK, defines no function or
#   vtable more, and each one both define has the same code, but for where its local data is in
#   the object. With the SDK, it defines more: the templates of libqi its headers instantiate.
#
# Only the headers and the compiler's code are compared: nothing of the toolchain runs.
#
# Usage: check-abi.sh <cross toolchain or C++ SDK of NAOqi 2.1>
set -euo pipefail

if [ "${1:-}" = --inside ]; then
  # In the container: --inside <SDK> <our declarations> <naoqi_shim.cpp> <work directory>
  SDK=$2 OURS=$3 SHIM=$4
  cd "$5"
  FLAGS=(-pthread -D_GLIBCXX_USE_CXX11_ABI=0 -DBOOST_SIGNALS_NO_DEPRECATION_WARNING -fPIC
    -ffunction-sections -fdata-sections -Wno-invalid-offsetof)
  # The SDK's Boost, in include/boost-<version>, else the image's.
  BOOST=$(find "$SDK/include" -maxdepth 1 -name "boost-*" -type d | sort | tail -1)
  SDK_INCLUDES=(-I"$SDK/include" ${BOOST:+-I"$BOOST"})
  failed=0
  # The probes: <name> <value> of each function probe_, from its `mov $value` (`xor` for 0).
  probes() {
    objdump -d --no-show-raw-insn "$1" | awk '
      /<probe_.*>:/ { name = $2; gsub(/[<>:]/, "", name) }
      name && /mov +\$0x[0-9a-f]+,%[er]ax/ { match($0, /\$0x[0-9a-f]+/); print name, substr($0, RSTART + 1, RLENGTH - 1); name = "" }
      name && /xor +%eax,%eax/ { print name, 0; name = "" }'
  }
  # The sections of an object named after a symbol: functions, vtables and typeinfos.
  sections() {
    readelf -SW "$1" | grep -oE '\.(text|data\.rel\.ro(\.local)?|rodata)\.[A-Za-z0-9_$.]+' |
      grep -vE '^\.text\.(startup|unlikely)|_GLOBAL__sub_I|__static_initialization_and_destruction' | sort -u
  }
  # The content of a section: its code, or its data and relocations, with the references to the
  # local data of the object (string literals, statics) masked. Each function is a section of
  # its own: its addresses, its branches' included, are from its start.
  content() {
    case "$2" in
      .text.*) objdump -dr --no-show-raw-insn -j "$2" "$1" | sed -n '/>:$/,$p' ;;
      *) objdump -sr -j "$2" "$1" | sed -n '/^RELOCATION\|^Contents/,$p' ;;
    esac |
      awk '/R_[A-Z0-9_]+[ \t]+\.(rodata|bss|data)/ { sub(/0x[0-9a-f]+/, "<local>", previous); sub(/\+0x[0-9a-f]+$/, "") }
           NR > 1 { print previous } { previous = $0 } END { print previous }'
  }
  for m in -m32 -m64; do
    g++ $m "${FLAGS[@]}" -O2 "${SDK_INCLUDES[@]}" -c "$OURS/abi-probe.cpp" -o sdk-probe.o
    g++ $m "${FLAGS[@]}" -O2 -I"$OURS/include" -c "$OURS/abi-probe.cpp" -o our-probe.o
    if ! diff <(probes sdk-probe.o) <(probes our-probe.o) > probes.diff; then
      echo "$m: layouts differ (< the SDK, > ours):" >&2; cat probes.diff >&2; failed=1
    else
      echo "$m: the $(probes our-probe.o | wc -l) sizes and offsets of the classes are the same"
    fi
    g++ $m "${FLAGS[@]}" -O0 "${SDK_INCLUDES[@]}" -c "$SHIM" -o sdk.o
    g++ $m "${FLAGS[@]}" -O0 -I"$OURS/include" -c "$SHIM" -o ours.o
    more=$(comm -13 <(nm -u sdk.o | awk '{print $2}' | sort) <(nm -u ours.o | awk '{print $2}' | sort))
    if [ -n "$more" ]; then
      echo "$m: the bridge needs more symbols with ours:" >&2; echo "$more" | c++filt >&2; failed=1
    fi
    more=$(comm -13 <(sections sdk.o) <(sections ours.o))
    if [ -n "$more" ]; then
      echo "$m: the bridge defines more with ours:" >&2; echo "$more" | c++filt >&2; failed=1
    fi
    same=0
    for section in $(comm -12 <(sections sdk.o) <(sections ours.o)); do
      if [ "$(content sdk.o "$section")" = "$(content ours.o "$section")" ]; then
        same=$((same + 1))
      else
        echo "$m: differs: $section ($(echo "${section#.*.}" | sed -E 's/^(rel\.ro\.)?(local\.)?//' | c++filt))" >&2
        failed=1
      fi
    done
    echo "$m: the bridge has the same $same functions, vtables and typeinfos with ours" \
      "($(comm -23 <(sections sdk.o) <(sections ours.o) | wc -l) more with the SDK)"
  done
  exit $failed
fi

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)
SDK=$(cd "${1:?usage: check-abi.sh <cross toolchain or C++ SDK of NAOqi 2.1>}" && pwd)
[ -f "$SDK/libnaoqi/include/alcommon/almodule.h" ] && SDK="$SDK/libnaoqi"
[ -f "$SDK/include/alcommon/almodule.h" ] || { echo "$SDK is not a NAOqi C++ SDK" >&2; exit 2; }
VERSION=$(awk '/^name = "libqi-vibe"$/ {getline; gsub(/"/, "", $3); print $3}' "$ROOT/Cargo.lock")
(cd "$ROOT" && cargo fetch -q)
SHIM=$(ls -d "${CARGO_HOME:-$HOME/.cargo}"/registry/src/*/"libqi-vibe-$VERSION" | head -1)/cpp/naoqi_shim.cpp
[ -f "$SHIM" ] || { echo "naoqi_shim.cpp of libqi-vibe $VERSION not found" >&2; exit 2; }

IMAGE=$("$HERE/image.sh")
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
docker run --rm --network none -u "$(id -u):$(id -g)" -v "$SDK:$SDK:ro" -v "$HERE:$HERE:ro" \
  -v "$(dirname "$SHIM"):$(dirname "$SHIM"):ro" -v "$WORK:$WORK" "$IMAGE" \
  "$HERE/check-abi.sh" --inside "$SDK" "$HERE/naoqi-2.1" "$SHIM" "$WORK"
