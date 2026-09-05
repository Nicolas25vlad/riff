use crossterm::event::{KeyCode, KeyEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    NextView,
    PreviousView,
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
    OpenSearch,
    OpenEditor,
    OpenLyrics,
}

pub fn from_key(key: KeyEvent) -> Option<Action> {
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
        KeyCode::Char('/') => Some(Action::OpenSearch),
        KeyCode::Char('e') => Some(Action::OpenEditor),
        KeyCode::Char('y') => Some(Action::OpenLyrics),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    #[test]
    fn maps_transport_keys_to_actions() {
        assert_eq!(
            from_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)),
            Some(Action::TogglePlayback)
        );
        assert_eq!(
            from_key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE)),
            Some(Action::VolumeUp)
        );
        assert_eq!(
            from_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE)),
            Some(Action::OpenSearch)
        );
    }
}
