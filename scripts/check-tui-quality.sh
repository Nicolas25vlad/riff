#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "::error::$1"
  exit 1
}

# The Workbench owns the alternate screen. Direct terminal writes corrupt Ratatui frames.
if grep -RInE '\b(print|eprint)ln!\s*\(' src/workbench --include='*.rs'; then
  fail 'Workbench must not write directly to stdout/stderr while it owns the terminal.'
fi

if grep -RIn 'init_cli_logging' src/workbench --include='*.rs'; then
  fail 'CLI logging must never be initialized from the Workbench.'
fi

if grep -RIn 'env_logger' src/workbench --include='*.rs'; then
  fail 'Workbench must not initialize env_logger directly.'
fi

python - <<'PY'
from pathlib import Path

source = Path('src/workbench/mod.rs').read_text()
up = source.split('MouseEventKind::ScrollUp => {', 1)[1].split('MouseEventKind::ScrollDown => {', 1)[0]
down = source.split('MouseEventKind::ScrollDown => {', 1)[1].split('_ => {}', 1)[0]
for name, block in [('scroll up', up), ('scroll down', down)]:
    assert 'View::Search' in block, f'{name} must navigate Search results'
    assert '.volume' in block and 'contains(rect, point)' in block, f'{name} may change volume only when hovering the volume control'
PY

cargo test --bin riff workbench::tests -- --nocapture

echo 'TUI quality invariants passed.'
