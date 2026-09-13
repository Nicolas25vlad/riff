from pathlib import Path

input_path = Path("src/workbench/input.rs")
text = input_path.read_text()
old = '''pub fn global_status_hint() -> String {
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
'''
new = '''pub fn global_status_hint(compact: bool) -> String {
    if compact {
        format!(
            " {} play · {}/{} track · {}/{} vol · {} view · {} quit ",
            primary_hint(Action::TogglePlayback),
            primary_hint(Action::PreviousTrack),
            primary_hint(Action::NextTrack),
            primary_hint(Action::VolumeUp),
            primary_hint(Action::VolumeDown),
            primary_hint(Action::NextView),
            primary_hint(Action::Quit),
        )
    } else {
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
}
'''
assert old in text, "global status hint anchor not found"
text = text.replace(old, new, 1)
text = text.replace("let hint = global_status_hint();", "let hint = global_status_hint(false);", 1)
anchor = '''    #[test]
    fn every_footer_action_has_a_primary_binding() {
'''
insert = '''    #[test]
    fn compact_footer_uses_primary_bindings_and_is_shorter() {
        let compact = global_status_hint(true);
        let wide = global_status_hint(false);
        for expected in ["Space", "h/l", "+/-", "Tab", "q"] {
            assert!(compact.contains(expected), "missing compact hint {expected}: {compact}");
        }
        assert!(compact.len() < wide.len());
    }

'''
assert anchor in text, "input tests anchor not found"
text = text.replace(anchor, insert + anchor, 1)
input_path.write_text(text)

mod_path = Path("src/workbench/mod.rs")
text = mod_path.read_text()
text = text.replace("const SEEK_STEP_MS: u32 = 5_000;", "const SEEK_STEP_MS: u32 = 5_000;\nconst COMPACT_TRANSPORT_WIDTH: u16 = 72;", 1)

start = text.index("fn draw_transport(")
end = text.index("fn draw_status(")
replacement = '''#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportMode {
    Compact,
    Wide,
}

fn transport_mode(width: u16) -> TransportMode {
    if width < COMPACT_TRANSPORT_WIDTH {
        TransportMode::Compact
    } else {
        TransportMode::Wide
    }
}

fn draw_transport(frame: &mut Frame<'_>, area: Rect, workbench: &mut Workbench) {
    let theme = workbench.state.theme;
    let mode = transport_mode(area.width);
    let compact = mode == TransportMode::Compact;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(2)])
        .split(area);
    let constraints = if compact {
        [
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Min(7),
            Constraint::Length(10),
        ]
    } else {
        [
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Min(10),
            Constraint::Length(20),
        ]
    };
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(rows[0]);
    let previous_label = if compact { " ◀ " } else { " ◀ prev " };
    let toggle_label = match (compact, workbench.state.status == PlaybackStatus::Playing) {
        (true, true) => " Ⅱ ",
        (true, false) => " ▶ ",
        (false, true) => " Ⅱ pause ",
        (false, false) => " ▶ play ",
    };
    let next_label = if compact { " ▶ " } else { " next ▶ " };
    let previous = Paragraph::new(previous_label)
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.accent));
    let toggle = Paragraph::new(toggle_label)
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        );
    let next = Paragraph::new(next_label)
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.accent));
    frame.render_widget(previous, top[0]);
    frame.render_widget(toggle, top[1]);
    frame.render_widget(next, top[2]);
    workbench.state.hits.previous = Some(top[0]);
    workbench.state.hits.toggle = Some(top[1]);
    workbench.state.hits.next = Some(top[2]);

    let shuffle = if workbench.state.shuffle { "●" } else { "○" };
    let repeat = if workbench.state.repeat { "●" } else { "○" };
    let flags = if compact {
        format!("{shuffle} S  {repeat} R")
    } else {
        format!("{shuffle} shuffle   {repeat} repeat")
    };
    frame.render_widget(
        Paragraph::new(flags).style(Style::default().fg(theme.muted)),
        top[3],
    );
    let volume_percent = volume_percent(workbench.state.volume);
    let volume_ratio = volume_percent as f64 / 100.0;
    let volume_label = if compact {
        format!("{volume_percent}%")
    } else {
        format!("vol {volume_percent:>3}%")
    };
    frame.render_widget(
        Gauge::default().ratio(volume_ratio).label(volume_label),
        top[4],
    );
    workbench.state.hits.volume = Some(top[4]);

    let duration = workbench.state.duration_ms();
    let ratio = if duration == 0 {
        0.0
    } else {
        (workbench.state.position_ms as f64 / duration as f64).clamp(0.0, 1.0)
    };
    frame.render_widget(
        Gauge::default()
            .block(Block::default().borders(Borders::TOP))
            .ratio(ratio)
            .label(format!(
                "{} / {}",
                format_duration(workbench.state.position_ms),
                format_duration(duration)
            )),
        rows[1],
    );
    workbench.state.hits.progress = Some(rows[1]);
}

'''
text = text[:start] + replacement + text[end:]
text = text.replace("_ => global_status_hint(),", "_ => global_status_hint(area.width < COMPACT_TRANSPORT_WIDTH),", 1)

test_anchor = '''    #[test]
    fn formats_transport_time() {
'''
test_insert = '''    #[test]
    fn transport_mode_switches_at_compact_breakpoint() {
        assert_eq!(transport_mode(COMPACT_TRANSPORT_WIDTH - 1), TransportMode::Compact);
        assert_eq!(transport_mode(COMPACT_TRANSPORT_WIDTH), TransportMode::Wide);
    }

'''
assert test_anchor in text, "transport tests anchor not found"
text = text.replace(test_anchor, test_insert + test_anchor, 1)
mod_path.write_text(text)
