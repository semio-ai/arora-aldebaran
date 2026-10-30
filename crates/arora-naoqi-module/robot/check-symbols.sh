#!/bin/bash
# Checks what a library for a robot needs of it: the glibc symbols it uses are not newer than
# 2.12, and every unversioned symbol it leaves to the process is defined by the robot's
# libraries:
#
# - given the library directories of the robot (the `libnaoqi/lib` and the sysroot's `lib` of
#   its cross toolchain), by those: NAOqi's libraries, their Boost, the robot's libstdc++,
#   libgcc_s and glibc;
# - else, NAOqi's (AL::, qi::, their typeinfos and vtables) and Boost's symbols are assumed
#   there (check-abi.sh checks the bridge needs no more of them with naoqi-2.1/ than with the
#   SDK), and the others must be in the libstdc++ or libgcc_s of GCC 4.8 with a version the
#   robot's of NAOqi 2.1 has (GCC 4.5's: GLIBCXX_3.4.14, CXXABI_1.3.4, GCC_4.5.0).
#
# Usage: check-symbols.sh <library> <-m32|-m64> [<robot library directory>...]
set -euo pipefail
LIBRARY=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
M=$2
shift 2
DIRS=()
MOUNTS=(-v "$(dirname "$LIBRARY"):$(dirname "$LIBRARY")")
for dir in "$@"; do
  dir=$(cd "$dir" && pwd)
  DIRS+=("$dir")
  MOUNTS+=(-v "$dir:$dir")
done
docker run --rm --network none "${MOUNTS[@]}" "$("$(dirname "$0")/image.sh")" bash -c '
  set -euo pipefail
  library=$1; m=$2; shift 2
  newest=$(objdump -T "$library" | grep -o "GLIBC_[0-9.]*" | sort -uV | tail -1)
  echo "newest glibc symbol version: $newest"
  case "$newest" in GLIBC_2.1[3-9]*|GLIBC_2.[2-9][0-9]*) echo "too new for NAOqi 2.1" >&2; exit 1;; esac
  if [ $# -gt 0 ]; then
    where="the robot'"'"'s libraries"
    for dir in "$@"; do
      find "$dir" -maxdepth 1 -name "*.so*" -type f
    done | while read -r lib; do
      nm -D --defined-only "$lib" 2>/dev/null || true
    done | awk "NF == 3 {print \$3}" | sed "s/@.*//" | sort -u > /tmp/defined
    assumed="^$"
  else
    where="the libstdc++ or libgcc_s of NAOqi 2.1 (GCC 4.5)"
    # The symbols of GCC 4.8 that GCC 4.5 had: of a version no newer than the robot'"'"'s.
    nm -D --defined-only "$(readlink -f "$(g++ $m -print-file-name=libstdc++.so)")" \
      "$(readlink -f "$(g++ $m -print-file-name=libgcc_s.so.1)")" |
      awk "NF == 3 {print \$3}" | sed "s/@@*/ /" > /tmp/versioned
    for newest in GLIBCXX_3.4.14 CXXABI_1.3.4 GCC_4.5.0; do
      awk "{print \$2}" /tmp/versioned | grep "^${newest%%_*}_" | sort -uV |
        sed "/^$newest\$/q"
    done > /tmp/versions
    awk "NR == FNR {old[\$1]; next} \$2 in old {print \$1}" /tmp/versions /tmp/versioned |
      sort -u > /tmp/defined
    assumed="^_Z(T[ISV])?N?K?(2AL|2qi|5boost)"
  fi
  missing=0
  while read -r symbol; do
    if echo "$symbol" | grep -qE "$assumed"; then
      continue
    fi
    if ! grep -qxF "$symbol" /tmp/defined; then
      echo "not in $where: $symbol $(echo "$symbol" | c++filt)" >&2
      missing=1
    fi
  done < <(objdump -T "$library" | awk "\$2 != \"w\" && /\*UND\*/ && !/GLIBC_/ {print \$NF}")
  [ "$missing" = 0 ] && echo "every other symbol it needs is in $where"
' check "$LIBRARY" "$M" "${DIRS[@]}"
