#!/bin/bash
# Stamps a release version into Cargo.toml and Cargo.lock.
# Usage: scripts/set-version.sh <version>
set -euo pipefail
v="${1:?usage: set-version.sh <version>}"
cd "$(dirname "$0")/.."

# First `version = "…"` line in Cargo.toml (the package's).
perl -0pi -e 's/^version = "[^"]*"/version = "'"$v"'"/m' Cargo.toml
# The jira-tickets entry in Cargo.lock.
perl -0pi -e 's/(name = "jira-tickets"\nversion = ")[^"]*(")/${1}'"$v"'${2}/' Cargo.lock

grep -q "^version = \"$v\"" Cargo.toml
grep -A1 '^name = "jira-tickets"$' Cargo.lock | grep -q "^version = \"$v\""
