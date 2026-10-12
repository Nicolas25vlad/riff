mod editor;
mod git_context;
mod input;
mod model;
mod player_task;
mod theme;

use std::{
    collections::HashMap,
    fs,
    future::Future,
    io,
    io::Write,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use crossterm::{
    cursor::Show,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use image::DynamicImage;
use ratatui::{
    Frame, Terminal,
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect, Size},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, LineGauge, List, ListItem, Padding, Paragraph, Wrap},
};
use ratatui_image::{Image as TerminalImage, Resize, picker::Picker, protocol::Protocol};
use riff::Playlist;
use tokio::sync::mpsc;

use editor::EditorState;
use input::{Action, action_for_key, global_status_hint};
use model::{AppState, HitMap, LyricsState, PlaybackStatus, QueueItem, SearchState, View};
use player_task::{Control, PlayerUpdate};
use theme::Theme;

const WIDE_ART_SIZE: Size = Size::new(28, 14);
const COMPACT_ART_SIZE: Size = Size::new(18, 9);
const SEARCH_ART_SIZE: Size = Size::new(22, 11);
const VOLUME_MAX: u16 = u16::MAX;
const VOLUME_STEP_PERCENT: i16 = 5;
const VOLUME_SEND_INTERVAL: Duration = Duration::from_millis(60);
const SEEK_STEP_MS: u32 = 5_000;
const COMPACT_TRANSPORT_WIDTH: u16 = 72;

struct RenderedArtwork {
    wide: Protocol,
    compact: Protocol,
    search: Protocol,
}

struct Workbench {
    state: AppState,
    editor: EditorState,
    artwork: HashMap<String, RenderedArtwork>,
    artwork_pending: HashMap<String, bool>,
    pending_volume: Option<u16>,
    last_volume_send: Instant,
    command: Option<String>,
    command_suggestion: usize,
    command_error: Option<String>,
    visualizer: [u8; 32],
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

        enable_raw_mode()
            .map_err(|error| format!("could not enable terminal raw mode: {error}"))?;
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

pub async fn run(file_path: PathBuf, playlist: Playlist) -> Result<(), String> {
    if playlist.tracks.is_empty() {
        return Err("playlist has no tracks to play".to_string());
    }

    let editor = EditorState::load(&file_path)?;
    let file_name = file_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| file_path.display().to_string());
    let theme = Theme::from_env();
    player_task::prepare_authentication().await?;

    let mut terminal_guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)
        .map_err(|error| format!("could not initialize terminal UI: {error}"))?;
    terminal
        .clear()
        .map_err(|error| format!("could not clear terminal: {error}"))?;

    let queue = resolve_queue_with_startup(
        &mut terminal,
        &playlist.name,
        theme,
        player_task::resolve_queue(&playlist),
        poll_startup_cancel,
    )
    .await?;
    let auto_radio = playlist.radio || playlist.tracks.len() == 1;

    let state = AppState {
        file_path: file_path.clone(),
        file_name,
        playlist_name: playlist.name,
        queue: queue.clone(),
        transient_current: None,
        status: PlaybackStatus::Starting,
        current_uri: None,
        position_ms: 0,
        volume: VOLUME_MAX,
        shuffle: false,
        repeat: false,
        message: "Connecting to Spotify…".into(),
        view: View::NowPlaying,
        theme,
        git: git_context::detect(&file_path),
        search: SearchState::default(),
        lyrics: LyricsState::default(),
        hits: HitMap::default(),
    };

    let (control_tx, control_rx) = mpsc::unbounded_channel();
    let (update_tx, update_rx) = mpsc::unbounded_channel();
    let player_task = tokio::spawn(async move {
        if let Err(error) =
            player_task::run_player(queue, auto_radio, control_rx, update_tx.clone()).await
        {
            let _ = update_tx.send(PlayerUpdate::Error(error));
        }
    });

    let mut workbench = Workbench {
        state,
        editor,
        artwork: HashMap::new(),
        artwork_pending: HashMap::new(),
        pending_volume: None,
        last_volume_send: Instant::now() - VOLUME_SEND_INTERVAL,
        command: None,
        command_suggestion: 0,
        command_error: None,

        visualizer: [0; 32],
    };
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    let ui_result = run_terminal_loop(
        &mut terminal,
        &mut workbench,
        &control_tx,
        update_rx,
        &picker,
    )
    .await;

    let _ = control_tx.send(Control::Quit);
    drop(control_tx);
    drop(terminal);
    let restore = terminal_guard.restore();
    let _ = player_task.await;
    ui_result.and(restore)
}

async fn resolve_queue_with_startup<B, F, C>(
    terminal: &mut Terminal<B>,
    playlist_name: &str,
    theme: Theme,
    resolver: F,
    mut poll_cancel: C,
) -> Result<Vec<QueueItem>, String>
where
    B: Backend,
    F: Future<Output = Result<Vec<QueueItem>, String>>,
    C: FnMut() -> Result<bool, String>,
{
    let mut resolver = Box::pin(resolver);
    let mut startup_drawn = false;
    loop {
        tokio::select! {
            result = &mut resolver => {
                return match result {
                    Ok(queue) => Ok(queue),
                    Err(error) => {
                        terminal
                            .draw(|frame| draw_startup_error(frame, playlist_name, theme, &error))
                            .map_err(|draw_error| format!("{error}; could not render startup error: {draw_error}"))?;
                        wait_for_startup_ack()?;
                        Err(error)
                    }
                };
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                if !startup_drawn {
                    terminal
                        .draw(|frame| draw_startup(frame, playlist_name, theme))
                        .map_err(|error| format!("could not render startup state: {error}"))?;
                    startup_drawn = true;
                }
            }
        }

        if !startup_drawn {
            continue;
        }
        if startup_drawn && poll_cancel()? {
            return Err("Workbench startup cancelled".to_string());
        }
    }
}

fn poll_startup_cancel() -> Result<bool, String> {
    if !event::poll(Duration::ZERO)
        .map_err(|error| format!("could not poll startup input: {error}"))?
    {
        return Ok(false);
    }
    match event::read().map_err(|error| format!("could not read startup input: {error}"))? {
        Event::Key(key)
            if key.kind == KeyEventKind::Press
                && matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) =>
        {
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn wait_for_startup_ack() -> Result<(), String> {
    loop {
        if event::poll(Duration::from_millis(100))
            .map_err(|error| format!("could not wait for startup error acknowledgement: {error}"))?
            && let Event::Key(key) = event::read()
                .map_err(|error| format!("could not read startup error acknowledgement: {error}"))?
            && key.kind == KeyEventKind::Press
        {
            return Ok(());
        }
    }
}

fn draw_startup(frame: &mut Frame<'_>, playlist_name: &str, theme: Theme) {
    draw_startup_panel(
        frame,
        theme,
        "Preparing playlist",
        playlist_name,
        "Loading track metadata",
        "q / Esc cancels",
        false,
    );
}

fn draw_startup_error(frame: &mut Frame<'_>, playlist_name: &str, theme: Theme, error: &str) {
    draw_startup_panel(
        frame,
        theme,
        "Startup failed",
        playlist_name,
        error,
        "Press any key to return to the terminal",
        true,
    );
}

fn draw_startup_panel(
    frame: &mut Frame<'_>,
    theme: Theme,
    title: &str,
    playlist_name: &str,
    detail: &str,
    hint: &str,
    danger: bool,
) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background).fg(theme.foreground)),
        area,
    );

    let width = area.width.min(64);
    let height = area.height.min(9);
    let panel = Rect::new(
        (area.width - width) / 2,
        (area.height - height) / 2,
        width,
        height,
    );
    let title_color = if danger { theme.danger } else { theme.accent };
    let block = Block::default()
        .title(" RIFF ")
        .title_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.background).fg(theme.foreground))
        .padding(Padding::uniform(1));
    let inner = block.inner(panel);
    let mut lines = vec![
        Line::from(Span::styled(
            title,
            Style::default()
                .fg(title_color)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            playlist_name,
            Style::default().fg(theme.foreground),
        )),
        Line::from(Span::styled(detail, Style::default().fg(theme.muted))),
    ];
    if !hint.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            hint,
            Style::default().fg(theme.muted),
        )));
    }

    frame.render_widget(block, panel);
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        inner,
    );
}

async fn run_terminal_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    workbench: &mut Workbench,
    controls: &mpsc::UnboundedSender<Control>,
    mut updates: mpsc::UnboundedReceiver<PlayerUpdate>,
    picker: &Picker,
) -> Result<(), String> {
    let (art_tx, mut art_rx) = mpsc::unbounded_channel::<(String, RenderedArtwork)>();
    request_current_artwork(workbench, controls);

    loop {
        while let Ok(update) = updates.try_recv() {
            apply_player_update(workbench, update, picker, art_tx.clone(), controls)?;
        }
        while let Ok((key, artwork)) = art_rx.try_recv() {
            workbench.artwork.insert(key.clone(), artwork);
            workbench.artwork_pending.remove(&key);
        }
        flush_pending_volume(workbench, controls);

        terminal
            .draw(|frame| draw(frame, workbench))
            .map_err(|error| format!("could not render Riff Workbench: {error}"))?;

        if event::poll(Duration::from_millis(16))
            .map_err(|error| format!("could not poll terminal input: {error}"))?
        {
            match event::read()
                .map_err(|error| format!("could not read terminal input: {error}"))?
            {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if handle_key(workbench, key, controls)? {
                        break;
                    }
                }
                Event::Mouse(mouse) => handle_mouse(workbench, mouse, controls)?,
                _ => {}
            }
        }
    }
    Ok(())
}

fn apply_player_update(
    workbench: &mut Workbench,
    update: PlayerUpdate,
    picker: &Picker,
    art_tx: mpsc::UnboundedSender<(String, RenderedArtwork)>,
    controls: &mpsc::UnboundedSender<Control>,
) -> Result<(), String> {
    match update {
        PlayerUpdate::Status(status) => workbench.state.status = status,
        PlayerUpdate::Track { uri, position_ms } => {
            let changed = workbench.state.current_uri.as_deref() != Some(uri.as_str());
            workbench.state.current_uri = Some(uri.clone());
            workbench.state.position_ms = position_ms;
            if workbench.state.queue.iter().any(|item| item.uri == uri) {
                workbench.state.transient_current = None;
            }
            if changed {
                workbench.state.lyrics = LyricsState {
                    track_uri: Some(uri.clone()),
                    loading: true,
                    ..LyricsState::default()
                };
                request_current_artwork(workbench, controls);
            }
        }
        PlayerUpdate::Position { uri, position_ms } => {
            workbench.state.current_uri = Some(uri);
            workbench.state.position_ms = position_ms;
        }
        PlayerUpdate::Visualizer(levels) => workbench.visualizer = levels,
        PlayerUpdate::Volume(volume) => workbench.state.volume = volume,
        PlayerUpdate::Shuffle(shuffle) => workbench.state.shuffle = shuffle,
        PlayerUpdate::Repeat(repeat) => workbench.state.repeat = repeat,
        PlayerUpdate::Artwork { key, image } => {
            if workbench.artwork.contains_key(&key) || workbench.artwork_pending.contains_key(&key)
            {
                return Ok(());
            }
            workbench.artwork_pending.insert(key.clone(), true);
            let picker = picker.clone();
            tokio::task::spawn_blocking(move || render_artwork(&picker, key, image, art_tx));
        }
        PlayerUpdate::SearchResults { query, results } => {
            if query == workbench.state.search.submitted_query {
                workbench.state.search.searching = false;
                workbench.state.search.error = None;
                workbench.state.search.results = results;
                workbench.state.search.selected = 0;
                request_selected_search_artwork(workbench, controls);
            }
        }
        PlayerUpdate::SearchError { query, error } => {
            if query == workbench.state.search.submitted_query {
                workbench.state.search.searching = false;
                workbench.state.search.error = Some(error);
            }
        }
        PlayerUpdate::Lyrics {
            uri,
            provider,
            lines,
        } => {
            if workbench.state.current_uri.as_deref() == Some(uri.as_str()) {
                workbench.state.lyrics.track_uri = Some(uri);
                workbench.state.lyrics.provider = Some(provider);
                workbench.state.lyrics.lines = lines;
                workbench.state.lyrics.loading = false;
                workbench.state.lyrics.error = None;
            }
        }
        PlayerUpdate::LyricsError { uri, error } => {
            if workbench.state.current_uri.as_deref() == Some(uri.as_str()) {
                workbench.state.lyrics.loading = false;
                workbench.state.lyrics.error = Some(error);
            }
        }
        PlayerUpdate::Error(error) => return Err(error),
    }
    Ok(())
}

fn render_artwork(
    picker: &Picker,
    key: String,
    image: Arc<DynamicImage>,
    tx: mpsc::UnboundedSender<(String, RenderedArtwork)>,
) {
    let wide = picker.new_protocol((*image).clone(), WIDE_ART_SIZE, Resize::Fit(None));
    let compact = picker.new_protocol((*image).clone(), COMPACT_ART_SIZE, Resize::Fit(None));
    let search = picker.new_protocol((*image).clone(), SEARCH_ART_SIZE, Resize::Fit(None));
    if let (Ok(wide), Ok(compact), Ok(search)) = (wide, compact, search) {
        let _ = tx.send((
            key,
            RenderedArtwork {
                wide,
                compact,
                search,
            },
        ));
    }
}

fn apply_action(
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
    if workbench.command.is_some() {
        return handle_command_key(workbench, key, controls);
    }
    if workbench.state.view != View::Editor
        && key.code == KeyCode::Char(':')
        && !key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
    {
        workbench.command = Some(String::new());
        workbench.command_error = None;
        workbench.command_suggestion = 0;
        return Ok(false);
    }
    if workbench.state.view == View::Editor {
        return handle_editor_key(workbench, key, controls);
    }
    if workbench.state.view == View::Search {
        return handle_search_key(workbench, key, controls);
    }
    if workbench.state.view == View::Lyrics {
        return handle_lyrics_key(workbench, key, controls);
    }

    Ok(action_for_key(key)
        .map(|action| apply_action(workbench, action, controls))
        .unwrap_or(false))
}
fn handle_command_key(
    workbench: &mut Workbench,
    key: KeyEvent,
    controls: &mpsc::UnboundedSender<Control>,
) -> Result<bool, String> {
    match key.code {
        KeyCode::Esc => {
            workbench.command = None;
            workbench.command_error = None;
        }
        KeyCode::Enter => {
            let input = workbench.command.take().unwrap_or_default();
            let suggestions = command_suggestions(&input);
            let command = if input.trim().is_empty() && workbench.command_suggestion == 0 {
                String::new()
            } else if let Some(suggestion) = suggestions.get(workbench.command_suggestion) {
                (*suggestion).to_string()
            } else {
                input.trim().to_ascii_lowercase()
            };
            workbench.command_error = None;
            match command.as_str() {
                "" => {}
                "q" | "quit" => return Ok(apply_action(workbench, Action::Quit, controls)),
                name => match view_for_command(name) {
                    Some(view) => {
                        apply_action(workbench, Action::SetView(view), controls);
                    }
                    None => {
                        workbench.command_error = Some(format!("unknown command: {name}"));
                    }
                },
            }
        }
        KeyCode::Up | KeyCode::Down => {
            let suggestions = command_suggestions(workbench.command.as_deref().unwrap_or_default());
            if !suggestions.is_empty() {
                workbench.command_suggestion = match key.code {
                    KeyCode::Up => workbench
                        .command_suggestion
                        .checked_sub(1)
                        .unwrap_or(suggestions.len() - 1),
                    _ => (workbench.command_suggestion + 1) % suggestions.len(),
                };
            }
        }
        KeyCode::Backspace => {
            if let Some(command) = &mut workbench.command {
                command.pop();
            }
            workbench.command_suggestion = 0;
            workbench.command_error = None;
        }
        KeyCode::Char(ch)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT) =>
        {
            if let Some(command) = &mut workbench.command {
                command.push(ch);
            }
            workbench.command_suggestion = 0;
            workbench.command_error = None;
        }
        _ => {}
    }
    Ok(false)
}

fn command_suggestions(query: &str) -> Vec<&'static str> {
    ["now", "search", "playlist", "lyrics", "editor", "quit"]
        .into_iter()
        .filter(|command| command.starts_with(&query.trim().to_ascii_lowercase()))
        .collect()
}

fn view_for_command(command: &str) -> Option<View> {
    match command {
        "now" | "nowplaying" | "now-playing" => Some(View::NowPlaying),
        "search" => Some(View::Search),
        "playlist" | "queue" => Some(View::Playlist),
        "lyrics" => Some(View::Lyrics),
        "editor" => Some(View::Editor),
        _ => None,
    }
}

fn handle_lyrics_key(
    workbench: &mut Workbench,
    key: KeyEvent,
    controls: &mpsc::UnboundedSender<Control>,
) -> Result<bool, String> {
    let position_ms = workbench.state.position_ms;
    let page = workbench.state.lyrics.visible_rows.max(1) as isize;
    match key.code {
        KeyCode::Esc => workbench.state.view = View::NowPlaying,
        KeyCode::Up => workbench.state.lyrics.scroll_by(-1, position_ms),
        KeyCode::Down => workbench.state.lyrics.scroll_by(1, position_ms),
        KeyCode::PageUp => workbench.state.lyrics.scroll_by(-page, position_ms),
        KeyCode::PageDown => workbench.state.lyrics.scroll_by(page, position_ms),
        KeyCode::Home => workbench.state.lyrics.scroll = Some(0),
        KeyCode::End => {
            workbench.state.lyrics.scroll = Some(
                workbench
                    .state
                    .lyrics
                    .lines
                    .len()
                    .saturating_sub(workbench.state.lyrics.visible_rows),
            );
        }
        KeyCode::Char('f') => workbench.state.lyrics.scroll = None,
        _ => {
            return Ok(
                action_for_key(key).is_some_and(|action| apply_action(workbench, action, controls))
            );
        }
    }
    Ok(false)
}

fn handle_search_key(
    workbench: &mut Workbench,
    key: KeyEvent,
    controls: &mpsc::UnboundedSender<Control>,
) -> Result<bool, String> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('a') => append_selected_to_playlist(workbench)?,
            KeyCode::Char('p') => play_selected_search(workbench, controls),
            KeyCode::Char('q') => {
                let _ = controls.send(Control::Quit);
                return Ok(true);
            }
            _ => {}
        }
        return Ok(false);
    }

    if key.modifiers.is_empty() && key.code == KeyCode::Char('y') {
        copy_selected_search(workbench, false)?;
        return Ok(false);
    }
    if key.modifiers.is_empty() && key.code == KeyCode::Char('Y') {
        copy_selected_search(workbench, true)?;
        return Ok(false);
    }

    match key.code {
        KeyCode::Esc => workbench.state.view = View::NowPlaying,
        KeyCode::Tab => workbench.state.view = workbench.state.view.next(),
        KeyCode::BackTab => workbench.state.view = workbench.state.view.previous(),
        KeyCode::Up => {
            workbench.state.search.move_up();
            request_selected_search_artwork(workbench, controls);
        }
        KeyCode::Down => {
            workbench.state.search.move_down();
            request_selected_search_artwork(workbench, controls);
        }
        KeyCode::Enter => {
            if workbench.state.search.query.trim().is_empty() {
                return Ok(false);
            }
            if workbench.state.search.query != workbench.state.search.submitted_query
                || workbench.state.search.results.is_empty()
            {
                submit_search(workbench, controls);
            } else {
                play_selected_search(workbench, controls);
            }
        }
        KeyCode::Backspace => {
            workbench.state.search.query.pop();
        }
        KeyCode::Char(ch) => {
            workbench.state.search.query.push(ch);
        }
        _ => {}
    }
    Ok(false)
}

fn handle_editor_key(
    workbench: &mut Workbench,
    key: KeyEvent,
    controls: &mpsc::UnboundedSender<Control>,
) -> Result<bool, String> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('s') => match workbench.editor.save(&workbench.state.file_path) {
                Ok(()) => {
                    workbench.state.git = git_context::detect(&workbench.state.file_path);
                    workbench.state.message = "editor · saved and validated".into();
                }
                Err(error) => workbench.editor.message = error,
            },
            KeyCode::Char('k') => workbench.editor.cut_line(),
            KeyCode::Char('u') => workbench.editor.paste_line(),
            KeyCode::Char('g') => {
                workbench.editor.message =
                    "Ctrl+S save · Ctrl+K cut line · Ctrl+U paste line · Ctrl+X leave".into();
            }
            KeyCode::Char('x') => workbench.state.view = View::NowPlaying,
            KeyCode::Char('q') => {
                let _ = controls.send(Control::Quit);
                return Ok(true);
            }
            _ => {}
        }
        return Ok(false);
    }

    match key.code {
        KeyCode::Left => workbench.editor.move_left(),
        KeyCode::Right => workbench.editor.move_right(),
        KeyCode::Up => workbench.editor.move_up(),
        KeyCode::Down => workbench.editor.move_down(),
        KeyCode::Home => workbench.editor.col = 0,
        KeyCode::End => {
            workbench.editor.col = workbench.editor.lines[workbench.editor.row].chars().count()
        }
        KeyCode::Backspace => workbench.editor.backspace(),
        KeyCode::Delete => workbench.editor.delete(),
        KeyCode::Enter => workbench.editor.newline(),
        KeyCode::Tab => {
            for _ in 0..4 {
                workbench.editor.insert_char(' ');
            }
        }
        KeyCode::Esc => workbench.state.view = View::NowPlaying,
        KeyCode::Char(ch) => workbench.editor.insert_char(ch),
        _ => {}
    }
    Ok(false)
}

fn submit_search(workbench: &mut Workbench, controls: &mpsc::UnboundedSender<Control>) {
    let query = workbench.state.search.query.trim().to_string();
    if query.is_empty() {
        return;
    }
    workbench.state.search.submitted_query = query.clone();
    workbench.state.search.searching = true;
    workbench.state.search.error = None;
    workbench.state.search.results.clear();
    workbench.state.search.selected = 0;
    let _ = controls.send(Control::Search(query));
}

fn play_selected_search(workbench: &mut Workbench, controls: &mpsc::UnboundedSender<Control>) {
    if let Some(item) = workbench.state.search.selected().cloned() {
        workbench.state.transient_current = Some(item.clone());
        request_item_artwork(workbench, &item, controls);
        let _ = controls.send(Control::PlayUri(item.uri.clone()));
        workbench.state.message = format!("playing search result · {}", item.label());
    }
}

fn copy_selected_search(workbench: &mut Workbench, copy_name: bool) -> Result<(), String> {
    let Some(item) = workbench.state.search.selected() else {
        return Ok(());
    };
    let value = if copy_name {
        item.label()
    } else {
        item.uri.clone()
    };
    io::stdout()
        .write_all(clipboard_osc52(&value).as_bytes())
        .and_then(|()| io::stdout().flush())
        .map_err(|error| format!("could not copy Search result: {error}"))?;
    workbench.state.message = if copy_name {
        "copied track name to terminal clipboard".into()
    } else {
        "copied track ID to terminal clipboard".into()
    };
    Ok(())
}

fn clipboard_osc52(value: &str) -> String {
    use base64::Engine;

    format!(
        "\\x1b]52;c;{}\\x07",
        base64::engine::general_purpose::STANDARD.encode(value)
    )
}

fn append_selected_to_playlist(workbench: &mut Workbench) -> Result<(), String> {
    if workbench.editor.dirty {
        workbench.state.message =
            "save or discard editor changes before adding tracks from Search".into();
        return Ok(());
    }

    let Some(item) = workbench.state.search.selected().cloned() else {
        return Ok(());
    };
    if item.title.contains('"') || item.artist.contains('"') {
        workbench.state.message = "cannot write a track label containing quotes yet".into();
        return Ok(());
    }

    let source = fs::read_to_string(&workbench.state.file_path)
        .map_err(|error| format!("could not read playlist: {error}"))?;
    let playlist = Playlist::parse(&source).map_err(|error| error.to_string())?;
    if playlist
        .tracks
        .iter()
        .any(|track| track.id.as_deref() == Some(item.uri.as_str()))
    {
        workbench.state.message = "track already exists in this .riff playlist".into();
        return Ok(());
    }

    let line = format!("    track \"{}\" id=\"{}\"\n", item.label(), item.uri);
    let insert_at = source
        .rfind('}')
        .ok_or_else(|| "playlist is missing closing brace".to_string())?;
    let mut updated = String::with_capacity(source.len() + line.len());
    updated.push_str(&source[..insert_at]);
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&line);
    updated.push_str(&source[insert_at..]);
    Playlist::parse(&updated)
        .map_err(|error| format!("refusing to write invalid playlist: {error}"))?;
    fs::write(&workbench.state.file_path, updated)
        .map_err(|error| format!("could not update playlist: {error}"))?;

    workbench.state.queue.push(item.clone());
    workbench.state.git = git_context::detect(&workbench.state.file_path);
    workbench.editor = EditorState::load(&workbench.state.file_path)?;
    workbench.state.message = format!("added to {} · {}", workbench.state.file_name, item.label());
    Ok(())
}

fn seek_relative(workbench: &Workbench, delta_ms: i64, controls: &mpsc::UnboundedSender<Control>) {
    let duration = workbench.state.duration_ms();
    if duration == 0 {
        return;
    }
    let target = (workbench.state.position_ms as i64 + delta_ms).clamp(0, duration as i64) as u32;
    let _ = controls.send(Control::Seek(target));
}

fn request_current_artwork(workbench: &mut Workbench, controls: &mpsc::UnboundedSender<Control>) {
    let Some(item) = workbench
        .state
        .current()
        .cloned()
        .or_else(|| workbench.state.queue.first().cloned())
    else {
        return;
    };
    request_item_artwork(workbench, &item, controls);
}

fn request_selected_search_artwork(
    workbench: &mut Workbench,
    controls: &mpsc::UnboundedSender<Control>,
) {
    if let Some(item) = workbench.state.search.selected().cloned() {
        request_item_artwork(workbench, &item, controls);
    }
}

fn request_item_artwork(
    workbench: &mut Workbench,
    item: &QueueItem,
    controls: &mpsc::UnboundedSender<Control>,
) {
    if workbench.artwork.contains_key(&item.uri)
        || workbench.artwork_pending.contains_key(&item.uri)
    {
        return;
    }
    let Some(cover_id) = item.cover_id.clone() else {
        return;
    };
    let _ = controls.send(Control::RequestArtwork {
        key: item.uri.clone(),
        cover_id,
    });
}

fn handle_mouse(
    workbench: &mut Workbench,
    mouse: MouseEvent,
    controls: &mpsc::UnboundedSender<Control>,
) -> Result<(), String> {
    let point = (mouse.column, mouse.row);
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if workbench
                .state
                .hits
                .previous
                .is_some_and(|rect| contains(rect, point))
            {
                let _ = controls.send(Control::Previous);
            } else if workbench
                .state
                .hits
                .toggle
                .is_some_and(|rect| contains(rect, point))
            {
                let _ = controls.send(Control::Toggle);
            } else if workbench
                .state
                .hits
                .next
                .is_some_and(|rect| contains(rect, point))
            {
                let _ = controls.send(Control::Next);
            } else if let Some(rect) = workbench
                .state
                .hits
                .progress
                .filter(|rect| contains(*rect, point))
            {
                let duration = workbench.state.duration_ms();
                if duration > 0 && rect.width > 1 {
                    let relative = mouse.column.saturating_sub(rect.x) as f64 / rect.width as f64;
                    let _ = controls.send(Control::Seek(
                        (duration as f64 * relative.clamp(0.0, 1.0)) as u32,
                    ));
                }
            } else if let Some(rect) = workbench
                .state
                .hits
                .volume
                .filter(|rect| contains(*rect, point))
            {
                if rect.width > 1 {
                    let relative = mouse.column.saturating_sub(rect.x) as f64 / rect.width as f64;
                    let percent = (relative.clamp(0.0, 1.0) * 100.0).round() as u8;
                    set_volume_percent(workbench, percent, controls);
                }
            } else if workbench.state.view == View::Search
                && let Some((index, _)) = workbench
                    .state
                    .hits
                    .search_rows
                    .iter()
                    .find(|(_, rect)| contains(*rect, point))
            {
                workbench.state.search.selected = *index;
                request_selected_search_artwork(workbench, controls);
            }
        }
        MouseEventKind::ScrollUp => {
            if workbench.state.view == View::Search {
                workbench.state.search.move_up();
                request_selected_search_artwork(workbench, controls);
            } else if workbench
                .state
                .hits
                .volume
                .is_some_and(|rect| contains(rect, point))
            {
                adjust_volume(workbench, VOLUME_STEP_PERCENT, controls);
            } else if workbench.state.view == View::Lyrics
                && workbench
                    .state
                    .hits
                    .lyrics
                    .is_some_and(|rect| contains(rect, point))
            {
                workbench
                    .state
                    .lyrics
                    .scroll_by(-1, workbench.state.position_ms);
            }
        }
        MouseEventKind::ScrollDown => {
            if workbench.state.view == View::Search {
                workbench.state.search.move_down();
                request_selected_search_artwork(workbench, controls);
            } else if workbench
                .state
                .hits
                .volume
                .is_some_and(|rect| contains(rect, point))
            {
                adjust_volume(workbench, -VOLUME_STEP_PERCENT, controls);
            } else if workbench.state.view == View::Lyrics
                && workbench
                    .state
                    .hits
                    .lyrics
                    .is_some_and(|rect| contains(rect, point))
            {
                workbench
                    .state
                    .lyrics
                    .scroll_by(1, workbench.state.position_ms);
            }
        }
        _ => {}
    }
    Ok(())
}

fn volume_percent(volume: u16) -> u8 {
    (((volume as u32 * 100) + (VOLUME_MAX as u32 / 2)) / VOLUME_MAX as u32).min(100) as u8
}

fn volume_from_percent(percent: u8) -> u16 {
    (((percent.min(100) as u32 * VOLUME_MAX as u32) + 50) / 100) as u16
}

fn quantize_volume_percent(percent: u8) -> u8 {
    let step = VOLUME_STEP_PERCENT as u16;
    ((((percent.min(100) as u16) + step / 2) / step) * step).min(100) as u8
}

fn set_volume_percent(
    workbench: &mut Workbench,
    percent: u8,
    controls: &mpsc::UnboundedSender<Control>,
) {
    let percent = quantize_volume_percent(percent);
    let volume = volume_from_percent(percent);
    workbench.state.volume = volume;
    workbench.pending_volume = Some(volume);
    flush_pending_volume(workbench, controls);
}

fn adjust_volume(
    workbench: &mut Workbench,
    delta_percent: i16,
    controls: &mpsc::UnboundedSender<Control>,
) {
    let current = quantize_volume_percent(volume_percent(workbench.state.volume)) as i16;
    let target = (current + delta_percent).clamp(0, 100) as u8;
    set_volume_percent(workbench, target, controls);
}

fn flush_pending_volume(workbench: &mut Workbench, controls: &mpsc::UnboundedSender<Control>) {
    if workbench.last_volume_send.elapsed() < VOLUME_SEND_INTERVAL {
        return;
    }
    if let Some(volume) = workbench.pending_volume.take() {
        let _ = controls.send(Control::SetVolume(volume));
        workbench.last_volume_send = Instant::now();
    }
}

fn contains(rect: Rect, point: (u16, u16)) -> bool {
    point.0 >= rect.x
        && point.0 < rect.x.saturating_add(rect.width)
        && point.1 >= rect.y
        && point.1 < rect.y.saturating_add(rect.height)
}

fn draw(frame: &mut Frame<'_>, workbench: &mut Workbench) {
    let theme = workbench.state.theme;
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background).fg(theme.foreground)),
        frame.area(),
    );
    workbench.state.hits = HitMap::default();

    let outer = Layout::default()
        .constraints([
            Constraint::Min(10),
            Constraint::Length(4),
            Constraint::Length(1),
        ])
        .split(frame.area());

    match workbench.state.view {
        View::NowPlaying => draw_now_playing(frame, outer[0], workbench),
        View::Search => draw_search(frame, outer[0], workbench),
        View::Playlist => draw_playlist(frame, outer[0], workbench),
        View::Lyrics => draw_lyrics(frame, outer[0], workbench),
        View::Editor => draw_editor(frame, outer[0], workbench),
    }
    draw_transport(frame, outer[1], workbench);
    draw_status(frame, outer[2], workbench);
    draw_command_popup(frame, frame.area(), workbench);
}

fn draw_now_playing(frame: &mut Frame<'_>, area: Rect, workbench: &Workbench) {
    if area.width >= 82 && area.height >= 14 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(34), Constraint::Min(32)])
            .split(area);
        draw_art(
            frame,
            columns[0],
            workbench,
            false,
            current_art_key(workbench),
        );
        draw_metadata(frame, columns[1], workbench);
    } else if area.height >= 27 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(11), Constraint::Min(7)])
            .split(area);
        draw_art(frame, rows[0], workbench, true, current_art_key(workbench));
        draw_metadata(frame, rows[1], workbench);
    } else {
        draw_metadata(frame, area, workbench);
    }
}

fn current_art_key(workbench: &Workbench) -> Option<&str> {
    workbench
        .state
        .current_uri
        .as_deref()
        .or_else(|| workbench.state.queue.first().map(|item| item.uri.as_str()))
}

fn draw_art(
    frame: &mut Frame<'_>,
    area: Rect,
    workbench: &Workbench,
    compact: bool,
    key: Option<&str>,
) {
    let theme = workbench.state.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(theme.background))
        .title(" album ")
        .padding(Padding::uniform(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(key) = key else {
        return;
    };
    let Some(artwork) = workbench.artwork.get(key) else {
        frame.render_widget(
            Paragraph::new("loading artwork…")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.muted)),
            inner,
        );
        return;
    };
    let protocol = if compact {
        &artwork.compact
    } else {
        &artwork.wide
    };
    let image_width = protocol.size().width.min(inner.width);
    let image_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(image_width),
            Constraint::Min(0),
        ])
        .split(inner)[1];
    frame.render_widget(
        TerminalImage::new(protocol).allow_clipping(true),
        image_area,
    );
}

fn draw_metadata(frame: &mut Frame<'_>, area: Rect, workbench: &Workbench) {
    let theme = workbench.state.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(theme.background))
        .title(" now playing ")
        .padding(Padding::new(2, 2, 1, 1));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(current) = workbench
        .state
        .current()
        .or_else(|| workbench.state.queue.first())
    else {
        return;
    };
    let status_color = if workbench.state.status == PlaybackStatus::Playing {
        theme.playing
    } else {
        theme.accent
    };
    let mut lines = vec![
        Line::from(Span::styled(
            format!(
                "{}  {}",
                workbench.state.status.glyph(),
                workbench.state.status.label().to_uppercase()
            ),
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            &current.title,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            &current.artist,
            Style::default().fg(theme.accent),
        )),
        Line::from(""),
        Line::from(format!("album    {}", current.album)),
    ];
    if let Some(version) = &current.version {
        lines.push(Line::from(format!("version  {version}")));
    }
    lines.push(Line::from(Span::styled(
        &current.uri,
        Style::default().fg(theme.muted),
    )));
    let visualizer_height = inner.height.saturating_sub(lines.len() as u16).min(5);
    let panels = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(visualizer_height)])
        .split(inner);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), panels[0]);
    frame.render_widget(
        Paragraph::new(visualizer_lines(
            &workbench.visualizer,
            panels[1].width,
            panels[1].height,
            theme.accent,
        )),
        panels[1],
    );
}

fn visualizer_lines(
    levels: &[u8; 32],
    width: u16,
    height: u16,
    color: ratatui::style::Color,
) -> Vec<Line<'static>> {
    let bar_count = width.saturating_add(1) as usize / 2;
    (0..height)
        .map(|row| {
            let threshold = u32::from(height - row) * u32::from(u8::MAX);
            let mut bars = String::with_capacity(width as usize);
            for index in 0..bar_count {
                let level = levels[index * levels.len() / bar_count];
                let symbol = if u32::from(level) * u32::from(height) >= threshold {
                    '█'
                } else if row + 1 == height {
                    '▁'
                } else {
                    ' '
                };
                bars.push(symbol);
                if index + 1 < bar_count {
                    bars.push(' ');
                }
            }
            Line::from(Span::styled(bars, Style::default().fg(color)))
        })
        .collect()
}

#[cfg(test)]
mod visualizer_tests {
    use super::*;

    #[test]
    fn draws_full_height_bars_across_the_visualizer_width_without_background() {
        let lines = visualizer_lines(&[u8::MAX; 32], 100, 4, ratatui::style::Color::Green);
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(100, 4)).unwrap();
        terminal
            .draw(|frame| frame.render_widget(Paragraph::new(lines), frame.area()))
            .unwrap();

        for row in 0..4 {
            let buffer = terminal.backend().buffer();
            let filled = (0..100)
                .filter(|x| buffer.cell((*x, row)).unwrap().symbol() == "█")
                .count();
            assert_eq!(filled, 50);
            assert_eq!(
                buffer.cell((0, row)).unwrap().bg,
                ratatui::style::Color::Reset
            );
        }
        let lines = visualizer_lines(&[0; 32], 100, 1, ratatui::style::Color::Green);
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(100, 1)).unwrap();
        terminal
            .draw(|frame| frame.render_widget(Paragraph::new(lines), frame.area()))
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.cell((0, 0)).unwrap().symbol(), "▁");
        assert_eq!(buffer.cell((1, 0)).unwrap().symbol(), " ");
        assert_eq!(buffer.cell((2, 0)).unwrap().symbol(), "▁");
    }
}

fn draw_search(frame: &mut Frame<'_>, area: Rect, workbench: &mut Workbench) {
    let theme = workbench.state.theme;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(4)])
        .split(area);
    let prompt = Paragraph::new(format!("> {}▏", workbench.state.search.query)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent))
            .title_style(
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
            .style(Style::default().bg(theme.background))
            .title(" smart search "),
    );
    frame.render_widget(prompt, rows[0]);

    if workbench.state.search.searching {
        frame.render_widget(
            Paragraph::new("Searching Spotify…").alignment(Alignment::Center),
            rows[1],
        );
        return;
    }
    if let Some(error) = &workbench.state.search.error {
        frame.render_widget(
            Paragraph::new(error.as_str())
                .style(Style::default().fg(theme.danger))
                .wrap(Wrap { trim: true }),
            rows[1],
        );
        return;
    }
    if workbench.state.search.results.is_empty() {
        frame.render_widget(Paragraph::new("Type anything, typos included. Enter searches · Enter again plays · Ctrl+A writes the selected exact ID into the .riff file.").wrap(Wrap { trim: true }).alignment(Alignment::Center), rows[1]);
        return;
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(rows[1]);
    workbench.state.hits.search_rows.clear();
    let visible_height = columns[0].height.saturating_sub(2) as usize / 2;
    let selected = workbench.state.search.selected;
    let start = selected.saturating_sub(visible_height / 2);
    let items = workbench
        .state
        .search
        .results
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_height)
        .map(|(index, item)| {
            let score = item
                .match_score
                .map(|score| format!(" {score:>3}%"))
                .unwrap_or_default();
            let version = item
                .version
                .as_ref()
                .map(|value| format!(" · {value}"))
                .unwrap_or_default();
            let text = format!(
                "{}  {}\n     {} · {}{}",
                score, item.title, item.artist, item.album, version
            );
            let style = if index == selected {
                Style::default()
                    .bg(theme.selection)
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
        })
        .collect::<Vec<_>>();
    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(theme.background))
        .title(format!(
            " results · {} ",
            workbench.state.search.results.len()
        ));
    frame.render_widget(List::new(items).block(list_block), columns[0]);

    for visible in 0..visible_height.min(workbench.state.search.results.len().saturating_sub(start))
    {
        let y = columns[0]
            .y
            .saturating_add(1)
            .saturating_add((visible as u16).saturating_mul(2));
        if y < columns[0].bottom() {
            workbench.state.hits.search_rows.push((
                start + visible,
                Rect::new(columns[0].x + 1, y, columns[0].width.saturating_sub(2), 2),
            ));
        }
    }

    if let Some(item) = workbench.state.search.selected() {
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(13), Constraint::Min(5)])
            .split(columns[1]);
        let art_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border))
            .title_style(
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
            .style(Style::default().bg(theme.background))
            .title(" selected ")
            .padding(Padding::uniform(1));
        let inner = art_block.inner(right[0]);
        frame.render_widget(art_block, right[0]);
        if let Some(art) = workbench.artwork.get(&item.uri) {
            frame.render_widget(TerminalImage::new(&art.search).allow_clipping(true), inner);
        } else {
            frame.render_widget(
                Paragraph::new("loading cover…").alignment(Alignment::Center),
                inner,
            );
        }
        let details = vec![
            Line::from(Span::styled(
                &item.title,
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                &item.artist,
                Style::default().fg(theme.accent),
            )),
            Line::from(item.album.as_str()),
            Line::from(format_duration(item.duration_ms)),
            Line::from(""),
            Line::from(Span::styled(
                "Enter play · Ctrl+A add to .riff",
                Style::default().fg(theme.muted),
            )),
        ];
        frame.render_widget(Paragraph::new(details).wrap(Wrap { trim: true }), right[1]);
    }
}

fn playlist_window(total: usize, current: Option<usize>, visible: usize) -> (usize, usize) {
    if total == 0 || visible == 0 {
        return (0, 0);
    }

    let visible = visible.min(total);
    let focus = current.unwrap_or(0).min(total - 1);
    let mut start = focus.saturating_sub(visible / 2);
    if start + visible > total {
        start = total - visible;
    }
    (start, start + visible)
}

fn draw_playlist(frame: &mut Frame<'_>, area: Rect, workbench: &Workbench) {
    let theme = workbench.state.theme;
    let current = workbench.state.current_index();
    let visible_items = usize::from(area.height.saturating_sub(2) / 2);
    let (start, end) = playlist_window(workbench.state.queue.len(), current, visible_items);
    let items = workbench
        .state
        .queue
        .iter()
        .enumerate()
        .skip(start)
        .take(end.saturating_sub(start))
        .map(|(index, item)| {
            let prefix = if Some(index) == current { "▶" } else { " " };
            let text = format!(
                "{prefix} {:>2}. {}\n      {} · {}",
                index + 1,
                item.title,
                item.artist,
                item.album
            );
            let style = if Some(index) == current {
                Style::default()
                    .fg(theme.highlight)
                    .bg(theme.selection)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
        })
        .collect::<Vec<_>>();
    let window = if start > 0 || end < workbench.state.queue.len() {
        format!(" · showing {}-{}", start + 1, end)
    } else {
        String::new()
    };
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border))
                .title_style(
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                )
                .style(Style::default().bg(theme.background))
                .title(format!(
                    " {} · {} tracks{} · generated by code or TUI ",
                    workbench.state.playlist_name,
                    workbench.state.queue.len(),
                    window
                )),
        ),
        area,
    );
}

fn draw_lyrics(frame: &mut Frame<'_>, area: Rect, workbench: &mut Workbench) {
    let theme = workbench.state.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(theme.background))
        .title(format!(
            " lyrics{} ",
            workbench
                .state
                .lyrics
                .provider
                .as_ref()
                .map(|provider| format!(" · {provider}"))
                .unwrap_or_default()
        ));
    let inner = block.inner(area);
    workbench.state.hits.lyrics = Some(inner);
    workbench.state.lyrics.visible_rows = inner.height as usize;
    frame.render_widget(block, area);
    if workbench.state.lyrics.loading {
        frame.render_widget(
            Paragraph::new("Loading synchronized lyrics…").alignment(Alignment::Center),
            inner,
        );
        return;
    }
    if let Some(error) = &workbench.state.lyrics.error {
        frame.render_widget(
            Paragraph::new(format!("No synced lyrics for this track.\n\n{error}"))
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.muted))
                .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }
    if workbench.state.lyrics.lines.is_empty() {
        frame.render_widget(
            Paragraph::new("Lyrics will appear when playback starts.").alignment(Alignment::Center),
            inner,
        );
        return;
    }
    let position_ms = workbench.state.position_ms;
    let active = workbench.state.lyrics.active_line(position_ms).unwrap_or(0);
    let start = workbench.state.lyrics.scroll_offset(position_ms);
    let lines = workbench
        .state
        .lyrics
        .lines
        .iter()
        .enumerate()
        .skip(start)
        .take(inner.height as usize)
        .map(|(index, line)| {
            let style = if index == active {
                Style::default()
                    .fg(theme.playing)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            };
            Line::from(Span::styled(line.text.as_str(), style)).alignment(Alignment::Center)
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        inner,
    );
}

fn draw_editor(frame: &mut Frame<'_>, area: Rect, workbench: &mut Workbench) {
    let theme = workbench.state.theme;
    let title = format!(
        " nano-ish editor · {}{} ",
        workbench.state.file_name,
        if workbench.editor.dirty { " *" } else { "" }
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(theme.background))
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let height = inner.height.saturating_sub(1) as usize;
    workbench.editor.ensure_cursor_visible(height);
    let lines = workbench
        .editor
        .lines
        .iter()
        .enumerate()
        .skip(workbench.editor.scroll)
        .take(height)
        .map(|(index, source)| {
            let mut spans = vec![Span::styled(
                format!("{:>4} │ ", index + 1),
                Style::default().fg(theme.muted),
            )];
            spans.extend(highlight_riff_line(source, theme));
            Line::from(spans)
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines),
        Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(1),
        ),
    );
    frame.render_widget(
        Paragraph::new(workbench.editor.message.as_str()).style(Style::default().fg(theme.muted)),
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
    );

    let cursor_row = workbench.editor.row.saturating_sub(workbench.editor.scroll) as u16;
    if cursor_row < inner.height.saturating_sub(1) {
        let gutter = 7u16;
        let x = inner
            .x
            .saturating_add(gutter)
            .saturating_add(workbench.editor.col as u16);
        let y = inner.y.saturating_add(cursor_row);
        frame.set_cursor_position((x.min(inner.right().saturating_sub(1)), y));
    }
}

fn highlight_riff_line(source: &str, theme: Theme) -> Vec<Span<'_>> {
    let trimmed = source.trim_start();
    let leading = source.len().saturating_sub(trimmed.len());
    let mut spans = Vec::new();
    if leading > 0 {
        spans.push(Span::raw(&source[..leading]));
    }
    if trimmed.starts_with("playlist ") {
        spans.push(Span::styled(
            "playlist",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(&trimmed[8..]));
    } else if trimmed.starts_with("track ") {
        spans.push(Span::styled("track", Style::default().fg(theme.accent)));
        spans.push(Span::raw(&trimmed[5..]));
    } else if trimmed.starts_with('#') {
        spans.push(Span::styled(trimmed, Style::default().fg(theme.muted)));
    } else {
        spans.push(Span::raw(trimmed));
    }
    spans
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    let compact = transport_mode(area.width) == TransportMode::Compact;
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
    let previous = Paragraph::new("|◀")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.accent));
    let toggle = Paragraph::new(if workbench.state.status == PlaybackStatus::Playing {
        "Ⅱ"
    } else {
        "▶"
    })
    .alignment(Alignment::Center)
    .style(
        Style::default()
            .fg(if workbench.state.status == PlaybackStatus::Playing {
                theme.playing
            } else {
                theme.accent
            })
            .add_modifier(Modifier::BOLD),
    );
    let next = Paragraph::new("▶|")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.accent));
    frame.render_widget(previous, top[0]);
    frame.render_widget(toggle, top[1]);
    frame.render_widget(next, top[2]);
    workbench.state.hits.previous = Some(top[0]);
    workbench.state.hits.toggle = Some(top[1]);
    workbench.state.hits.next = Some(top[2]);

    let shuffle = if workbench.state.shuffle {
        "●"
    } else {
        "○"
    };
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
        Gauge::default()
            .ratio(volume_ratio)
            .label(volume_label)
            .style(Style::default().fg(theme.muted).bg(theme.background))
            .gauge_style(Style::default().fg(theme.accent).bg(theme.background)),
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
        LineGauge::default()
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(theme.border))
                    .style(Style::default().bg(theme.background)),
            )
            .ratio(ratio)
            .filled_symbol("━")
            .unfilled_symbol("┄")
            .filled_style(Style::default().fg(theme.accent))
            .unfilled_style(Style::default().fg(theme.border))
            .style(Style::default().bg(theme.background))
            .label(""),
        rows[1],
    );
    workbench.state.hits.progress = Some(rows[1]);
}

fn draw_status(frame: &mut Frame<'_>, area: Rect, workbench: &Workbench) {
    let theme = workbench.state.theme;
    let (text, style, alignment) = if let Some(error) = &workbench.command_error {
        (
            format!("{error} · press : to retry"),
            Style::default().fg(theme.danger),
            Alignment::Left,
        )
    } else {
        let hint = match workbench.state.view {
            View::Search => {
                " Enter search/play · ↑↓ select · Ctrl+A add · Ctrl+P play · y ID · Y name · : commands "
                    .to_string()
            }
            View::Lyrics => " ↑↓/PgUp/PgDn scroll · f follow playback · : commands ".to_string(),
            View::Editor => {
                " Ctrl+S save · Ctrl+K/U cut/paste · Ctrl+G help · Ctrl+X leave ".to_string()
            }
            _ => global_status_hint(area.width < COMPACT_TRANSPORT_WIDTH),
        };
        (hint, Style::default().fg(theme.muted), Alignment::Center)
    };
    frame.render_widget(Paragraph::new(text).alignment(alignment).style(style), area);
}

fn draw_command_popup(frame: &mut Frame<'_>, area: Rect, workbench: &Workbench) {
    let Some(command) = &workbench.command else {
        return;
    };
    let theme = workbench.state.theme;
    let suggestions = command_suggestions(command);
    let rows = suggestions.len().clamp(1, 8) as u16;
    let popup_height = (3 + rows).min(area.height);
    let popup_width = area.width.min(56);
    let popup = Rect {
        x: area.x + area.width.saturating_sub(popup_width) / 2,
        y: area.y + area.height.saturating_sub(popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };
    let input = Rect {
        height: popup.height.min(3),
        ..popup
    };
    let line = Line::from(vec![
        Span::styled("> ", Style::default().fg(theme.playing)),
        Span::raw(command.as_str()),
        Span::styled("█", Style::default().fg(theme.playing)),
    ]);
    frame.render_widget(
        Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.playing))
                .title(" Cmdline ")
                .title_alignment(Alignment::Center)
                .style(Style::default().bg(theme.background)),
        ),
        input,
    );

    let list = Rect {
        y: popup.y.saturating_add(input.height),
        height: popup.height.saturating_sub(input.height),
        ..popup
    };
    if list.height == 0 {
        return;
    }
    let rows = if suggestions.is_empty() {
        vec![Line::from("  No matching command")]
    } else {
        suggestions
            .iter()
            .take(list.height as usize)
            .enumerate()
            .map(|(index, suggestion)| {
                let selected = index == workbench.command_suggestion;
                Line::from(vec![
                    Span::styled(
                        if selected { "› " } else { "  " },
                        Style::default().fg(theme.playing),
                    ),
                    Span::styled(
                        *suggestion,
                        Style::default()
                            .fg(if selected {
                                theme.foreground
                            } else {
                                theme.muted
                            })
                            .bg(if selected {
                                theme.selection
                            } else {
                                theme.background
                            })
                            .add_modifier(if selected {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                ])
            })
            .collect()
    };
    frame.render_widget(
        Paragraph::new(rows).style(Style::default().bg(theme.background)),
        list,
    );
}

fn format_duration(duration_ms: u32) -> String {
    let total_seconds = duration_ms / 1000;
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_prompt_renders_without_starting_playback() {
        let theme = Theme::named("riff").unwrap();
        let mut workbench = Workbench {
            state: AppState {
                file_path: "playlist.riff".into(),
                file_name: "playlist.riff".into(),
                playlist_name: "test".into(),
                queue: Vec::new(),
                transient_current: None,
                status: PlaybackStatus::Starting,
                current_uri: None,
                position_ms: 0,
                volume: 0,
                shuffle: false,
                repeat: false,
                message: String::new(),
                view: View::NowPlaying,
                theme,
                git: None,
                search: Default::default(),
                lyrics: Default::default(),
                hits: Default::default(),
            },
            editor: EditorState {
                lines: vec![String::new()],
                row: 0,
                col: 0,
                scroll: 0,
                dirty: false,
                clipboard: None,
                message: String::new(),
            },
            artwork: HashMap::new(),
            artwork_pending: HashMap::new(),
            pending_volume: None,
            last_volume_send: Instant::now(),
            command: Some(String::new()),
            command_suggestion: 0,
            command_error: None,
            visualizer: [0; 32],
        };
        let (controls, mut pending_controls) = mpsc::unbounded_channel();
        workbench.command = Some("lyrics".into());
        assert!(
            !handle_command_key(
                &mut workbench,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                &controls
            )
            .unwrap()
        );
        assert_eq!(workbench.state.view, View::Lyrics);
        assert!(matches!(
            pending_controls.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        workbench.command = Some(String::new());
        assert!(
            !handle_command_key(
                &mut workbench,
                KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                &controls,
            )
            .unwrap()
        );
        assert_eq!(workbench.command_suggestion, 1);
        assert!(
            !handle_command_key(
                &mut workbench,
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                &controls,
            )
            .unwrap()
        );
        assert_eq!(workbench.state.view, View::Search);
        let item = QueueItem {
            title: "War Pigs".into(),
            artist: "Black Sabbath".into(),
            album: "Paranoid".into(),
            version: None,
            uri: "spotify:track:war-pigs".into(),
            duration_ms: 470_000,
            cover_id: None,
            match_score: None,
        };
        workbench.state.search.results.push(item.clone());
        workbench.state.queue.push(item);
        handle_search_key(
            &mut workbench,
            KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
            &controls,
        )
        .unwrap();
        assert_eq!(
            workbench.state.message,
            "copied track ID to terminal clipboard"
        );
        handle_search_key(
            &mut workbench,
            KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::NONE),
            &controls,
        )
        .unwrap();
        assert_eq!(
            workbench.state.message,
            "copied track name to terminal clipboard"
        );
        workbench.command_suggestion = 1;
        handle_key(
            &mut workbench,
            KeyEvent::new(KeyCode::Char(':'), KeyModifiers::NONE),
            &controls,
        )
        .unwrap();
        assert_eq!(workbench.command_suggestion, 0);
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(100, 40)).unwrap();

        terminal.draw(|frame| draw(frame, &mut workbench)).unwrap();

        let output = (0..40)
            .map(|y| {
                (0..100)
                    .map(|x| {
                        terminal
                            .backend()
                            .buffer()
                            .cell((x, y))
                            .map_or(" ", |cell| cell.symbol())
                    })
                    .collect::<String>()
            })
            .collect::<String>();
        assert!(output.contains("Cmdline"));
        assert!(output.contains("> █"));
        let title_row = (0..40).find(|y| {
            (0..100)
                .map(|x| {
                    terminal
                        .backend()
                        .buffer()
                        .cell((x, *y))
                        .map_or(" ", |cell| cell.symbol())
                })
                .collect::<String>()
                .contains("Cmdline")
        });
        assert_eq!(title_row, Some(15));
        let suggestion_row = (0..100)
            .map(|x| {
                terminal
                    .backend()
                    .buffer()
                    .cell((x, 18))
                    .map_or(" ", |cell| cell.symbol())
            })
            .collect::<String>();
        assert!(suggestion_row.contains("now"));
        workbench.command = None;
        workbench.visualizer = [u8::MAX; 32];
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
        terminal
            .draw(|frame| draw_metadata(frame, frame.area(), &workbench))
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.cell((95, 27)).unwrap().symbol(), "█");
        assert_eq!(buffer.cell((50, 29)).unwrap().symbol(), "─");
    }

    #[test]
    fn command_names_resolve_to_their_workbench_views() {
        for (command, view) in [
            ("now", View::NowPlaying),
            ("nowplaying", View::NowPlaying),
            ("now-playing", View::NowPlaying),
            ("search", View::Search),
            ("playlist", View::Playlist),
            ("queue", View::Playlist),
            ("lyrics", View::Lyrics),
            ("editor", View::Editor),
        ] {
            assert_eq!(view_for_command(command), Some(view));
        }
        assert_eq!(view_for_command("unknown"), None);
    }

    #[test]
    fn command_suggestions_filter_and_clipboard_uses_osc52() {
        assert_eq!(command_suggestions("pl"), ["playlist"]);
        assert_eq!(command_suggestions("NO"), ["now"]);
        assert_eq!(clipboard_osc52("War Pigs"), "\\x1b]52;c;V2FyIFBpZ3M=\\x07");
    }

    #[tokio::test]
    async fn ready_cached_queue_skips_the_startup_panel() {
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        let queue = resolve_queue_with_startup(
            &mut terminal,
            "cached-playlist",
            Theme::named("riff").unwrap(),
            async { Ok(Vec::new()) },
            || Ok(false),
        )
        .await
        .unwrap();

        assert!(queue.is_empty());
        let mut output = String::new();
        for y in 0..24 {
            for x in 0..80 {
                output.push_str(
                    terminal
                        .backend()
                        .buffer()
                        .cell((x, y))
                        .map_or(" ", |cell| cell.symbol()),
                );
            }
        }
        assert!(!output.contains("Preparing playlist"));
    }

    #[tokio::test]
    async fn slow_queue_shows_the_startup_panel() {
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        let queue = resolve_queue_with_startup(
            &mut terminal,
            "uncached-playlist",
            Theme::named("riff").unwrap(),
            async {
                tokio::time::sleep(Duration::from_millis(110)).await;
                Ok(Vec::new())
            },
            || Ok(false),
        )
        .await
        .unwrap();

        assert!(queue.is_empty());
        assert!(
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .contains("Preparing playlist")
        );
    }

    #[test]
    fn startup_screen_has_one_clear_loading_message() {
        let theme = Theme::named("riff").unwrap();
        for (width, height) in [(80, 24), (160, 40)] {
            let mut terminal =
                Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| draw_startup(frame, "coding-metal", theme))
                .unwrap();

            let output = (0..height)
                .map(|y| {
                    (0..width)
                        .map(|x| {
                            terminal
                                .backend()
                                .buffer()
                                .cell((x, y))
                                .map_or(" ", |cell| cell.symbol())
                        })
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");

            assert!(output.contains("Preparing playlist"));
            assert!(output.contains("coding-metal"));
            assert_eq!(output.matches("RIFF").count(), 1);
            assert_eq!(output.matches("Loading track metadata").count(), 1);
            assert!(
                !["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"]
                    .iter()
                    .any(|frame| output.contains(frame))
            );
            assert_eq!(
                terminal
                    .backend()
                    .buffer()
                    .cell(((width - 64) / 2, (height - 9) / 2))
                    .unwrap()
                    .symbol(),
                "┌"
            );
        }
    }

    #[test]
    fn transport_mode_switches_at_compact_breakpoint() {
        assert_eq!(
            transport_mode(COMPACT_TRANSPORT_WIDTH - 1),
            TransportMode::Compact
        );
        assert_eq!(transport_mode(COMPACT_TRANSPORT_WIDTH), TransportMode::Wide);
    }

    #[test]
    fn formats_transport_time() {
        assert_eq!(format_duration(475_000), "7:55");
    }

    #[test]
    fn rectangle_hit_test_is_half_open() {
        let rect = Rect::new(10, 10, 5, 5);
        assert!(contains(rect, (10, 10)));
        assert!(contains(rect, (14, 14)));
        assert!(!contains(rect, (15, 14)));
    }

    #[test]
    fn volume_percent_round_trips_expected_steps() {
        for percent in (0..=100).step_by(5) {
            assert_eq!(volume_percent(volume_from_percent(percent)), percent);
        }
    }

    #[test]
    fn volume_quantization_uses_five_percent_steps() {
        assert_eq!(quantize_volume_percent(0), 0);
        assert_eq!(quantize_volume_percent(2), 0);
        assert_eq!(quantize_volume_percent(3), 5);
        assert_eq!(quantize_volume_percent(72), 70);
        assert_eq!(quantize_volume_percent(73), 75);
        assert_eq!(quantize_volume_percent(99), 100);
        assert_eq!(quantize_volume_percent(100), 100);
    }

    #[test]
    fn playlist_window_shows_all_short_queues() {
        assert_eq!(playlist_window(3, Some(2), 8), (0, 3));
    }

    #[test]
    fn playlist_window_clamps_near_start() {
        assert_eq!(playlist_window(20, Some(1), 5), (0, 5));
    }

    #[test]
    fn playlist_window_centers_middle_track() {
        assert_eq!(playlist_window(20, Some(10), 5), (8, 13));
    }

    #[test]
    fn playlist_window_clamps_near_end() {
        assert_eq!(playlist_window(20, Some(19), 5), (15, 20));
    }

    #[test]
    fn playlist_window_handles_empty_or_zero_height() {
        assert_eq!(playlist_window(0, None, 5), (0, 0));
        assert_eq!(playlist_window(20, Some(10), 0), (0, 0));
    }
}
