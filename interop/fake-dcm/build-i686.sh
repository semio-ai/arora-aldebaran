#!/bin/bash
# Builds a 32-bit fake-dcm, in interop/fake-dcm/build/i686, to run the library a robot gets
# (crates/arora-naoqi-module/robot/build.sh) in a 32-bit process with the libstdc++ of GCC 4.8,
# in the image of the robot's compiler: fake-dcm-i686.sh runs it there. Its stub of the SDK
# also has the Boost.System NAOqi's libraries load (boost_system.cpp). Both are multithreaded,
# as NAOqi and its SDK are: Boost gives its signals another mutex otherwise. Run build.sh first:
# it builds the stub of the SDK, which the library must be built against for this harness
# (`NAOQI_SDK=interop/fake-dcm/build/sdk crates/arora-naoqi-module/robot/build.sh`), not
# against the declarations of NAOqi 2.1 (robot/naoqi-2.1/), whose ABI the stub's is not.
set -e
HERE=$(cd "$(dirname "$0")" && pwd)
ROBOT=$(cd "$HERE/../../crates/arora-naoqi-module/robot" && pwd)
OUT="$HERE/build/i686"
SDK="$HERE/build/sdk"
[ -f "$SDK/src/stub.cpp" ] || { echo "run build.sh first" >&2; exit 2; }
IMAGE=$("$ROBOT/image.sh")
mkdir -p "$OUT"
docker run --rm --network none -v "$HERE:$HERE" -w "$OUT" "$IMAGE" bash -c "
  set -e
  g++ -m32 -pthread -shared -fPIC -O1 -I$SDK/include $SDK/src/stub.cpp $HERE/boost_system.cpp -o libnaoqistub.so
  g++ -m32 -pthread -O1 -I$SDK/include $HERE/fake_dcm.cpp -o fake-dcm -L. -lnaoqistub -ldl -Wl,-rpath,$OUT
"
echo "built in $OUT"
