from pathlib import Path
import re


def replace(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    if old not in text:
        raise SystemExit(f"missing expected block in {path}: {old[:100]!r}")
    p.write_text(text.replace(old, new, 1))


def sub(path: str, pattern: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    updated, count = re.subn(pattern, new, text, count=1, flags=re.S)
    if count != 1:
        raise SystemExit(f"expected one regex match in {path}, got {count}: {pattern[:100]!r}")
    p.write_text(updated)


# Shared runtime cache root for safe TUI log files.
replace(
    "src/platform.rs",
    '''pub fn spotify_cache_dir() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("XDG_CACHE_HOME") {
        return Ok(PathBuf::from(path).join("riff").join("spotify"));
    }

    if cfg!(target_os = "windows") {
        if let Some(path) = env::var_os("LOCALAPPDATA") {
            return Ok(PathBuf::from(path).join("Riff").join("spotify"));
        }
        if let Some(path) = env::var_os("USERPROFILE") {
            return Ok(PathBuf::from(path)
                .join(".cache")
                .join("riff")
                .join("spotify"));
        }
    }

    if let Some(path) = env::var_os("HOME") {
        return Ok(PathBuf::from(path)
            .join(".cache")
            .join("riff")
            .join("spotify"));
    }

    Err(
        "could not determine a cache directory; set XDG_CACHE_HOME or a platform home directory"
            .to_string(),
    )
}
''',
    '''pub fn riff_cache_dir() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("XDG_CACHE_HOME") {
        return Ok(PathBuf::from(path).join("riff"));
    }

    if cfg!(target_os = "windows") {
        if let Some(path) = env::var_os("LOCALAPPDATA") {
            return Ok(PathBuf::from(path).join("Riff"));
        }
        if let Some(path) = env::var_os("USERPROFILE") {
            return Ok(PathBuf::from(path).join(".cache").join("riff"));
        }
    }

    if let Some(path) = env::var_os("HOME") {
        return Ok(PathBuf::from(path).join(".cache").join("riff"));
    }

    Err(
        "could not determine a cache directory; set XDG_CACHE_HOME or a platform home directory"
            .to_string(),
    )
}

pub fn spotify_cache_dir() -> Result<PathBuf, String> {
    Ok(riff_cache_dir()?.join("spotify"))
}
''',
)

# TUI logging goes to a file only when RIFF_LOG was explicitly requested.
replace(
    "src/player.rs",
    'use std::{collections::BTreeMap, fs};',
    'use std::{collections::BTreeMap, env, fs::{self, OpenOptions}, path::PathBuf};',
)
replace(
    "src/player.rs",
    '''pub fn init_cli_logging() {
    let env = Env::default().filter_or("RIFF_LOG", "riff=info,librespot=info");
    let _ = env_logger::Builder::from_env(env).try_init();
}
''',
    '''pub fn init_cli_logging() {
    let env = Env::default().filter_or("RIFF_LOG", "riff=info,librespot=info");
    let _ = env_logger::Builder::from_env(env).try_init();
}

pub fn init_tui_logging() -> Result<Option<PathBuf>, String> {
    if env::var_os("RIFF_LOG").is_none() {
        return Ok(None);
    }

    let log_dir = platform::riff_cache_dir()?.join("logs");
    fs::create_dir_all(&log_dir)
        .map_err(|err| format!("could not create Riff log directory: {err}"))?;
    platform::secure_cache_dir(&log_dir)?;
    let log_path = log_dir.join("tui.log");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|err| format!("could not open TUI log file: {err}"))?;

    let env = Env::default().filter_or("RIFF_LOG", "riff=debug,librespot=info");
    let mut builder = env_logger::Builder::from_env(env);
    builder
        .target(env_logger::Target::Pipe(Box::new(file)))
        .format_timestamp_millis();
    builder
        .try_init()
        .map_err(|err| format!("could not initialize TUI file logging: {err}"))?;
    log::info!("Riff Workbench logging started at {}", log_path.display());
    Ok(Some(log_path))
}
''',
)
replace(
    "src/main.rs",
    '''        let path = PathBuf::from(path);
        let source = match fs::read_to_string(&path) {
''',
    '''        if let Err(err) = riff::player::init_tui_logging() {
            eprintln!("riff: {err}");
            process::exit(1);
        }

        let path = PathBuf::from(path);
        let source = match fs::read_to_string(&path) {
''',
)

# Player task no longer exposes variable-size volume commands. The TUI sends coalesced SetVolume.
replace(
    "src/workbench/player_task.rs",
    '''    VolumeUp,
    VolumeDown,
    SetVolume(u16),
''',
    '''    SetVolume(u16),
''',
)
replace(
    "src/workbench/player_task.rs",
    '''                    Some(Control::VolumeUp) => spirc.volume_up().map_err(|err| format!("could not raise volume: {err}"))?,
                    Some(Control::VolumeDown) => spirc.volume_down().map_err(|err| format!("could not lower volume: {err}"))?,
                    Some(Control::SetVolume(volume)) => spirc.set_volume(volume).map_err(|err| format!("could not set volume: {err}"))?,
''',
    '''                    Some(Control::SetVolume(volume)) => spirc.set_volume(volume).map_err(|err| format!("could not set volume: {err}"))?,
''',
)

p = Path("src/workbench/mod.rs")
text = p.read_text()
text = text.replace(
    '''mod editor;
mod git_context;
mod model;
mod player_task;
mod theme;
''',
    '''mod actions;
mod editor;
mod git_context;
mod model;
mod player_task;
mod terminal;
mod theme;
mod volume;
''',
    1,
)
text = text.replace(
    'use std::{collections::HashMap, fs, io, path::PathBuf, sync::Arc, time::Duration};',
    'use std::{collections::HashMap, fs, io, path::PathBuf, sync::Arc, time::{Duration, Instant}};',
    1,
)
text = re.sub(
    r'use crossterm::\{.*?\};\nuse image::DynamicImage;',
    '''use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use image::DynamicImage;''',
    text,
    count=1,
    flags=re.S,
)
text = text.replace(
    '''use editor::EditorState;
use model::{AppState, HitMap, LyricsState, PlaybackStatus, QueueItem, SearchState, View};
use player_task::{Control, PlayerUpdate};
use theme::Theme;
''',
    '''use actions::{Action, from_key as action_from_key};
use editor::EditorState;
use model::{AppState, HitMap, LyricsState, PlaybackStatus, QueueItem, SearchState, View};
use player_task::{Control, PlayerUpdate};
use terminal::TerminalGuard;
use theme::Theme;
''',
    1,
)
text = text.replace(
    'const SEEK_STEP_MS: u32 = 5_000;\n',
    'const SEEK_STEP_MS: u32 = 5_000;\nconst VOLUME_FLUSH_INTERVAL: Duration = Duration::from_millis(60);\n',
    1,
)
text = text.replace(
    '''struct Workbench {
    state: AppState,
    editor: EditorState,
    artwork: HashMap<String, RenderedArtwork>,
    artwork_pending: HashMap<String, bool>,
}
''',
    '''struct Workbench {
    state: AppState,
    editor: EditorState,
    artwork: HashMap<String, RenderedArtwork>,
    artwork_pending: HashMap<String, bool>,
    pending_volume: Option<u16>,
    volume_target: Option<u16>,
    last_volume_flush: Instant,
}
''',
    1,
)

new_runtime = r'''pub async fn run(file_path: PathBuf, playlist: Playlist) -> Result<(), String> {
    if playlist.tracks.is_empty() {
        return Err("playlist has no tracks to play".to_string());
    }
    let editor = EditorState::load(&file_path)?;
    let file_name = file_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| file_path.display().to_string());

    let state = AppState {
        file_path: file_path.clone(),
        file_name,
        playlist_name: playlist.name.clone(),
        queue: Vec::new(),
        transient_current: None,
        status: PlaybackStatus::Starting,
        current_uri: None,
        position_ms: 0,
        volume: VOLUME_MAX,
        shuffle: false,
        repeat: false,
        message: "Preparing Workbench…".into(),
        view: View::NowPlaying,
        theme: Theme::from_env(),
        git: git_context::detect(&file_path),
        search: SearchState::default(),
        lyrics: LyricsState::default(),
        hits: HitMap::default(),
    };

    let mut workbench = Workbench {
        state,
        editor,
        artwork: HashMap::new(),
        artwork_pending: HashMap::new(),
        pending_volume: None,
        volume_target: None,
        last_volume_flush: Instant::now() - VOLUME_FLUSH_INTERVAL,
    };
    run_terminal(&mut workbench, playlist).await
}

async fn run_terminal(workbench: &mut Workbench, playlist: Playlist) -> Result<(), String> {
    let _terminal_guard = TerminalGuard::enter()?;
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)
        .map_err(|error| format!("could not initialize terminal UI: {error}"))?;
    terminal
        .clear()
        .map_err(|error| format!("could not clear terminal: {error}"))?;

    let mut spinner_tick = 0usize;
    let mut resolver = Box::pin(player_task::resolve_queue(&playlist));
    let queue = loop {
        terminal
            .draw(|frame| draw_loading(frame, workbench, spinner_tick))
            .map_err(|error| format!("could not render Riff startup: {error}"))?;
        tokio::select! {
            result = &mut resolver => break result?,
            _ = tokio::time::sleep(Duration::from_millis(90)) => {
                spinner_tick = spinner_tick.wrapping_add(1);
            }
        }
    };
    workbench.state.queue = queue.clone();
    workbench.state.message = "Connecting to Spotify…".into();

    let (controls, control_rx) = mpsc::unbounded_channel();
    let (update_tx, mut updates) = mpsc::unbounded_channel();
    let player_task = tokio::spawn(async move {
        if let Err(error) = player_task::run_player(queue, control_rx, update_tx.clone()).await {
            let _ = update_tx.send(PlayerUpdate::Error(error));
        }
    });

    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
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
            flush_pending_volume(workbench, &controls);

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

    let _ = controls.send(Control::Quit);
    let _ = player_task.await;
    loop_result
}

fn draw_loading(frame: &mut Frame<'_>, workbench: &Workbench, tick: usize) {
    const SPINNER: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
    let theme = workbench.state.theme;
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background).fg(theme.foreground)),
        frame.area(),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .title(" Riff Workbench ");
    let inner = block.inner(frame.area());
    frame.render_widget(block, frame.area());
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(5),
            Constraint::Percentage(40),
        ])
        .split(inner);
    let lines = vec![
        Line::from(Span::styled(
            format!("{}  preparing {}", SPINNER[tick % SPINNER.len()], workbench.state.file_name),
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!(
            "resolving {} track{} · Spotify session + metadata",
            workbench.state.playlist_name,
            if workbench.state.queue.len() == 1 { "" } else { "s" }
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme.foreground)),
        rows[1],
    );
}

fn apply_player_update'''
text, count = re.subn(
    r'pub async fn run\(.*?\nfn apply_player_update',
    new_runtime,
    text,
    count=1,
    flags=re.S,
)
if count != 1:
    raise SystemExit(f"failed to replace Workbench runtime block: {count}")

text = text.replace(
    '        PlayerUpdate::Volume(volume) => workbench.state.volume = volume,',
    '''        PlayerUpdate::Volume(volume) => {
            if let Some(target) = workbench.volume_target {
                let tolerance = volume::from_percent(1);
                if target.abs_diff(volume) <= tolerance {
                    workbench.state.volume = target;
                    workbench.volume_target = None;
                }
            } else {
                workbench.state.volume = volume;
            }
        }''',
    1,
)

new_handle = r'''fn handle_key(
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

    if let Some(action) = action_from_key(key) {
        return apply_action(workbench, action, controls);
    }
    Ok(false)
}

fn apply_action(
    workbench: &mut Workbench,
    action: Action,
    controls: &mpsc::UnboundedSender<Control>,
) -> Result<bool, String> {
    match action {
        Action::NextView => workbench.state.view = workbench.state.view.next(),
        Action::PreviousView => workbench.state.view = workbench.state.view.previous(),
        Action::CycleTheme => {
            workbench.state.theme = workbench.state.theme.next();
            workbench.state.message = format!("theme · {}", workbench.state.theme.name);
        }
        Action::Quit => {
            let _ = controls.send(Control::Quit);
            return Ok(true);
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
        Action::VolumeUp => adjust_volume(workbench, volume::STEP_PERCENT as i16),
        Action::VolumeDown => adjust_volume(workbench, -(volume::STEP_PERCENT as i16)),
        Action::SeekForward => seek_relative(workbench, SEEK_STEP_MS as i64, controls),
        Action::SeekBackward => seek_relative(workbench, -(SEEK_STEP_MS as i64), controls),
        Action::ToggleShuffle => {
            let _ = controls.send(Control::Shuffle(!workbench.state.shuffle));
        }
        Action::ToggleRepeat => {
            let _ = controls.send(Control::Repeat(!workbench.state.repeat));
        }
        Action::OpenSearch => workbench.state.view = View::Search,
        Action::OpenEditor => workbench.state.view = View::Editor,
        Action::OpenLyrics => workbench.state.view = View::Lyrics,
    }
    Ok(false)
}

fn handle_search_key'''
text, count = re.subn(
    r'fn handle_key\(.*?\nfn handle_search_key',
    new_handle,
    text,
    count=1,
    flags=re.S,
)
if count != 1:
    raise SystemExit(f"failed to replace key handler: {count}")

# Insert coalesced volume helpers before seek_relative.
marker = '''fn seek_relative(workbench: &Workbench, delta_ms: i64, controls: &mpsc::UnboundedSender<Control>) {'''
helpers = '''fn queue_volume(workbench: &mut Workbench, target: u16) {
    workbench.state.volume = target;
    workbench.pending_volume = Some(target);
    workbench.volume_target = Some(target);
}

fn adjust_volume(workbench: &mut Workbench, delta_percent: i16) {
    let target = volume::stepped(workbench.state.volume, delta_percent);
    queue_volume(workbench, target);
}

fn flush_pending_volume(workbench: &mut Workbench, controls: &mpsc::UnboundedSender<Control>) {
    if workbench.pending_volume.is_none()
        || workbench.last_volume_flush.elapsed() < VOLUME_FLUSH_INTERVAL
    {
        return;
    }
    if let Some(volume) = workbench.pending_volume.take() {
        let _ = controls.send(Control::SetVolume(volume));
        workbench.last_volume_flush = Instant::now();
    }
}

'''
if marker not in text:
    raise SystemExit("seek_relative marker missing")
text = text.replace(marker, helpers + marker, 1)

text = text.replace(
    '''                    let _ = controls.send(Control::SetVolume(
                        (VOLUME_MAX as f64 * relative.clamp(0.0, 1.0)) as u16,
                    ));''',
    '''                    queue_volume(workbench, volume::from_ratio(relative));''',
    1,
)
text = text.replace(
    '                let _ = controls.send(Control::VolumeUp);',
    '                adjust_volume(workbench, volume::STEP_PERCENT as i16);',
    1,
)
text = text.replace(
    '                let _ = controls.send(Control::VolumeDown);',
    '                adjust_volume(workbench, -(volume::STEP_PERCENT as i16));',
    1,
)

new_transport = r'''fn draw_transport(frame: &mut Frame<'_>, area: Rect, workbench: &mut Workbench) {
    let theme = workbench.state.theme;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(2)])
        .split(area);
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Min(18),
            Constraint::Length(22),
        ])
        .split(rows[0]);
    let previous = Paragraph::new(" ◀ prev ")
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.accent));
    let toggle_label = if workbench.state.status == PlaybackStatus::Playing {
        " Ⅱ pause  "
    } else {
        " ▶ play   "
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
        "{} · {} shuffle · {} repeat",
        workbench.state.status.label(),
        if workbench.state.shuffle { "on" } else { "off" },
        if workbench.state.repeat { "on" } else { "off" }
    );
    frame.render_widget(
        Paragraph::new(flags).style(Style::default().fg(theme.muted)),
        top[3],
    );
    let volume_percent = volume::percent(workbench.state.volume);
    frame.render_widget(
        Gauge::default()
            .ratio(volume_percent as f64 / 100.0)
            .label(format!("volume {volume_percent:>3}%")),
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
text, count = re.subn(
    r'fn draw_transport\(.*?\n}\n\nfn draw_status',
    new_transport + '\nfn draw_status',
    text,
    count=1,
    flags=re.S,
)
if count != 1:
    raise SystemExit(f"failed to replace transport: {count}")

text = text.replace(
    '            " Tab views · Space play/pause · h/l prev/next · +/- volume · [/] seek · s shuffle · r repeat · F6 theme · q quit "',
    '            " Tab views · h/← prev · Space play/pause · l/→ next · +/- volume 5% · [/] seek · s shuffle · r repeat · F6 theme · q quit "',
    1,
)
text, count = re.subn(
    r'\nfn restore_terminal\(.*?\n}\n(?=#\[cfg\(test\)\])',
    '\n',
    text,
    count=1,
    flags=re.S,
)
if count != 1:
    raise SystemExit(f"failed to remove restore_terminal: {count}")

p.write_text(text)

# TUI CI must keep the RAII guard and safe logging boundary.
replace(
    "scripts/check-tui-quality.sh",
    '''if grep -RIn 'env_logger' src/workbench --include='*.rs'; then
  fail 'Workbench must not initialize env_logger directly.'
fi
''',
    '''if grep -RIn 'env_logger' src/workbench --include='*.rs'; then
  fail 'Workbench must not initialize env_logger directly.'
fi

if ! grep -q 'TerminalGuard::enter' src/workbench/mod.rs; then
  fail 'Workbench must enter fullscreen mode through the crash-safe TerminalGuard.'
fi
''',
)
