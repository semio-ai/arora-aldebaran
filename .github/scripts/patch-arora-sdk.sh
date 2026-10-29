#!/usr/bin/env bash
# Point this workspace's Arora dependencies at an arora-sdk checkout instead of
# crates.io: appends a [patch.crates-io] entry to Cargo.toml (to its [patch.crates-io],
# which must then be its last table) for every crate that
# arora-sdk's release workflow publishes, updates Cargo.lock, and fails when one of
# those crates still resolves from a registry (a patch applies only where its version
# meets the requirement: arora-sdk at arora-hal 5 does not replace an "arora-hal = 4").
# Usage: patch-arora-sdk.sh <arora-sdk dir>. Run cargo afterwards with
# CARGO_UNSTABLE_BINDEPS=true: arora-sdk's manifests declare artifact dependencies.
set -euo pipefail
export CARGO_UNSTABLE_BINDEPS=true
sdk=$(cd "$1" && pwd)
publishable=$(grep -o 'publishable=([^)]*)' "$sdk/.github/workflows/release.yml" \
  | sed 's/^publishable=(//; s/)$//')
header="[patch.crates-io]"
if grep -q '^\[patch\.crates-io\]' Cargo.toml; then
  if [ "$(grep '^\[' Cargo.toml | tail -1)" != "[patch.crates-io]" ]; then
    echo "Cargo.toml's [patch.crates-io] must be its last table, to be extended" >&2
    exit 1
  fi
  header=""
fi
{
  echo
  echo "# Added by .github/scripts/patch-arora-sdk.sh: the Arora crates from $sdk."
  [ -z "$header" ] || echo "$header"
  (cd "$sdk" && cargo metadata --no-deps --format-version 1) | python3 -c '
import json, os, sys
wanted = set(sys.argv[1].split())
for package in json.load(sys.stdin)["packages"]:
    if package["name"] in wanted:
        name, path = package["name"], os.path.dirname(package["manifest_path"])
        print("%s = { path = \"%s\" }" % (name, path))
' "$publishable"
} >> Cargo.toml
# The patched crates the lockfile holds are unlocked too: a patch at another version than
# the locked one (arora-sdk's main past a release) is otherwise left unused.
locked=$(sed -n 's/^name = "\(.*\)"$/\1/p' Cargo.lock | sort -u)
update=(--workspace)
for name in $publishable; do
  if grep -qxF "$name" <<< "$locked"; then
    update+=(-p "$name")
  fi
done
cargo update "${update[@]}"
cargo metadata --format-version 1 --all-features | python3 -c '
import json, sys
wanted = set(sys.argv[1].split())
outside = sorted(
    "%s %s (%s)" % (package["name"], package["version"], package["source"])
    for package in json.load(sys.stdin)["packages"]
    if package["name"] in wanted and package["source"] is not None
)
if outside:
    print("These arora-sdk crates resolve outside the checkout, so they are not tested:")
    for line in outside:
        print("  " + line)
    print("Raise the requirements in Cargo.toml to the versions of arora-sdk main.")
    sys.exit(1)
' "$publishable"
