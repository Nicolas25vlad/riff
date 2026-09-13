use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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

pub fn action_for_key(key: KeyEvent) -> Option<Action> {
    if key.modifiers.contains(KeyModifiers::ALT) {
        return match key.code {
            KeyCode::Char('1') => Some(Action::SetView(View::NowPlaying)),
            KeyCode::Char('2') => Some(Action::SetView(View::Search)),
            KeyCode::Char('3') => Some(Action::SetView(View::Playlist)),
            KeyCode::Char('4') => Some(Action::SetView(View::Lyrics)),
            KeyCode::Char('5') => Some(Action::SetView(View::Editor)),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Tab => Some(Action::NextView),
        KeyCode::BackTab => Some(Action::PreviousView),
        KeyCode::F(6) => Some(Action::CycleTheme),
        KeyCode::Char('q') | KeyCode::Esc => Some(Action::Quit),
        KeyCode::Char(' ') => Some(Action::TogglePlayback),
        KeyCode::Char('n') | KeyCode::Char('l') | KeyCode::Right => Some(Action::NextTrack),
        KeyCode::Char('p') | KeyCode::Char('h') | KeyCode::Left => Some(Action::PreviousTrack),
        KeyCode::Char('+') | KeyCode::Char('=') => Some(Action::VolumeUp),
        KeyCode::Char('-') => Some(Action::VolumeDown),
        KeyCode::Char(']') => Some(Action::SeekForward),
        KeyCode::Char('[') => Some(Action::SeekBackward),
        KeyCode::Char('s') => Some(Action::ToggleShuffle),
        KeyCode::Char('r') => Some(Action::ToggleRepeat),
        KeyCode::Char('/') => Some(Action::SetView(View::Search)),
        KeyCode::Char('e') => Some(Action::SetView(View::Editor)),
        KeyCode::Char('y') => Some(Action::SetView(View::Lyrics)),
        _ => None,
    }
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
}
