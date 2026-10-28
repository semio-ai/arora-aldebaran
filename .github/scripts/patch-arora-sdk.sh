#!/usr/bin/env bash
# Point this workspace's Arora dependencies at an arora-sdk checkout instead of
# crates.io: appends a [patch.crates-io] entry to Cargo.toml for every crate that
# arora-sdk's release workflow publishes. Usage: patch-arora-sdk.sh <arora-sdk dir>
set -euo pipefail
sdk=$(cd "$1" && pwd)
publishable=$(grep -o 'publishable=([^)]*)' "$sdk/.github/workflows/release.yml" \
  | sed 's/^publishable=(//; s/)$//')
{
  echo
  echo "# Added by .github/scripts/patch-arora-sdk.sh: the Arora crates from $sdk."
  echo "[patch.crates-io]"
  (cd "$sdk" && cargo metadata --no-deps --format-version 1) | python3 -c '
import json, os, sys
wanted = set(sys.argv[1].split())
for package in json.load(sys.stdin)["packages"]:
    if package["name"] in wanted:
        name, path = package["name"], os.path.dirname(package["manifest_path"])
        print("%s = { path = \"%s\" }" % (name, path))
' "$publishable"
} >> Cargo.toml
