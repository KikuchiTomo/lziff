#!/usr/bin/env bash
# Wait for the just-published version of <crate> to appear in the
# crates.io index. The next `cargo publish` will fail to resolve its
# path-then-version dependency until the index updates, which usually
# takes 5–60 seconds. We poll the public API rather than the sparse
# index because the API returns immediately once the new version is
# live for `cargo publish`'s purposes.

set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <crate-name>" >&2
  exit 64
fi

crate="$1"
# Discover the version from local Cargo metadata. The metadata graph
# uses the *published* package name (set by `package =` on the workspace
# dependency, when present), which is what we want here.
version=$(cargo metadata --no-deps --format-version 1 \
  | jq -r --arg name "$crate" '.packages[] | select(.name == $name) | .version')

if [ -z "$version" ] || [ "$version" = "null" ]; then
  echo "could not resolve version for crate '$crate'" >&2
  exit 1
fi

echo "Waiting for $crate v$version on crates.io…"

# 90 attempts × 5 s ≈ 7.5 min budget. The first publish of a brand-new
# crate sometimes takes longer than a republish; if we ever blow past
# this, the next step's cargo publish will fail with a clear error and
# the workflow can be re-run.
attempts=90
sleep_s=5
for i in $(seq 1 "$attempts"); do
  if curl --fail --silent --show-error \
       "https://crates.io/api/v1/crates/${crate}/${version}" \
       -o /dev/null; then
    echo "$crate v$version is live (attempt $i)."
    exit 0
  fi
  sleep "$sleep_s"
done

echo "::error::$crate v$version did not appear on crates.io within $((attempts * sleep_s))s." >&2
exit 1
