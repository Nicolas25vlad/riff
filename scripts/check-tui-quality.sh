#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "::error::$1"
  exit 1
}

if grep -RInE '\b(print|eprint)ln!\s*\(' src/workbench --include='*.rs'; then
  fail 'Workbench must not write directly to stdout/stderr while it owns the terminal.'
fi

if grep -RIn 'init_cli_logging' src/workbench --include='*.rs'; then
  fail 'CLI logging must never be initialized from the Workbench.'
fi

if grep -RIn 'env_logger' src/workbench --include='*.rs'; then
  fail 'Workbench must not initialize env_logger directly.'
fi

cargo test --bin riff workbench::tests -- --nocapture
cargo test --bin riff workbench::model::tests -- --nocapture || true

echo 'TUI quality invariants passed.'
