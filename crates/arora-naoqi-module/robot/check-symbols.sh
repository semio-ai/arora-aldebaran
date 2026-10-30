#!/bin/bash
# Checks what a library for a robot needs of it: the glibc symbols it uses are not newer than
# 2.12, and the unversioned symbols it leaves to the process are NAOqi's (AL::...), Boost's
# (boost::system, which NAOqi's libalcommon loads with Boost.Thread), weak, or in the libstdc++
# or libgcc_s of GCC 4.8 (whose newest version, GLIBCXX_3.4.19, NAOqi 2.1's must have).
#
# Usage: check-symbols.sh <library> <-m32|-m64>
set -euo pipefail
LIBRARY=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
M=$2
DIR=$(dirname "$LIBRARY")
docker run --rm --network none -v "$DIR:$DIR" arora-robot-gxx bash -c '
  set -euo pipefail
  library=$1; m=$2
  stdcxx=$(g++ $m -print-file-name=libstdc++.so)
  gcc_s=$(g++ $m -print-file-name=libgcc_s.so.1)
  newest=$(objdump -T "$library" | grep -o "GLIBC_[0-9.]*" | sort -uV | tail -1)
  echo "newest glibc symbol version: $newest"
  case "$newest" in GLIBC_2.1[3-9]*|GLIBC_2.[2-9][0-9]*) echo "too new for NAOqi 2.1" >&2; exit 1;; esac
  nm -D --defined-only "$(readlink -f "$stdcxx")" "$(readlink -f "$gcc_s")" |
    awk "NF == 3 {print \$3}" | sed "s/@.*//" | sort -u > /tmp/stdcxx
  missing=0
  while read -r symbol; do
    case "$symbol" in _ZN2AL*|_ZNK2AL*|_ZN5boost*|_ZNK5boost*) continue;; esac
    if ! grep -qxF "$symbol" /tmp/stdcxx; then
      echo "not in the libstdc++ or libgcc_s of GCC 4.8: $symbol $(echo "$symbol" | c++filt)" >&2
      missing=1
    fi
  done < <(objdump -T "$library" | awk "\$2 != \"w\" && /\*UND\*/ && !/GLIBC_/ {print \$NF}")
  [ "$missing" = 0 ] && echo "every C++ symbol it needs is in the libstdc++ or libgcc_s of GCC 4.8, NAOqi'"'"'s or its Boost"
' check "$LIBRARY" "$M"
