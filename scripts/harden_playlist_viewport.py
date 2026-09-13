from pathlib import Path

path = Path("src/workbench/mod.rs")
text = path.read_text()
old = "    let visible_items = usize::from(area.height.saturating_sub(2) / 2).max(1);\n"
new = "    let visible_items = usize::from(area.height.saturating_sub(2) / 2);\n"
assert old in text, "playlist visible-items anchor not found"
path.write_text(text.replace(old, new, 1))
