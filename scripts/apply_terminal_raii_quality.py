from pathlib import Path

path = Path("scripts/check-tui-quality.sh")
text = path.read_text()
anchor = '''if grep -RIn 'env_logger' src/workbench --include='*.rs'; then
  fail 'Workbench must not initialize env_logger directly.'
fi

'''
addition = '''if grep -RIn 'env_logger' src/workbench --include='*.rs'; then
  fail 'Workbench must not initialize env_logger directly.'
fi

if ! grep -Fq 'impl Drop for TerminalGuard' src/workbench/mod.rs; then
  fail 'Workbench terminal ownership must use a Drop-based RAII guard.'
fi

if ! grep -Fq 'TerminalGuard::enter()?' src/workbench/mod.rs; then
  fail 'Workbench must enter terminal mode through TerminalGuard.'
fi

if grep -Fq 'fn restore_terminal(' src/workbench/mod.rs; then
  fail 'Legacy manual-only terminal restoration must not return.'
fi

'''
assert anchor in text, "quality gate anchor not found"
path.write_text(text.replace(anchor, addition, 1))
