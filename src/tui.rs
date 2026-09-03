use std::{env, fs, io, path::PathBuf, time::Duration};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use env_logger::Env;
use librespot::{
    connect::{ConnectConfig, LoadRequest, LoadRequestOptions, Spirc},
    core::{authentication::Credentials, cache::Cache, config::SessionConfig, session::Session},
    oauth::OAuthClientBuilder,
    playback::{
        audio_backend,
        config::{AudioFormat, PlayerConfig},
        mixer,
        mixer::MixerConfig,
        player::{Player, PlayerEvent},
    },
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Wrap},
};
use riff::{Playlist, player};
use tokio::sync::mpsc;

const OAUTH_REDIRECT_URI: &str = "http://127.0.0.1:8898/login";
const OAUTH_SCOPES: &[&str] = &[
    "streaming",
    "user-read-playback-state",
    "user-modify-playback-state",
];

#[derive(Debug, Clone)]
struct QueueItem {
    label: String,
    uri: String,
    duration_ms: u32,
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
}

#[derive(Debug, Clone)]
struct AppState {
    playlist_name: String,
    queue: Vec<QueueItem>,
    status: PlaybackStatus,
    current_uri: Option<String>,
    position_ms: u32,
    message: String,
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
    let mut queue = Vec::with_capacity(playlist.tracks.len());

    for (index, track) in playlist.tracks.iter().enumerate() {
        println!(
            "  [{}/{}] {}",
            index + 1,
            playlist.tracks.len(),
            track.label
        );

        let candidate = if let Some(uri) = track.id.as_deref() {
            player::inspect_track(uri).await?
        } else {
            player::search(&track.label, 1)
                .await?
                .into_iter()
                .next()
                .ok_or_else(|| format!("no confident Spotify track found for `{}`", track.label))?
        };

        let duration_ms = candidate
            .metadata
            .get("duration")
            .and_then(|value| parse_duration_ms(value))
            .unwrap_or(0);

        println!("       -> {}", candidate.display_name());
        queue.push(QueueItem {
            label: candidate.display_name(),
            uri: candidate.uri,
            duration_ms,
        });
    }

    Ok(queue)
}

fn parse_duration_ms(value: &str) -> Option<u32> {
    let mut parts = value.split(':');
    let minutes = parts.next()?.parse::<u32>().ok()?;
    let seconds = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || seconds >= 60 {
        return None;
    }
    Some((minutes * 60 + seconds) * 1000)
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

    let sink_builder = audio_backend::find(None)
        .ok_or_else(|| "no supported audio backend was found".to_string())?;
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
                        let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Playing));
                        let _ = updates.send(PlayerUpdate::Track {
                            uri: track_id.to_string(),
                            position_ms,
                        });
                    }
                    PlayerEvent::Paused { track_id, position_ms, .. } => {
                        let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Paused));
                        let _ = updates.send(PlayerUpdate::Track {
                            uri: track_id.to_string(),
                            position_ms,
                        });
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

    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|err| format!("could not initialize terminal UI: {err}"))?;
    terminal
        .clear()
        .map_err(|err| format!("could not clear terminal: {err}"))?;

    let mut state = AppState {
        playlist_name,
        queue,
        status: PlaybackStatus::Starting,
        current_uri: None,
        position_ms: 0,
        message: "Connecting to Spotify...".to_string(),
    };

    let loop_result = async {
        loop {
            while let Ok(update) = updates.try_recv() {
                apply_update(&mut state, update)?;
            }

            terminal
                .draw(|frame| draw(frame, &state))
                .map_err(|err| format!("could not render terminal UI: {err}"))?;

            if event::poll(Duration::from_millis(50))
                .map_err(|err| format!("could not poll terminal input: {err}"))?
            {
                if let Event::Key(key) =
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
                        KeyCode::Char('n') | KeyCode::Right => {
                            let _ = controls.send(Control::Next);
                        }
                        KeyCode::Char('p') | KeyCode::Left => {
                            let _ = controls.send(Control::Previous);
                        }
                        _ => {}
                    }
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

fn apply_update(state: &mut AppState, update: PlayerUpdate) -> Result<(), String> {
    match update {
        PlayerUpdate::Status(status) => {
            state.status = status;
            state.message = match status {
                PlaybackStatus::Starting => "Starting playback...".to_string(),
                PlaybackStatus::Playing => "Space pause  n next  p previous  q quit".to_string(),
                PlaybackStatus::Paused => {
                    "Paused. Space resume  n next  p previous  q quit".to_string()
                }
                PlaybackStatus::Stopped => "Playback stopped".to_string(),
            };
        }
        PlayerUpdate::Track { uri, position_ms } | PlayerUpdate::Position { uri, position_ms } => {
            state.current_uri = Some(uri);
            state.position_ms = position_ms;
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
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);

    let title = Paragraph::new(Line::from(vec![
        Span::styled(" RIFF ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(format!("  {}", state.playlist_name)),
    ]))
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(title, outer[0]);

    if area.width >= 82 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(outer[1]);
        draw_now_playing(frame, columns[0], state);
        draw_queue(frame, columns[1], state);
    } else {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(8), Constraint::Min(5)])
            .split(outer[1]);
        draw_now_playing(frame, rows[0], state);
        draw_queue(frame, rows[1], state);
    }

    let footer = Paragraph::new(state.message.as_str())
        .block(Block::default().borders(Borders::ALL).title(" controls "));
    frame.render_widget(footer, outer[2]);
}

fn draw_now_playing(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(area);

    let current = state
        .current()
        .map(|item| item.label.as_str())
        .unwrap_or("Waiting for playback...");
    let now = Paragraph::new(vec![
        Line::from(Span::styled(
            state.status.label().to_uppercase(),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(current),
    ])
    .wrap(Wrap { trim: true })
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" now playing "),
    );
    frame.render_widget(now, rows[0]);

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
            "{} / {}",
            format_duration(state.position_ms),
            format_duration(duration_ms)
        )
    };
    let progress = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" progress "))
        .ratio(ratio)
        .label(label);
    frame.render_widget(progress, rows[1]);

    let hint = Paragraph::new("Space play/pause   ←/p previous   →/n next   q quit");
    frame.render_widget(hint, rows[2]);
}

fn draw_queue(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let current = state.current_index();
    let items = state
        .queue
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let prefix = if Some(index) == current { "▶" } else { " " };
            let text = format!("{prefix} {:>2}. {}", index + 1, item.label);
            let style = if Some(index) == current {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
        })
        .collect::<Vec<_>>();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" queue · {} tracks ", state.queue.len())),
    );
    frame.render_widget(list, area);
}

fn format_duration(duration_ms: u32) -> String {
    let total_seconds = duration_ms / 1000;
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}

fn session_parts() -> Result<(SessionConfig, Cache, Credentials), String> {
    let cache_dir = cache_dir()?;
    let files_dir = cache_dir.join("files");
    fs::create_dir_all(&files_dir)
        .map_err(|err| format!("could not create Spotify cache directory: {err}"))?;
    secure_cache_dir(&cache_dir)?;

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

fn cache_dir() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("XDG_CACHE_HOME") {
        return Ok(PathBuf::from(path).join("riff").join("spotify"));
    }

    let home = env::var_os("HOME").ok_or_else(|| {
        "HOME is not set; set HOME or XDG_CACHE_HOME so Riff can store Spotify credentials"
            .to_string()
    })?;

    Ok(PathBuf::from(home)
        .join(".cache")
        .join("riff")
        .join("spotify"))
}

#[cfg(unix)]
fn secure_cache_dir(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .map_err(|err| format!("could not inspect Spotify cache permissions: {err}"))?
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
        .map_err(|err| format!("could not secure Spotify cache directory: {err}"))
}

#[cfg(not(unix))]
fn secure_cache_dir(_path: &std::path::Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_duration() {
        assert_eq!(parse_duration_ms("7:55"), Some(475_000));
        assert_eq!(parse_duration_ms("7:99"), None);
    }

    #[test]
    fn formats_duration_for_ui() {
        assert_eq!(format_duration(475_000), "7:55");
    }
}
