from pathlib import Path

input_path = Path("src/workbench/input.rs")
input_path.write_text(r'''use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::model::View;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    NextView,
    PreviousView,
    SetView(View),
    CycleTheme,
    Quit,
    TogglePlayback,
    NextTrack,
    PreviousTrack,
    VolumeUp,
    VolumeDown,
    SeekForward,
    SeekBackward,
    ToggleShuffle,
    ToggleRepeat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindingKey {
    Code(KeyCode),
    AltChar(char),
}

#[derive(Debug, Clone, Copy)]
struct Binding {
    key: BindingKey,
    action: Action,
    display: &'static str,
    primary: bool,
}

const BINDINGS: &[Binding] = &[
    Binding { key: BindingKey::Code(KeyCode::Tab), action: Action::NextView, display: "Tab", primary: true },
    Binding { key: BindingKey::Code(KeyCode::BackTab), action: Action::PreviousView, display: "Shift+Tab", primary: true },
    Binding { key: BindingKey::Code(KeyCode::F(6)), action: Action::CycleTheme, display: "F6", primary: true },
    Binding { key: BindingKey::Code(KeyCode::Char('q')), action: Action::Quit, display: "q", primary: true },
    Binding { key: BindingKey::Code(KeyCode::Esc), action: Action::Quit, display: "Esc", primary: false },
    Binding { key: BindingKey::Code(KeyCode::Char(' ')), action: Action::TogglePlayback, display: "Space", primary: true },
    Binding { key: BindingKey::Code(KeyCode::Char('n')), action: Action::NextTrack, display: "n", primary: false },
    Binding { key: BindingKey::Code(KeyCode::Char('l')), action: Action::NextTrack, display: "l", primary: true },
    Binding { key: BindingKey::Code(KeyCode::Right), action: Action::NextTrack, display: "→", primary: false },
    Binding { key: BindingKey::Code(KeyCode::Char('p')), action: Action::PreviousTrack, display: "p", primary: false },
    Binding { key: BindingKey::Code(KeyCode::Char('h')), action: Action::PreviousTrack, display: "h", primary: true },
    Binding { key: BindingKey::Code(KeyCode::Left), action: Action::PreviousTrack, display: "←", primary: false },
    Binding { key: BindingKey::Code(KeyCode::Char('+')), action: Action::VolumeUp, display: "+", primary: true },
    Binding { key: BindingKey::Code(KeyCode::Char('=')), action: Action::VolumeUp, display: "=", primary: false },
    Binding { key: BindingKey::Code(KeyCode::Char('-')), action: Action::VolumeDown, display: "-", primary: true },
    Binding { key: BindingKey::Code(KeyCode::Char(']')), action: Action::SeekForward, display: "]", primary: true },
    Binding { key: BindingKey::Code(KeyCode::Char('[')), action: Action::SeekBackward, display: "[", primary: true },
    Binding { key: BindingKey::Code(KeyCode::Char('s')), action: Action::ToggleShuffle, display: "s", primary: true },
    Binding { key: BindingKey::Code(KeyCode::Char('r')), action: Action::ToggleRepeat, display: "r", primary: true },
    Binding { key: BindingKey::Code(KeyCode::Char('/')), action: Action::SetView(View::Search), display: "/", primary: true },
    Binding { key: BindingKey::Code(KeyCode::Char('e')), action: Action::SetView(View::Editor), display: "e", primary: true },
    Binding { key: BindingKey::Code(KeyCode::Char('y')), action: Action::SetView(View::Lyrics), display: "y", primary: true },
    Binding { key: BindingKey::AltChar('1'), action: Action::SetView(View::NowPlaying), display: "Alt+1", primary: true },
    Binding { key: BindingKey::AltChar('2'), action: Action::SetView(View::Search), display: "Alt+2", primary: false },
    Binding { key: BindingKey::AltChar('3'), action: Action::SetView(View::Playlist), display: "Alt+3", primary: true },
    Binding { key: BindingKey::AltChar('4'), action: Action::SetView(View::Lyrics), display: "Alt+4", primary: false },
    Binding { key: BindingKey::AltChar('5'), action: Action::SetView(View::Editor), display: "Alt+5", primary: false },
];

fn binding_matches(binding: BindingKey, key: KeyEvent) -> bool {
    match binding {
        BindingKey::AltChar(ch) => {
            key.modifiers.contains(KeyModifiers::ALT) && key.code == KeyCode::Char(ch)
        }
        BindingKey::Code(code) => {
            !key.modifiers.contains(KeyModifiers::ALT) && key.code == code
        }
    }
}

pub fn action_for_key(key: KeyEvent) -> Option<Action> {
    BINDINGS
        .iter()
        .find(|binding| binding_matches(binding.key, key))
        .map(|binding| binding.action)
}

fn primary_hint(action: Action) -> &'static str {
    BINDINGS
        .iter()
        .find(|binding| binding.action == action && binding.primary)
        .map(|binding| binding.display)
        .unwrap_or("?")
}

pub fn global_status_hint() -> String {
    format!(
        " {} views · {} play/pause · {}/{} prev/next · {}/{} volume · {}/{} seek · {} shuffle · {} repeat · {} theme · {} quit ",
        primary_hint(Action::NextView),
        primary_hint(Action::TogglePlayback),
        primary_hint(Action::PreviousTrack),
        primary_hint(Action::NextTrack),
        primary_hint(Action::VolumeUp),
        primary_hint(Action::VolumeDown),
        primary_hint(Action::SeekBackward),
        primary_hint(Action::SeekForward),
        primary_hint(Action::ToggleShuffle),
        primary_hint(Action::ToggleRepeat),
        primary_hint(Action::CycleTheme),
        primary_hint(Action::Quit),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn transport_aliases_map_to_actions() {
        assert_eq!(action_for_key(key(KeyCode::Char(' '))), Some(Action::TogglePlayback));
        assert_eq!(action_for_key(key(KeyCode::Char('n'))), Some(Action::NextTrack));
        assert_eq!(action_for_key(key(KeyCode::Char('l'))), Some(Action::NextTrack));
        assert_eq!(action_for_key(key(KeyCode::Right)), Some(Action::NextTrack));
        assert_eq!(action_for_key(key(KeyCode::Char('p'))), Some(Action::PreviousTrack));
        assert_eq!(action_for_key(key(KeyCode::Char('h'))), Some(Action::PreviousTrack));
        assert_eq!(action_for_key(key(KeyCode::Left)), Some(Action::PreviousTrack));
    }

    #[test]
    fn navigation_and_view_shortcuts_are_actions() {
        assert_eq!(action_for_key(key(KeyCode::Tab)), Some(Action::NextView));
        assert_eq!(action_for_key(key(KeyCode::BackTab)), Some(Action::PreviousView));
        assert_eq!(action_for_key(key(KeyCode::Char('/'))), Some(Action::SetView(View::Search)));
        assert_eq!(
            action_for_key(KeyEvent::new(KeyCode::Char('4'), KeyModifiers::ALT)),
            Some(Action::SetView(View::Lyrics))
        );
    }

    #[test]
    fn playback_adjustments_are_action_based() {
        assert_eq!(action_for_key(key(KeyCode::Char('+'))), Some(Action::VolumeUp));
        assert_eq!(action_for_key(key(KeyCode::Char('-'))), Some(Action::VolumeDown));
        assert_eq!(action_for_key(key(KeyCode::Char(']'))), Some(Action::SeekForward));
        assert_eq!(action_for_key(key(KeyCode::Char('['))), Some(Action::SeekBackward));
        assert_eq!(action_for_key(key(KeyCode::Char('s'))), Some(Action::ToggleShuffle));
        assert_eq!(action_for_key(key(KeyCode::Char('r'))), Some(Action::ToggleRepeat));
    }

    #[test]
    fn unknown_key_has_no_global_action() {
        assert_eq!(action_for_key(key(KeyCode::Char('z'))), None);
    }

    #[test]
    fn footer_is_derived_from_primary_bindings() {
        let hint = global_status_hint();
        for expected in ["Tab", "Space", "h/l", "+/-", "[/]", "s", "r", "F6", "q"] {
            assert!(hint.contains(expected), "missing footer hint {expected}: {hint}");
        }
    }

    #[test]
    fn every_footer_action_has_a_primary_binding() {
        for action in [
            Action::NextView,
            Action::TogglePlayback,
            Action::PreviousTrack,
            Action::NextTrack,
            Action::VolumeUp,
            Action::VolumeDown,
            Action::SeekBackward,
            Action::SeekForward,
            Action::ToggleShuffle,
            Action::ToggleRepeat,
            Action::CycleTheme,
            Action::Quit,
        ] {
            assert_ne!(primary_hint(action), "?");
        }
    }
}
''')

mod_path = Path("src/workbench/mod.rs")
text = mod_path.read_text()
text = text.replace(
    "use input::{Action, action_for_key};",
    "use input::{Action, action_for_key, global_status_hint};",
    1,
)
old = '''    let hint = match workbench.state.view {
        View::Search => " Enter search/play · ↑↓ select · Ctrl+A add · Ctrl+P play · Esc back ",
        View::Editor => " Ctrl+S save · Ctrl+K/U cut/paste · Ctrl+G help · Ctrl+X leave ",
        _ => {
            " Tab views · Space play/pause · h/l prev/next · +/- volume · [/] seek · s shuffle · r repeat · F6 theme · q quit "
        }
    };
    frame.render_widget(
        Paragraph::new(hint)
'''
new = '''    let hint = match workbench.state.view {
        View::Search => " Enter search/play · ↑↓ select · Ctrl+A add · Ctrl+P play · Esc back ".to_string(),
        View::Editor => " Ctrl+S save · Ctrl+K/U cut/paste · Ctrl+G help · Ctrl+X leave ".to_string(),
        _ => global_status_hint(),
    };
    frame.render_widget(
        Paragraph::new(hint)
'''
assert old in text, "draw_status hint anchor not found"
mod_path.write_text(text.replace(old, new, 1))
