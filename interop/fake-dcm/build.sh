#!/bin/bash
# Builds what the end-to-end test of arora-naoqi-module (crates/arora-naoqi-module/tests/
# fake_dcm.rs) runs, in interop/fake-dcm/build:
#
# - sdk/: the stub of the NAOqi C++ SDK of libqi-rs-vibe (interop/naoqi-sdk-stub), its headers,
#   its source and libnaoqistub.so;
# - fake-dcm: a fake NAOqi process, linked with the stub, loading the library as NAOqi does and
#   running the loop of its DCM (fake_dcm.cpp);
# - libarora_naoqi.so: the library, with the bridge to the DCM built against the stub's headers
#   only, as for a robot: its NAOqi symbols resolve in the process that loads it.
#
# Variables: LIBQI_RS_VIBE, a checkout of libqi-rs-vibe (else the release of the libqi-vibe
# of Cargo.lock is cloned in build/); CXX; CARGO_FLAGS.
set -e
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
OUT="$HERE/build"
CXX=${CXX:-g++}
mkdir -p "$OUT"
if [ -z "$LIBQI_RS_VIBE" ]; then
  REV=v$(sed -n '/^name = "libqi-vibe"$/{n;s/^version = "\(.*\)"$/\1/p}' "$ROOT/Cargo.lock")
  LIBQI_RS_VIBE="$OUT/libqi-rs-vibe"
  if [ ! -d "$LIBQI_RS_VIBE/.git" ]; then
    git clone --quiet https://github.com/victorpaleologue/libqi-rs-vibe "$LIBQI_RS_VIBE"
  fi
  git -C "$LIBQI_RS_VIBE" checkout --quiet "$REV"
fi
STUB="$LIBQI_RS_VIBE/interop/naoqi-sdk-stub"
mkdir -p "$OUT/sdk/lib" "$OUT/sdk/include" "$OUT/sdk/src"
cp -r "$STUB/include/." "$OUT/sdk/include/"
cp "$STUB/stub.cpp" "$OUT/sdk/src/"
$CXX -shared -fPIC -O1 -I"$STUB/include" "$STUB/stub.cpp" -o "$OUT/sdk/lib/libnaoqistub.so"
$CXX -O1 -I"$STUB/include" "$HERE/fake_dcm.cpp" -o "$OUT/fake-dcm" \
  -L"$OUT/sdk/lib" -lnaoqistub -ldl -Wl,-rpath,"$OUT/sdk/lib"
(cd "$ROOT" && QI_MODULE_NAOQI_SDK="$OUT/sdk" QI_MODULE_NAOQI_LIBS="" \
  cargo build -p arora-naoqi-module --features bridge $CARGO_FLAGS)
TARGET=$(cd "$ROOT" && cargo metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
cp "$TARGET/debug/libarora_naoqi.so" "$OUT/libarora_naoqi.so"
echo "built in $OUT"
