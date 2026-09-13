from pathlib import Path

path = Path("src/workbench/mod.rs")
text = path.read_text()

old_import = '''use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};'''
new_import = '''use crossterm::{
    cursor::Show,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};'''
assert old_import in text, "crossterm import anchor not found"
text = text.replace(old_import, new_import, 1)

workbench_anchor = '''struct Workbench {
    state: AppState,
    editor: EditorState,
    artwork: HashMap<String, RenderedArtwork>,
    artwork_pending: HashMap<String, bool>,
    pending_volume: Option<u16>,
    last_volume_send: Instant,
}
'''
terminal_guard = '''struct Workbench {
    state: AppState,
    editor: EditorState,
    artwork: HashMap<String, RenderedArtwork>,
    artwork_pending: HashMap<String, bool>,
    pending_volume: Option<u16>,
    last_volume_send: Instant,
}

#[derive(Debug, Default)]
struct TerminalGuard {
    raw_mode: bool,
    alternate_screen: bool,
    mouse_capture: bool,
}

impl TerminalGuard {
    fn enter() -> Result<Self, String> {
        let mut guard = Self::default();

        enable_raw_mode().map_err(|error| format!("could not enable terminal raw mode: {error}"))?;
        guard.raw_mode = true;

        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)
            .map_err(|error| format!("could not enter alternate screen: {error}"))?;
        guard.alternate_screen = true;

        execute!(stdout, EnableMouseCapture)
            .map_err(|error| format!("could not enable mouse capture: {error}"))?;
        guard.mouse_capture = true;

        Ok(guard)
    }

    fn restore(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        let mut stdout = io::stdout();

        if self.mouse_capture {
            match execute!(stdout, DisableMouseCapture) {
                Ok(()) => self.mouse_capture = false,
                Err(error) => errors.push(format!("could not disable mouse capture: {error}")),
            }
        }

        if self.alternate_screen {
            match execute!(stdout, LeaveAlternateScreen) {
                Ok(()) => self.alternate_screen = false,
                Err(error) => errors.push(format!("could not leave alternate screen: {error}")),
            }
        }

        if let Err(error) = execute!(stdout, Show) {
            errors.push(format!("could not restore cursor: {error}"));
        }

        if self.raw_mode {
            match disable_raw_mode() {
                Ok(()) => self.raw_mode = false,
                Err(error) => errors.push(format!("could not restore terminal mode: {error}")),
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
'''
assert workbench_anchor in text, "workbench struct anchor not found"
text = text.replace(workbench_anchor, terminal_guard, 1)

old_start = '''    enable_raw_mode().map_err(|error| format!("could not enable terminal raw mode: {error}"))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .map_err(|error| format!("could not enter alternate screen: {error}"))?;

    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    let backend = CrosstermBackend::new(stdout);
'''
new_start = '''    let mut terminal_guard = TerminalGuard::enter()?;
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    let backend = CrosstermBackend::new(io::stdout());
'''
assert old_start in text, "run_terminal setup anchor not found"
text = text.replace(old_start, new_start, 1)

old_end = '''    let restore = restore_terminal(&mut terminal);
    loop_result.and(restore)
}
'''
new_end = '''    drop(terminal);
    let restore = terminal_guard.restore();
    loop_result.and(restore)
}
'''
assert old_end in text, "run_terminal restore anchor not found"
text = text.replace(old_end, new_end, 1)

old_restore = '''fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<(), String> {
    disable_raw_mode().map_err(|error| format!("could not restore terminal mode: {error}"))?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )
    .map_err(|error| format!("could not leave alternate screen: {error}"))?;
    terminal
        .show_cursor()
        .map_err(|error| format!("could not restore cursor: {error}"))
}
'''
assert old_restore in text, "legacy restore_terminal function not found"
text = text.replace(old_restore, "", 1)

path.write_text(text)
