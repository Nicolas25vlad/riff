#!/usr/bin/env bash
set -euo pipefail

workflow=.github/workflows/release.yml

test -f "$workflow"
grep -Fq "tags:" "$workflow"
grep -Fq "Validate tag matches Cargo.toml" "$workflow"
grep -Fq "cargo build --release" "$workflow"
grep -Fq "gh release create" "$workflow"
grep -Fq -- "--verify-tag" "$workflow"

echo "Release workflow contract looks sane."
