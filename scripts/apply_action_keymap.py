from pathlib import Path

path = Path("src/workbench/mod.rs")
text = path.read_text()

text = text.replace("mod git_context;\nmod model;", "mod git_context;\nmod input;\nmod model;", 1)
text = text.replace(
    "use editor::EditorState;\nuse model::{AppState, HitMap, LyricsState, PlaybackStatus, QueueItem, SearchState, View};",
    "use editor::EditorState;\nuse input::{Action, action_for_key};\nuse model::{AppState, HitMap, LyricsState, PlaybackStatus, QueueItem, SearchState, View};",
    1,
)

start = text.index("fn handle_key(")
end = text.index("fn handle_search_key(")
replacement = '''fn apply_action(
    workbench: &mut Workbench,
    action: Action,
    controls: &mpsc::UnboundedSender<Control>,
) -> bool {
    match action {
        Action::NextView => workbench.state.view = workbench.state.view.next(),
        Action::PreviousView => workbench.state.view = workbench.state.view.previous(),
        Action::SetView(view) => workbench.state.view = view,
        Action::CycleTheme => {
            workbench.state.theme = workbench.state.theme.next();
            workbench.state.message = format!("theme · {}", workbench.state.theme.name);
        }
        Action::Quit => {
            let _ = controls.send(Control::Quit);
            return true;
        }
        Action::TogglePlayback => {
            let _ = controls.send(Control::Toggle);
        }
        Action::NextTrack => {
            let _ = controls.send(Control::Next);
        }
        Action::PreviousTrack => {
            let _ = controls.send(Control::Previous);
        }
        Action::VolumeUp => adjust_volume(workbench, VOLUME_STEP_PERCENT, controls),
        Action::VolumeDown => adjust_volume(workbench, -VOLUME_STEP_PERCENT, controls),
        Action::SeekForward => seek_relative(workbench, SEEK_STEP_MS as i64, controls),
        Action::SeekBackward => seek_relative(workbench, -(SEEK_STEP_MS as i64), controls),
        Action::ToggleShuffle => {
            let enabled = !workbench.state.shuffle;
            let _ = controls.send(Control::Shuffle(enabled));
        }
        Action::ToggleRepeat => {
            let enabled = !workbench.state.repeat;
            let _ = controls.send(Control::Repeat(enabled));
        }
    }
    false
}

fn handle_key(
    workbench: &mut Workbench,
    key: KeyEvent,
    controls: &mpsc::UnboundedSender<Control>,
) -> Result<bool, String> {
    if workbench.state.view == View::Editor {
        return handle_editor_key(workbench, key, controls);
    }
    if workbench.state.view == View::Search {
        return handle_search_key(workbench, key, controls);
    }

    Ok(action_for_key(key)
        .map(|action| apply_action(workbench, action, controls))
        .unwrap_or(false))
}

'''
text = text[:start] + replacement + text[end:]
path.write_text(text)
