use std::{
    collections::{HashMap, HashSet},
    fs, io,
    sync::Arc,
    time::Duration,
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use env_logger::Env;
use image::DynamicImage;
use librespot::{
    connect::{ConnectConfig, LoadRequest, LoadRequestOptions, Spirc},
    core::{
        FileId, SpotifyUri, authentication::Credentials, cache::Cache, config::SessionConfig,
        session::Session,
    },
    metadata::{Metadata, Track as SpotifyTrack},
    oauth::OAuthClientBuilder,
    playback::{
        config::{AudioFormat, PlayerConfig},
        mixer,
        mixer::MixerConfig,
        player::{Player, PlayerEvent},
    },
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect, Size},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Padding, Paragraph, Wrap},
};
use ratatui_image::{Image as TerminalImage, Resize, picker::Picker, protocol::Protocol};
use riff::{Playlist, platform, player};
use tokio::sync::{Mutex, mpsc};

const OAUTH_REDIRECT_URI: &str = "http://127.0.0.1:8898/login";
const OAUTH_SCOPES: &[&str] = &[
    "streaming",
    "user-read-playback-state",
    "user-modify-playback-state",
];
const WIDE_ART_SIZE: Size = Size::new(30, 15);
const COMPACT_ART_SIZE: Size = Size::new(20, 10);

#[derive(Debug, Clone)]
struct QueueItem {
    title: String,
    artist: String,
    album: String,
    version: Option<String>,
    uri: String,
    duration_ms: u32,
    cover_id: Option<String>,
}

impl QueueItem {
    fn label(&self) -> String {
        format!("{} - {}", self.artist, self.title)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaybackStatus {
    Starting,
    Playing,
    Paused,
    Stopped,
}

impl PlaybackStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Playing => "playing",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::Starting => "…",
            Self::Playing => "▶",
            Self::Paused => "Ⅱ",
            Self::Stopped => "■",
        }
    }
}

struct RenderedArtwork {
    uri: String,
    wide: Protocol,
    compact: Protocol,
}

struct AppState {
    playlist_name: String,
    queue: Vec<QueueItem>,
    status: PlaybackStatus,
    current_uri: Option<String>,
    position_ms: u32,
    message: String,
    artwork: Option<RenderedArtwork>,
    artwork_pending_uri: Option<String>,
}

impl AppState {
    fn current_index(&self) -> Option<usize> {
        let uri = self.current_uri.as_deref()?;
        self.queue.iter().position(|item| item.uri == uri)
    }

    fn current(&self) -> Option<&QueueItem> {
        self.current_index().and_then(|index| self.queue.get(index))
    }
}

#[derive(Debug, Clone, Copy)]
enum Control {
    Toggle,
    Next,
    Previous,
    Quit,
}

#[derive(Debug)]
enum PlayerUpdate {
    Status(PlaybackStatus),
    Track { uri: String, position_ms: u32 },
    Position { uri: String, position_ms: u32 },
    Artwork { uri: String, image: Arc<DynamicImage> },
    Error(String),
}

pub async fn run(playlist: Playlist) -> Result<(), String> {
    let env = Env::default().filter_or("RIFF_LOG", "off");
    let _ = env_logger::Builder::from_env(env).try_init();

    if playlist.tracks.is_empty() {
        return Err("playlist has no tracks to play".to_string());
    }

    println!(
        "Resolving `{}` for the terminal player ({} tracks)...",
        playlist.name,
        playlist.tracks.len()
    );
    let queue = resolve_queue(&playlist).await?;

    let (control_tx, control_rx) = mpsc::unbounded_channel();
    let (update_tx, update_rx) = mpsc::unbounded_channel();
    let player_queue = queue.clone();
    let player_task = tokio::spawn(async move {
        if let Err(err) = run_player(player_queue, control_rx, update_tx.clone()).await {
            let _ = update_tx.send(PlayerUpdate::Error(err));
        }
    });

    let ui_result = run_terminal(playlist.name, queue, control_tx, update_rx).await;
    let _ = player_task.await;
    ui_result
}

async fn resolve_queue(playlist: &Playlist) -> Result<Vec<QueueItem>, String> {
    let (session_config, cache, credentials) = session_parts()?;
    let session = Session::new(session_config, Some(cache));
    session
        .connect(credentials, false)
        .await
        .map_err(|err| format!("could not connect to Spotify for TUI metadata: {err}"))?;

    let mut queue = Vec::with_capacity(playlist.tracks.len());
    for (index, request) in playlist.tracks.iter().enumerate() {
        println!(
            "  [{}/{}] {}",
            index + 1,
            playlist.tracks.len(),
            request.label
        );

        let uri = if let Some(uri) = request.id.as_deref() {
            uri.to_string()
        } else {
            player::search(&request.label, 1)
                .await?
                .into_iter()
                .next()
                .ok_or_else(|| format!("no confident Spotify track found for `{}`", request.label))?
                .uri
        };

        let spotify_uri = SpotifyUri::from_uri(&uri)
            .map_err(|err| format!("Spotify returned an invalid track URI `{uri}`: {err}"))?;
        let track = SpotifyTrack::get(&session, &spotify_uri)
            .await
            .map_err(|err| format!("could not load TUI metadata for `{uri}`: {err}"))?;

        let artist = track
            .artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let cover_id = track
            .album
            .covers
            .iter()
            .max_by_key(|cover| cover.width.saturating_mul(cover.height))
            .map(|cover| cover.id.to_string());
        let version = (!track.version_title.trim().is_empty()).then(|| track.version_title.clone());
        let item = QueueItem {
            title: track.name.clone(),
            artist,
            album: track.album.name.clone(),
            version,
            uri,
            duration_ms: track.duration.max(0) as u32,
            cover_id,
        };
        println!("       -> {}", item.label());
        queue.push(item);
    }

    session.shutdown();
    Ok(queue)
}

async fn run_player(
    queue: Vec<QueueItem>,
    mut controls: mpsc::UnboundedReceiver<Control>,
    updates: mpsc::UnboundedSender<PlayerUpdate>,
) -> Result<(), String> {
    let (session_config, cache, credentials) = session_parts()?;
    let player_config = PlayerConfig {
        position_update_interval: Some(Duration::from_millis(500)),
        ..PlayerConfig::default()
    };
    let audio_format = AudioFormat::default();
    let mixer_config = MixerConfig::default();
    let connect_config = ConnectConfig {
        name: "Riff".to_string(),
        ..ConnectConfig::default()
    };

    let sink_builder = platform::audio_sink_builder()?;
    let mixer_builder =
        mixer::find(None).ok_or_else(|| "no supported audio mixer was found".to_string())?;

    let session = Session::new(session_config, Some(cache));
    let mixer =
        mixer_builder(mixer_config).map_err(|err| format!("could not start mixer: {err}"))?;
    let player = Player::new(
        player_config,
        session.clone(),
        mixer.get_soft_volume(),
        move || sink_builder(None, audio_format),
    );
    let mut player_events = player.get_player_event_channel();

    let (spirc, spirc_task) =
        Spirc::new(connect_config, session.clone(), credentials, player, mixer)
            .await
            .map_err(|err| format!("could not start Spotify Connect: {err}"))?;

    spirc
        .activate()
        .map_err(|err| format!("could not activate Spotify Connect device: {err}"))?;

    let uris = queue
        .iter()
        .map(|item| item.uri.clone())
        .collect::<Vec<_>>();
    spirc
        .load(LoadRequest::from_tracks(
            uris,
            LoadRequestOptions::default(),
        ))
        .map_err(|err| format!("could not load playlist: {err}"))?;
    spirc
        .play()
        .map_err(|err| format!("could not start playlist playback: {err}"))?;
    let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Starting));

    let artwork_cache = Arc::new(Mutex::new(HashMap::<String, Arc<DynamicImage>>::new()));
    let artwork_pending = Arc::new(Mutex::new(HashSet::<String>::new()));

    tokio::pin!(spirc_task);
    loop {
        tokio::select! {
            _ = &mut spirc_task => {
                return Err("Spotify Connect stopped unexpectedly".to_string());
            }
            command = controls.recv() => {
                match command {
                    Some(Control::Toggle) => spirc.play_pause().map_err(|err| format!("could not toggle playback: {err}"))?,
                    Some(Control::Next) => spirc.next().map_err(|err| format!("could not skip track: {err}"))?,
                    Some(Control::Previous) => spirc.prev().map_err(|err| format!("could not go to previous track: {err}"))?,
                    Some(Control::Quit) | None => {
                        session.shutdown();
                        return Ok(());
                    }
                }
            }
            event = player_events.recv() => {
                let Some(event) = event else {
                    continue;
                };
                match event {
                    PlayerEvent::Playing { track_id, position_ms, .. } => {
                        let uri = track_id.to_string();
                        let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Playing));
                        let _ = updates.send(PlayerUpdate::Track {
                            uri: uri.clone(),
                            position_ms,
                        });
                        request_artwork(
                            session.clone(),
                            &queue,
                            uri,
                            updates.clone(),
                            artwork_cache.clone(),
                            artwork_pending.clone(),
                        );
                    }
                    PlayerEvent::Paused { track_id, position_ms, .. } => {
                        let uri = track_id.to_string();
                        let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Paused));
                        let _ = updates.send(PlayerUpdate::Track {
                            uri: uri.clone(),
                            position_ms,
                        });
                        request_artwork(
                            session.clone(),
                            &queue,
                            uri,
                            updates.clone(),
                            artwork_cache.clone(),
                            artwork_pending.clone(),
                        );
                    }
                    PlayerEvent::PositionChanged { track_id, position_ms, .. }
                    | PlayerEvent::PositionCorrection { track_id, position_ms, .. }
                    | PlayerEvent::Seeked { track_id, position_ms, .. } => {
                        let _ = updates.send(PlayerUpdate::Position {
                            uri: track_id.to_string(),
                            position_ms,
                        });
                    }
                    PlayerEvent::Stopped { track_id, .. } => {
                        let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Stopped));
                        let _ = updates.send(PlayerUpdate::Track {
                            uri: track_id.to_string(),
                            position_ms: 0,
                        });
                    }
                    _ => {}
                }
            }
        }
    }
}

fn request_artwork(
    session: Session,
    queue: &[QueueItem],
    uri: String,
    updates: mpsc::UnboundedSender<PlayerUpdate>,
    cache: Arc<Mutex<HashMap<String, Arc<DynamicImage>>>>,
    pending: Arc<Mutex<HashSet<String>>>,
) {
    let Some(cover_id) = queue
        .iter()
        .find(|item| item.uri == uri)
        .and_then(|item| item.cover_id.clone())
    else {
        return;
    };

    tokio::spawn(async move {
        if let Some(image) = cache.lock().await.get(&cover_id).cloned() {
            let _ = updates.send(PlayerUpdate::Artwork { uri, image });
            return;
        }

        {
            let mut pending = pending.lock().await;
            if !pending.insert(cover_id.clone()) {
                return;
            }
        }

        let loaded = async {
            let file_id = file_id_from_hex(&cover_id)?;
            let bytes = session
                .spclient()
                .get_image(&file_id)
                .await
                .map_err(|err| format!("could not download album artwork: {err}"))?;
            let bytes = bytes.to_vec();
            tokio::task::spawn_blocking(move || image::load_from_memory(&bytes))
                .await
                .map_err(|err| format!("album artwork decoder task failed: {err}"))?
                .map(Arc::new)
                .map_err(|err| format!("could not decode album artwork: {err}"))
        }
        .await;

        pending.lock().await.remove(&cover_id);
        if let Ok(image) = loaded {
            cache.lock().await.insert(cover_id, image.clone());
            let _ = updates.send(PlayerUpdate::Artwork { uri, image });
        }
    });
}

async fn run_terminal(
    playlist_name: String,
    queue: Vec<QueueItem>,
    controls: mpsc::UnboundedSender<Control>,
    mut updates: mpsc::UnboundedReceiver<PlayerUpdate>,
) -> Result<(), String> {
    enable_raw_mode().map_err(|err| format!("could not enable terminal raw mode: {err}"))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|err| format!("could not enter alternate screen: {err}"))?;

    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|err| format!("could not initialize terminal UI: {err}"))?;
    terminal
        .clear()
        .map_err(|err| format!("could not clear terminal: {err}"))?;

    let (rendered_art_tx, mut rendered_art_rx) = mpsc::unbounded_channel::<RenderedArtwork>();
    let mut state = AppState {
        playlist_name,
        queue,
        status: PlaybackStatus::Starting,
        current_uri: None,
        position_ms: 0,
        message: "Connecting to Spotify...".to_string(),
        artwork: None,
        artwork_pending_uri: None,
    };

    let loop_result = async {
        loop {
            while let Ok(update) = updates.try_recv() {
                apply_update(
                    &mut state,
                    update,
                    &picker,
                    rendered_art_tx.clone(),
                )?;
            }
            while let Ok(artwork) = rendered_art_rx.try_recv() {
                if state.current_uri.as_deref() == Some(artwork.uri.as_str()) {
                    state.artwork_pending_uri = None;
                    state.artwork = Some(artwork);
                }
            }

            terminal
                .draw(|frame| draw(frame, &state))
                .map_err(|err| format!("could not render terminal UI: {err}"))?;

            if event::poll(Duration::from_millis(50))
                .map_err(|err| format!("could not poll terminal input: {err}"))?
                && let Event::Key(key) =
                    event::read().map_err(|err| format!("could not read terminal input: {err}"))?
            {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        let _ = controls.send(Control::Quit);
                        break;
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
                    _ => {}
                }
            }

            tokio::time::sleep(Duration::from_millis(16)).await;
        }
        Ok::<(), String>(())
    }
    .await;

    let restore_result = restore_terminal(&mut terminal);
    loop_result.and(restore_result)
}

fn apply_update(
    state: &mut AppState,
    update: PlayerUpdate,
    picker: &Picker,
    rendered_art_tx: mpsc::UnboundedSender<RenderedArtwork>,
) -> Result<(), String> {
    match update {
        PlayerUpdate::Status(status) => {
            state.status = status;
            state.message = match status {
                PlaybackStatus::Starting => "Starting playback...".to_string(),
                PlaybackStatus::Playing => "space pause  h/← previous  l/→ next  q quit".to_string(),
                PlaybackStatus::Paused => {
                    "paused · space resume  h/← previous  l/→ next  q quit".to_string()
                }
                PlaybackStatus::Stopped => "Playback stopped".to_string(),
            };
        }
        PlayerUpdate::Track { uri, position_ms } => {
            if state.current_uri.as_deref() != Some(uri.as_str()) {
                state.artwork = None;
                state.artwork_pending_uri = None;
            }
            state.current_uri = Some(uri);
            state.position_ms = position_ms;
        }
        PlayerUpdate::Position { uri, position_ms } => {
            state.current_uri = Some(uri);
            state.position_ms = position_ms;
        }
        PlayerUpdate::Artwork { uri, image } => {
            if state.current_uri.as_deref() != Some(uri.as_str())
                || state.artwork_pending_uri.as_deref() == Some(uri.as_str())
            {
                return Ok(());
            }
            state.artwork_pending_uri = Some(uri.clone());
            let picker = picker.clone();
            tokio::task::spawn_blocking(move || {
                let wide = picker.new_protocol((*image).clone(), WIDE_ART_SIZE, Resize::Fit(None));
                let compact =
                    picker.new_protocol((*image).clone(), COMPACT_ART_SIZE, Resize::Fit(None));
                if let (Ok(wide), Ok(compact)) = (wide, compact) {
                    let _ = rendered_art_tx.send(RenderedArtwork { uri, wide, compact });
                }
            });
        }
        PlayerUpdate::Error(err) => return Err(err),
    }
    Ok(())
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<(), String> {
    disable_raw_mode().map_err(|err| format!("could not restore terminal mode: {err}"))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|err| format!("could not leave alternate screen: {err}"))?;
    terminal
        .show_cursor()
        .map_err(|err| format!("could not restore cursor: {err}"))
}

fn draw(frame: &mut Frame<'_>, state: &AppState) {
    let area = frame.area();
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(12),
            Constraint::Length(3),
            Constraint::Length(queue_height(area)),
            Constraint::Length(2),
        ])
        .split(area);

    draw_header(frame, outer[0], state);
    draw_player(frame, outer[1], state);
    draw_progress(frame, outer[2], state);
    draw_queue(frame, outer[3], state);
    draw_footer(frame, outer[4], state);
}

fn queue_height(area: Rect) -> u16 {
    if area.height >= 34 {
        9
    } else if area.height >= 26 {
        6
    } else {
        4
    }
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let line = Line::from(vec![
        Span::styled(" RIFF ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::raw(&state.playlist_name),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_player(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    if area.width >= 82 && area.height >= 14 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(34), Constraint::Min(32)])
            .split(area);
        draw_artwork(frame, columns[0], state, false);
        draw_metadata(frame, columns[1], state);
    } else if area.height >= 20 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(12), Constraint::Min(7)])
            .split(area);
        draw_artwork(frame, rows[0], state, true);
        draw_metadata(frame, rows[1], state);
    } else {
        draw_metadata(frame, area, state);
    }
}

fn draw_artwork(frame: &mut Frame<'_>, area: Rect, state: &AppState, compact: bool) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" album ")
        .padding(Padding::uniform(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(artwork) = state.artwork.as_ref() else {
        let placeholder = if state.artwork_pending_uri.is_some() {
            "loading artwork…"
        } else {
            "▞▚\n▚▞\n\nalbum artwork"
        };
        frame.render_widget(
            Paragraph::new(placeholder)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true }),
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

fn draw_metadata(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" now playing ")
        .padding(Padding::new(2, 2, 1, 1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(current) = state.current() else {
        frame.render_widget(
            Paragraph::new("Waiting for Spotify playback…").alignment(Alignment::Center),
            inner,
        );
        return;
    };

    let mut lines = vec![
        Line::from(Span::styled(
            format!("{}  {}", state.status.glyph(), state.status.label().to_uppercase()),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            current.title.as_str(),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(current.artist.as_str()),
        Line::from(""),
        Line::from(format!("album  {}", current.album)),
    ];
    if let Some(version) = current.version.as_deref() {
        lines.push(Line::from(format!("version  {version}")));
    }
    lines.push(Line::from(format!("track  {}", current.uri)));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn draw_progress(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let duration_ms = state.current().map(|item| item.duration_ms).unwrap_or(0);
    let ratio = if duration_ms == 0 {
        0.0
    } else {
        (state.position_ms as f64 / duration_ms as f64).clamp(0.0, 1.0)
    };
    let label = if duration_ms == 0 {
        format_duration(state.position_ms)
    } else {
        format!(
            "{}  /  {}",
            format_duration(state.position_ms),
            format_duration(duration_ms)
        )
    };
    frame.render_widget(
        Gauge::default()
            .block(Block::default().borders(Borders::TOP | Borders::BOTTOM))
            .ratio(ratio)
            .label(label),
        area,
    );
}

fn draw_queue(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let current = state.current_index();
    let items = state
        .queue
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let prefix = if Some(index) == current { "▶" } else { " " };
            let text = format!("{prefix} {:>2}. {}", index + 1, item.label());
            let style = if Some(index) == current {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
        })
        .collect::<Vec<_>>();

    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::TOP)
                .title(format!(" queue · {} tracks ", state.queue.len())),
        ),
        area,
    );
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    frame.render_widget(
        Paragraph::new(format!(" {} ", state.message)).alignment(Alignment::Center),
        area,
    );
}

fn format_duration(duration_ms: u32) -> String {
    let total_seconds = duration_ms / 1000;
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}

fn file_id_from_hex(value: &str) -> Result<FileId, String> {
    if value.len() != 40 || !value.is_ascii() {
        return Err("invalid Spotify artwork id".to_string());
    }
    let mut bytes = [0u8; 20];
    for (index, byte) in bytes.iter_mut().enumerate() {
        let start = index * 2;
        *byte = u8::from_str_radix(&value[start..start + 2], 16)
            .map_err(|_| "invalid Spotify artwork id".to_string())?;
    }
    Ok(FileId::from_raw(&bytes))
}

fn session_parts() -> Result<(SessionConfig, Cache, Credentials), String> {
    let cache_dir = platform::spotify_cache_dir()?;
    let files_dir = cache_dir.join("files");
    fs::create_dir_all(&files_dir)
        .map_err(|err| format!("could not create Spotify cache directory: {err}"))?;
    platform::secure_cache_dir(&cache_dir)?;

    let session_config = SessionConfig::default();
    let cache = Cache::new(
        Some(cache_dir.clone()),
        Some(cache_dir),
        Some(files_dir),
        None,
    )
    .map_err(|err| format!("could not initialize Spotify cache: {err}"))?;

    let credentials = match cache.credentials() {
        Some(credentials) => credentials,
        None => oauth_credentials(&session_config)?,
    };

    Ok((session_config, cache, credentials))
}

fn oauth_credentials(session_config: &SessionConfig) -> Result<Credentials, String> {
    OAuthClientBuilder::new(
        &session_config.client_id,
        OAUTH_REDIRECT_URI,
        OAUTH_SCOPES.to_vec(),
    )
    .open_in_browser()
    .with_custom_message("Riff is connected. You can close this tab and return to the terminal.")
    .build()
    .map_err(|err| format!("could not initialize Spotify OAuth: {err}"))?
    .get_access_token()
    .map(|token| Credentials::with_access_token(token.access_token))
    .map_err(|err| format!("Spotify authorization failed: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_duration_for_ui() {
        assert_eq!(format_duration(475_000), "7:55");
    }

    #[test]
    fn parses_spotify_image_file_id() {
        let id = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(file_id_from_hex(id).unwrap().to_string(), id);
    }

    #[test]
    fn rejects_invalid_spotify_image_file_id() {
        assert!(file_id_from_hex("xyz").is_err());
    }
}
