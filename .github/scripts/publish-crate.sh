#!/usr/bin/env bash
# Publish one workspace crate to crates.io, treating "version already
# published" as success. The latter makes the workflow safely re-runnable
# when a previous run finished some crates and failed on a later one.

set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <crate-name>" >&2
  exit 64
fi

crate="$1"
# Read the version from Cargo metadata so this script doesn't need to
# know whether the crate uses workspace inheritance.
version=$(cargo metadata --no-deps --format-version 1 \
  | jq -r --arg name "$crate" '.packages[] | select(.name == $name) | .version')

if [ -z "$version" ] || [ "$version" = "null" ]; then
  echo "could not resolve version for crate '$crate'" >&2
  exit 1
fi

echo "::group::Publishing $crate v$version"

# Capture stderr so we can detect "already exists" without losing the
# real error on other failures.
tmp_err=$(mktemp)
trap 'rm -f "$tmp_err"' EXIT
if cargo publish -p "$crate" --locked 2>"$tmp_err"; then
  cat "$tmp_err"
  echo "published $crate v$version"
  echo "::endgroup::"
  exit 0
fi

cat "$tmp_err"
if grep -qE "already (uploaded|exists)|crate version .* is already uploaded" "$tmp_err"; then
  echo "$crate v$version is already on crates.io — treating as success."
  echo "::endgroup::"
  exit 0
fi

echo "::endgroup::"
exit 1
