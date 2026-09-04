mod editor;
mod git_context;
mod model;
mod player_task;
mod theme;

use std::{collections::HashMap, fs, io, path::PathBuf, sync::Arc, time::Duration};

use crossterm::{
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
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect, Size},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Padding, Paragraph, Wrap},
};
use ratatui_image::{Image as TerminalImage, Resize, picker::Picker, protocol::Protocol};
use riff::Playlist;
use tokio::sync::mpsc;

use editor::EditorState;
use model::{AppState, HitMap, LyricsState, PlaybackStatus, QueueItem, SearchState, View};
use player_task::{Control, PlayerUpdate};
use theme::Theme;

const WIDE_ART_SIZE: Size = Size::new(28, 14);
const COMPACT_ART_SIZE: Size = Size::new(18, 9);
const SEARCH_ART_SIZE: Size = Size::new(22, 11);
const VOLUME_MAX: u16 = u16::MAX;
const VOLUME_STEP_PERCENT: u8 = 5;
const SEEK_STEP_MS: u32 = 5_000;

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
}

pub async fn run(file_path: PathBuf, playlist: Playlist) -> Result<(), String> {
    if playlist.tracks.is_empty() {
        return Err("playlist has no tracks to play".to_string());
    }
    let queue = player_task::resolve_queue(&playlist).await?;
    let editor = EditorState::load(&file_path)?;
    let file_name = file_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| file_path.display().to_string());

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
        theme: Theme::from_env(),
        git: git_context::detect(&file_path),
        search: SearchState::default(),
        lyrics: LyricsState::default(),
        hits: HitMap::default(),
    };

    let (control_tx, control_rx) = mpsc::unbounded_channel();
    let (update_tx, update_rx) = mpsc::unbounded_channel();
    let player_task = tokio::spawn(async move {
        if let Err(error) = player_task::run_player(queue, control_rx, update_tx.clone()).await {
            let _ = update_tx.send(PlayerUpdate::Error(error));
        }
    });

    let mut workbench = Workbench {
        state,
        editor,
        artwork: HashMap::new(),
        artwork_pending: HashMap::new(),
    };
    let ui_result = run_terminal(&mut workbench, control_tx, update_rx).await;
    let _ = player_task.await;
    ui_result
}

struct TerminalModeGuard {
    raw_enabled: bool,
    alternate_enabled: bool,
}

impl Drop for TerminalModeGuard {
    fn drop(&mut self) {
        if self.alternate_enabled {
            let mut stdout = io::stdout();
            let _ = execute!(stdout, DisableMouseCapture, LeaveAlternateScreen);
        }
        if self.raw_enabled {
            let _ = disable_raw_mode();
        }
    }
}

async fn run_terminal(
    workbench: &mut Workbench,
    controls: mpsc::UnboundedSender<Control>,
    mut updates: mpsc::UnboundedReceiver<PlayerUpdate>,
) -> Result<(), String> {
    enable_raw_mode().map_err(|error| format!("could not enable terminal raw mode: {error}"))?;
    let mut terminal_mode = TerminalModeGuard {
        raw_enabled: true,
        alternate_enabled: false,
    };
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .map_err(|error| format!("could not enter alternate screen: {error}"))?;
    terminal_mode.alternate_enabled = true;

    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)
        .map_err(|error| format!("could not initialize terminal UI: {error}"))?;
    terminal
        .clear()
        .map_err(|error| format!("could not clear terminal: {error}"))?;

    let (art_tx, mut art_rx) = mpsc::unbounded_channel::<(String, RenderedArtwork)>();
    request_current_artwork(workbench, &controls);

    let loop_result = async {
        loop {
            while let Ok(update) = updates.try_recv() {
                apply_player_update(workbench, update, &picker, art_tx.clone(), &controls)?;
            }
            while let Ok((key, artwork)) = art_rx.try_recv() {
                workbench.artwork.insert(key.clone(), artwork);
                workbench.artwork_pending.remove(&key);
            }

            terminal
                .draw(|frame| draw(frame, workbench))
                .map_err(|error| format!("could not render Riff Workbench: {error}"))?;

            if event::poll(Duration::from_millis(40))
                .map_err(|error| format!("could not poll terminal input: {error}"))?
            {
                match event::read()
                    .map_err(|error| format!("could not read terminal input: {error}"))?
                {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        if handle_key(workbench, key, &controls)? {
                            break;
                        }
                    }
                    Event::Mouse(mouse) => handle_mouse(workbench, mouse, &controls)?,
                    _ => {}
                }
            }

            tokio::time::sleep(Duration::from_millis(16)).await;
        }
        Ok::<(), String>(())
    }
    .await;

    let cursor_result = terminal
        .show_cursor()
        .map_err(|error| format!("could not restore cursor: {error}"));
    drop(terminal_mode);
    loop_result.and(cursor_result)
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

    if key.modifiers.contains(KeyModifiers::ALT) {
        match key.code {
            KeyCode::Char('1') => workbench.state.view = View::NowPlaying,
            KeyCode::Char('2') => workbench.state.view = View::Search,
            KeyCode::Char('3') => workbench.state.view = View::Playlist,
            KeyCode::Char('4') => workbench.state.view = View::Lyrics,
            KeyCode::Char('5') => workbench.state.view = View::Editor,
            _ => {}
        }
        return Ok(false);
    }

    match key.code {
        KeyCode::Tab => workbench.state.view = workbench.state.view.next(),
        KeyCode::BackTab => workbench.state.view = workbench.state.view.previous(),
        KeyCode::F(6) => {
            workbench.state.theme = workbench.state.theme.next();
            workbench.state.message = format!("theme · {}", workbench.state.theme.name);
        }
        KeyCode::Char('q') | KeyCode::Esc => {
            let _ = controls.send(Control::Quit);
            return Ok(true);
        }
        KeyCode::Char(' ') => {
            let _ = controls.send(Control::Toggle);
        }
        KeyCode::Char('n') | KeyCode::Char('l') | KeyCode::Right => {
            let _ = controls.send(Control::Next);
        }
        KeyCode::Char('p') | KeyCode::Char('h') | KeyCode::Left => {
            let _ = controls.send(Control::Previous);
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            adjust_volume(workbench, VOLUME_STEP_PERCENT as i16, controls)
        }
        KeyCode::Char('-') => adjust_volume(workbench, -(VOLUME_STEP_PERCENT as i16), controls),
        KeyCode::Char(']') => seek_relative(workbench, SEEK_STEP_MS as i64, controls),
        KeyCode::Char('[') => seek_relative(workbench, -(SEEK_STEP_MS as i64), controls),
        KeyCode::Char('s') => {
            let enabled = !workbench.state.shuffle;
            let _ = controls.send(Control::Shuffle(enabled));
        }
        KeyCode::Char('r') => {
            let enabled = !workbench.state.repeat;
            let _ = controls.send(Control::Repeat(enabled));
        }
        KeyCode::Char('/') => workbench.state.view = View::Search,
        KeyCode::Char('e') => workbench.state.view = View::Editor,
        KeyCode::Char('y') => workbench.state.view = View::Lyrics,
        _ => {}
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

fn volume_percent(volume: u16) -> u8 {
    ((u32::from(volume) * 100 + u32::from(VOLUME_MAX) / 2) / u32::from(VOLUME_MAX)) as u8
}

fn volume_from_percent(percent: u8) -> u16 {
    ((u32::from(percent.min(100)) * u32::from(VOLUME_MAX) + 50) / 100) as u16
}

fn quantize_volume_percent(percent: u8) -> u8 {
    (((percent.min(100) + VOLUME_STEP_PERCENT / 2) / VOLUME_STEP_PERCENT) * VOLUME_STEP_PERCENT)
        .min(100)
}

fn set_volume_percent(
    workbench: &mut Workbench,
    percent: u8,
    controls: &mpsc::UnboundedSender<Control>,
) {
    let percent = quantize_volume_percent(percent);
    let volume = volume_from_percent(percent);
    workbench.state.volume = volume;
    let _ = controls.send(Control::SetVolume(volume));
}

fn adjust_volume(workbench: &mut Workbench, delta: i16, controls: &mpsc::UnboundedSender<Control>) {
    let current = i16::from(volume_percent(workbench.state.volume));
    let target = (current + delta).clamp(0, 100) as u8;
    set_volume_percent(workbench, target, controls);
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
            if let Some((view, _)) = workbench
                .state
                .hits
                .tabs
                .iter()
                .find(|(_, rect)| contains(*rect, point))
            {
                workbench.state.view = *view;
                return Ok(());
            }
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
                adjust_volume(workbench, VOLUME_STEP_PERCENT as i16, controls);
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
                adjust_volume(workbench, -(VOLUME_STEP_PERCENT as i16), controls);
            }
        }
        _ => {}
    }
    Ok(())
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
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(4),
            Constraint::Length(1),
        ])
        .split(frame.area());

    draw_header(frame, outer[0], workbench);
    draw_tabs(frame, outer[1], workbench);
    match workbench.state.view {
        View::NowPlaying => draw_now_playing(frame, outer[2], workbench),
        View::Search => draw_search(frame, outer[2], workbench),
        View::Playlist => draw_playlist(frame, outer[2], workbench),
        View::Lyrics => draw_lyrics(frame, outer[2], workbench),
        View::Editor => draw_editor(frame, outer[2], workbench),
    }
    draw_transport(frame, outer[3], workbench);
    draw_status(frame, outer[4], workbench);
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, workbench: &Workbench) {
    let theme = workbench.state.theme;
    let mut spans = vec![
        Span::styled(
            " RIFF ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            &workbench.state.file_name,
            Style::default().add_modifier(Modifier::BOLD),
        ),
    ];
    if let Some(git) = &workbench.state.git {
        spans.push(Span::raw("   "));
        spans.push(Span::styled(
            format!(" {}{}", git.branch, if git.dirty { " *" } else { "" }),
            Style::default().fg(theme.accent),
        ));
    }
    spans.push(Span::raw("   "));
    spans.push(Span::styled(
        format!("theme:{}", theme.name),
        Style::default().fg(theme.muted),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_tabs(frame: &mut Frame<'_>, area: Rect, workbench: &mut Workbench) {
    let theme = workbench.state.theme;
    let constraints = View::ALL.map(|_| Constraint::Ratio(1, View::ALL.len() as u32));
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);
    for (index, view) in View::ALL.iter().copied().enumerate() {
        let selected = view == workbench.state.view;
        let style = if selected {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(theme.muted)
        };
        frame.render_widget(
            Paragraph::new(view.title())
                .alignment(Alignment::Center)
                .style(style),
            columns[index],
        );
        workbench.state.hits.tabs.push((view, columns[index]));
    }
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
    } else if area.height >= 20 {
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
    frame.render_widget(TerminalImage::new(protocol).allow_clipping(true), inner);
}

fn draw_metadata(frame: &mut Frame<'_>, area: Rect, workbench: &Workbench) {
    let theme = workbench.state.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
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
    let mut lines = vec![
        Line::from(Span::styled(
            format!(
                "{}  {}",
                workbench.state.status.glyph(),
                workbench.state.status.label().to_uppercase()
            ),
            Style::default()
                .fg(theme.accent)
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
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
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
    let visible_height = columns[0].height.saturating_sub(2) as usize;
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
                    .fg(theme.foreground)
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

fn draw_playlist(frame: &mut Frame<'_>, area: Rect, workbench: &Workbench) {
    let theme = workbench.state.theme;
    let current = workbench.state.current_index();
    let items = workbench
        .state
        .queue
        .iter()
        .enumerate()
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
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border))
                .title(format!(
                    " {} · {} tracks · generated by code or TUI ",
                    workbench.state.playlist_name,
                    workbench.state.queue.len()
                )),
        ),
        area,
    );
}

fn draw_lyrics(frame: &mut Frame<'_>, area: Rect, workbench: &Workbench) {
    let theme = workbench.state.theme;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
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
    let active = workbench
        .state
        .lyrics
        .active_line(workbench.state.position_ms)
        .unwrap_or(0);
    let radius = (inner.height as usize / 2).max(2);
    let start = active.saturating_sub(radius);
    let end = (active + radius + 1).min(workbench.state.lyrics.lines.len());
    let lines = workbench.state.lyrics.lines[start..end]
        .iter()
        .enumerate()
        .flat_map(|(offset, line)| {
            let index = start + offset;
            let style = if index == active {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            };
            [
                Line::from(Span::styled(line.text.clone(), style)).alignment(Alignment::Center),
                Line::from(""),
            ]
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

fn draw_transport(frame: &mut Frame<'_>, area: Rect, workbench: &mut Workbench) {
    let theme = workbench.state.theme;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(2)])
        .split(area);
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Min(10),
            Constraint::Length(20),
        ])
        .split(rows[0]);
    let previous = Paragraph::new(" ◀ prev ")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.accent));
    let toggle_label = if workbench.state.status == PlaybackStatus::Playing {
        " Ⅱ pause "
    } else {
        " ▶ play "
    };
    let toggle = Paragraph::new(toggle_label)
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        );
    let next = Paragraph::new(" next ▶ ")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.accent));
    frame.render_widget(previous, top[0]);
    frame.render_widget(toggle, top[1]);
    frame.render_widget(next, top[2]);
    workbench.state.hits.previous = Some(top[0]);
    workbench.state.hits.toggle = Some(top[1]);
    workbench.state.hits.next = Some(top[2]);

    let flags = format!(
        "{} shuffle   {} repeat",
        if workbench.state.shuffle {
            "●"
        } else {
            "○"
        },
        if workbench.state.repeat { "●" } else { "○" }
    );
    frame.render_widget(
        Paragraph::new(flags).style(Style::default().fg(theme.muted)),
        top[3],
    );
    let volume_percent = volume_percent(workbench.state.volume);
    let volume_ratio = f64::from(volume_percent) / 100.0;
    frame.render_widget(
        Gauge::default()
            .ratio(volume_ratio)
            .label(format!("vol {volume_percent:>3}%")),
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

fn draw_status(frame: &mut Frame<'_>, area: Rect, workbench: &Workbench) {
    let theme = workbench.state.theme;
    let hint = match workbench.state.view {
        View::Search => " Enter search/play · ↑↓ select · Ctrl+A add · Ctrl+P play · Esc back ",
        View::Editor => " Ctrl+S save · Ctrl+K/U cut/paste · Ctrl+G help · Ctrl+X leave ",
        _ => {
            " Tab views · Space play/pause · h/l prev/next · +/- volume · [/] seek · s shuffle · r repeat · F6 theme · q quit "
        }
    };
    frame.render_widget(
        Paragraph::new(hint)
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme.muted)),
        area,
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
    fn formats_transport_time() {
        assert_eq!(format_duration(475_000), "7:55");
    }

    #[test]
    fn volume_math_is_quantized_and_bounded() {
        assert_eq!(volume_percent(volume_from_percent(0)), 0);
        assert_eq!(volume_percent(volume_from_percent(5)), 5);
        assert_eq!(volume_percent(volume_from_percent(95)), 95);
        assert_eq!(volume_percent(volume_from_percent(100)), 100);
        assert_eq!(quantize_volume_percent(2), 0);
        assert_eq!(quantize_volume_percent(3), 5);
        assert_eq!(quantize_volume_percent(98), 100);
    }

    #[test]
    fn rectangle_hit_test_is_half_open() {
        let rect = Rect::new(10, 10, 5, 5);
        assert!(contains(rect, (10, 10)));
        assert!(contains(rect, (14, 14)));
        assert!(!contains(rect, (15, 14)));
    }
}
