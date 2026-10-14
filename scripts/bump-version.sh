#!/bin/sh
# Set the version everywhere it's written down: the server and domain crates (workspace),
# the desktop app (its own workspace), the web app, and their lock files.
#   scripts/bump-version.sh 1.3.0
set -eu
v=$1
cd "$(dirname "$0")/.."
sed -i "/^\[workspace.package\]/,/^\[/ s/^version = \".*\"/version = \"$v\"/" Cargo.toml
sed -i "/^\[package\]/,/^\[/ s/^version = \".*\"/version = \"$v\"/" crates/desktop/Cargo.toml
(cd web && npm version "$v" --no-git-tag-version --allow-same-version >/dev/null)
cargo update --workspace --offline -q
cargo update --workspace --offline -q --manifest-path crates/desktop/Cargo.toml
git diff --stat
