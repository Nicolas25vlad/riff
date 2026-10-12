use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use image::DynamicImage;
use librespot::{
    connect::{ConnectConfig, LoadRequest, LoadRequestOptions, Spirc},
    core::{
        FileId, SpotifyId, SpotifyUri, authentication::Credentials, cache::Cache,
        config::SessionConfig, session::Session,
    },
    metadata::{Lyrics, Metadata, Track as SpotifyTrack},
    oauth::OAuthClientBuilder,
    playback::{
        SAMPLE_RATE,
        audio_backend::{Sink, SinkResult},
        config::{AudioFormat, PlayerConfig},
        convert::Converter,
        decoder::AudioPacket,
        mixer,
        mixer::MixerConfig,
        player::{Player, PlayerEvent},
    },
};
use riff::{Playlist, platform, player, resolution_cache::ResolutionCache};
use tokio::sync::{Mutex, mpsc};

use super::model::{LyricsLine, PlaybackStatus, QueueItem};

type LyricsCache = Arc<Mutex<HashMap<String, (String, Vec<LyricsLine>)>>>;
const SEARCH_CACHE_CAPACITY: usize = 64;
const SEARCH_CACHE_TTL: Duration = Duration::from_secs(3600);

#[derive(Default)]
struct SearchResultCache(HashMap<String, (Instant, Vec<QueueItem>)>);

impl SearchResultCache {
    fn get(&mut self, query: &str) -> Option<Vec<QueueItem>> {
        self.0
            .retain(|_, (saved_at, _)| saved_at.elapsed() < SEARCH_CACHE_TTL);
        self.0.get(query).map(|(_, results)| results.clone())
    }

    fn insert(&mut self, query: String, results: Vec<QueueItem>) {
        self.0
            .retain(|_, (saved_at, _)| saved_at.elapsed() < SEARCH_CACHE_TTL);
        self.0.remove(&query);
        if self.0.len() == SEARCH_CACHE_CAPACITY {
            if let Some(oldest) = self
                .0
                .iter()
                .min_by_key(|(_, (saved_at, _))| *saved_at)
                .map(|(query, _)| query.clone())
            {
                self.0.remove(&oldest);
            }
        }
        self.0.insert(query, (Instant::now(), results));
    }
}

const OAUTH_REDIRECT_URI: &str = "http://127.0.0.1:8898/login";
const OAUTH_SCOPES: &[&str] = &[
    "streaming",
    "user-read-playback-state",
    "user-modify-playback-state",
];

#[derive(Debug)]
struct TransitionProbe {
    trigger: &'static str,
    from_uri: Option<String>,
    started_at: Instant,
}

impl TransitionProbe {
    fn new(trigger: &'static str, from_uri: Option<String>) -> Self {
        Self {
            trigger,
            from_uri,
            started_at: Instant::now(),
        }
    }

    fn elapsed_ms(&self) -> u128 {
        self.started_at.elapsed().as_millis()
    }
}

fn next_track_uri<'a>(queue: &'a [QueueItem], current_uri: &str) -> Option<&'a str> {
    queue
        .iter()
        .position(|item| item.uri == current_uri)
        .and_then(|index| queue.get(index + 1))
        .map(|item| item.uri.as_str())
}

#[derive(Debug, Clone)]
pub enum Control {
    Toggle,
    Next,
    Previous,
    SetVolume(u16),
    Seek(u32),
    Shuffle(bool),
    Repeat(bool),
    Search(String),
    PlayUri(String),
    RequestArtwork { key: String, cover_id: String },
    Quit,
}

#[derive(Debug)]
pub enum PlayerUpdate {
    Status(PlaybackStatus),
    Track {
        uri: String,
        position_ms: u32,
    },
    Position {
        uri: String,
        position_ms: u32,
    },
    Volume(u16),
    Shuffle(bool),
    Repeat(bool),
    Artwork {
        key: String,
        image: Arc<DynamicImage>,
    },
    SearchResults {
        query: String,
        results: Vec<QueueItem>,
    },
    SearchError {
        query: String,
        error: String,
    },
    Lyrics {
        uri: String,
        provider: String,
        lines: Vec<LyricsLine>,
    },
    LyricsError {
        uri: String,
        error: String,
    },
    Visualizer([u8; 32]),
    Error(String),
}

struct VisualizerSink {
    inner: Box<dyn Sink>,
    updates: mpsc::UnboundedSender<PlayerUpdate>,
    epoch: Instant,
    last_update_ms: AtomicU64,
    coefficients: [f64; VISUALIZER_BINS],
    smoothed_levels: [f64; VISUALIZER_BINS],
}

impl Sink for VisualizerSink {
    fn start(&mut self) -> SinkResult<()> {
        self.inner.start()
    }

    fn stop(&mut self) -> SinkResult<()> {
        self.inner.stop()
    }

    fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        if let Ok(samples) = packet.samples() {
            let now_ms = self.epoch.elapsed().as_millis() as u64;
            let previous = self.last_update_ms.load(Ordering::Relaxed);
            if now_ms.saturating_sub(previous) >= VISUALIZER_UPDATE_INTERVAL_MS
                && self
                    .last_update_ms
                    .compare_exchange(previous, now_ms, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
            {
                let raw_levels = visualizer_levels(samples, &self.coefficients);
                let levels = smooth_visualizer_levels(&mut self.smoothed_levels, &raw_levels);
                let _ = self.updates.send(PlayerUpdate::Visualizer(levels));
            }
        }
        self.inner.write(packet, converter)
    }
}

const VISUALIZER_BINS: usize = 32;
const SPECTRUM_WINDOW_FRAMES: usize = 2048;
const VISUALIZER_UPDATE_INTERVAL_MS: u64 = 16;
const _: () = assert!(VISUALIZER_UPDATE_INTERVAL_MS <= 16);
const MIN_FREQUENCY_HZ: f64 = 40.0;
const MAX_FREQUENCY_HZ: f64 = 16_000.0;

fn visualizer_coefficients() -> [f64; VISUALIZER_BINS] {
    std::array::from_fn(|index| {
        let position = index as f64 / (VISUALIZER_BINS - 1) as f64;
        let frequency = MIN_FREQUENCY_HZ * (MAX_FREQUENCY_HZ / MIN_FREQUENCY_HZ).powf(position);
        2.0 * (std::f64::consts::TAU * frequency / f64::from(SAMPLE_RATE)).cos()
    })
}

fn visualizer_levels(
    samples: &[f64],
    coefficients: &[f64; VISUALIZER_BINS],
) -> [u8; VISUALIZER_BINS] {
    let mut levels = [0; VISUALIZER_BINS];
    let frames = (samples.len() / 2).min(SPECTRUM_WINDOW_FRAMES);
    if frames < 2 {
        return levels;
    }

    let start_frame = samples.len() / 2 - frames;
    let mut magnitudes = [0.0; VISUALIZER_BINS];
    for (magnitude, coefficient) in magnitudes.iter_mut().zip(coefficients) {
        let (mut previous, mut previous_previous) = (0.0, 0.0);
        for frame in start_frame..start_frame + frames {
            let sample = (samples[frame * 2] + samples[frame * 2 + 1]) * 0.5;
            let current = sample + coefficient * previous - previous_previous;
            previous_previous = previous;
            previous = current;
        }
        *magnitude = (previous * previous + previous_previous * previous_previous
            - coefficient * previous * previous_previous)
            .max(0.0)
            .sqrt();
    }

    let max_magnitude = magnitudes.iter().copied().fold(0.0_f64, f64::max);
    if max_magnitude > 0.0 {
        for (level, magnitude) in levels.iter_mut().zip(magnitudes) {
            *level = (magnitude / max_magnitude * f64::from(u8::MAX)).round() as u8;
        }
    }
    levels
}
fn smooth_visualizer_levels(
    previous: &mut [f64; VISUALIZER_BINS],
    target: &[u8; VISUALIZER_BINS],
) -> [u8; VISUALIZER_BINS] {
    let mut levels = [0; VISUALIZER_BINS];
    for (index, level) in levels.iter_mut().enumerate() {
        let target = f64::from(target[index]);
        let factor = if target > previous[index] { 0.8 } else { 0.35 };
        previous[index] += (target - previous[index]) * factor;
        *level = previous[index].round() as u8;
    }
    levels
}

pub(super) async fn prepare_authentication() -> Result<(), String> {
    let (session_config, cache, credentials) = session_parts()?;
    let cache_for_check = cache.clone();
    let session = Session::new(session_config, Some(cache));
    session
        .connect(credentials, true)
        .await
        .map_err(|error| format!("could not connect to Spotify before the Workbench: {error}"))?;
    session.shutdown();
    if cache_for_check.credentials().is_none() {
        return Err("could not persist Spotify credentials for later sessions".to_string());
    }
    Ok(())
}

const METADATA_CACHE_FILE: &str = "tui-track-metadata-v1.json";

#[derive(serde::Serialize, serde::Deserialize)]
struct TrackMetadataCache {
    version: u8,
    tracks: BTreeMap<String, QueueItem>,
    #[serde(skip)]
    dirty: bool,
}

impl Default for TrackMetadataCache {
    fn default() -> Self {
        Self {
            version: 1,
            tracks: BTreeMap::new(),
            dirty: false,
        }
    }
}

impl TrackMetadataCache {
    fn load() -> Self {
        let Ok(path) = platform::spotify_cache_dir() else {
            return Self::default();
        };
        Self::load_from(&path.join(METADATA_CACHE_FILE))
    }

    fn load_from(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .filter(|cache| cache.version == 1)
            .unwrap_or_default()
    }

    fn get(&self, uri: &str) -> Option<&QueueItem> {
        self.tracks.get(uri)
    }

    fn insert(&mut self, item: QueueItem) {
        if self.tracks.get(&item.uri) != Some(&item) {
            self.tracks.insert(item.uri.clone(), item);
            self.dirty = true;
        }
    }

    fn save(&mut self) -> Result<(), String> {
        if !self.dirty {
            return Ok(());
        }
        let path = platform::spotify_cache_dir()?;
        fs::create_dir_all(&path)
            .map_err(|err| format!("could not create Spotify cache: {err}"))?;
        platform::secure_cache_dir(&path)?;
        self.save_to(&path.join(METADATA_CACHE_FILE))
            .map_err(|err| format!("could not save TUI metadata cache: {err}"))
    }

    fn save_to(&mut self, path: &Path) -> std::io::Result<()> {
        let mut cache = serde_json::to_vec(self).map_err(std::io::Error::other)?;
        cache.push(b'\n');
        fs::write(path, cache)?;
        self.dirty = false;
        Ok(())
    }
}
pub async fn resolve_queue(playlist: &Playlist) -> Result<Vec<QueueItem>, String> {
    let mut metadata_cache = TrackMetadataCache::load();
    let resolution_cache = ResolutionCache::load();
    let mut queue = vec![None; playlist.tracks.len()];
    let mut requests = Vec::new();
    let mut request_positions = Vec::new();

    for (index, track) in playlist.tracks.iter().enumerate() {
        let cached_uri = track.id.as_deref().or_else(|| {
            resolution_cache
                .as_ref()
                .and_then(|cache| cache.get("spotify", &track.label))
        });
        if let Some(item) = cached_uri.and_then(|uri| metadata_cache.get(uri)) {
            queue[index] = Some(item.clone());
        } else {
            requests.push(player::TrackRequest {
                label: track.label.clone(),
                id: track.id.clone(),
            });
            request_positions.push(index);
        }
    }

    if !requests.is_empty() {
        let resolved = player::resolve_requests(&requests).await?;
        let (session_config, cache, credentials) = session_parts()?;
        let session = Session::new(session_config, Some(cache));
        session
            .connect(credentials, true)
            .await
            .map_err(|err| format!("could not connect to Spotify for TUI metadata: {err}"))?;

        for (position, uri) in request_positions.into_iter().zip(resolved) {
            let item = queue_item_from_uri(&session, &uri).await?;
            metadata_cache.insert(item.clone());
            queue[position] = Some(item);
        }

        session.shutdown();
    }

    if let Err(err) = metadata_cache.save() {
        log::warn!("{err}");
    }

    queue
        .into_iter()
        .map(|item| item.ok_or_else(|| "track metadata was not resolved".to_string()))
        .collect()
}

pub async fn run_player(
    queue: Vec<QueueItem>,
    auto_radio: bool,
    mut controls: mpsc::UnboundedReceiver<Control>,
    updates: mpsc::UnboundedSender<PlayerUpdate>,
) -> Result<(), String> {
    let (session_config, cache, credentials) = session_parts()?;
    let player_config = PlayerConfig {
        position_update_interval: Some(Duration::from_millis(250)),
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
    let visualizer_updates = updates.clone();
    let player = Player::new(
        player_config,
        session.clone(),
        mixer.get_soft_volume(),
        move || {
            Box::new(VisualizerSink {
                inner: sink_builder(None, audio_format),
                updates: visualizer_updates,
                epoch: Instant::now(),
                last_update_ms: AtomicU64::new(0),
                coefficients: visualizer_coefficients(),
                smoothed_levels: [0.0; VISUALIZER_BINS],
            })
        },
    );
    let mut player_events = player.get_player_event_channel();
    let (spirc, spirc_task) = Spirc::new(
        connect_config,
        session.clone(),
        credentials,
        player.clone(),
        mixer,
    )
    .await
    .map_err(|err| format!("could not start Spotify Connect: {err}"))?;

    spirc
        .activate()
        .map_err(|err| format!("could not activate Spotify Connect device: {err}"))?;
    if !queue.is_empty() {
        spirc
            .load(LoadRequest::from_tracks(
                queue.iter().map(|item| item.uri.clone()).collect(),
                LoadRequestOptions::default(),
            ))
            .map_err(|err| format!("could not load playlist: {err}"))?;
        spirc
            .play()
            .map_err(|err| format!("could not start playlist playback: {err}"))?;
    }
    let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Starting));

    let artwork_cache = Arc::new(Mutex::new(HashMap::<String, Arc<DynamicImage>>::new()));
    let artwork_pending = Arc::new(Mutex::new(HashMap::<String, Vec<String>>::new()));
    let lyrics_cache = Arc::new(Mutex::new(
        HashMap::<String, (String, Vec<LyricsLine>)>::new(),
    ));
    let search_cache = Arc::new(Mutex::new(SearchResultCache::default()));
    let lyrics_pending = Arc::new(Mutex::new(HashSet::<String>::new()));
    let mut radio_last_uri = auto_radio
        .then(|| queue.last().map(|item| item.uri.clone()))
        .flatten();
    let mut radio_pending = false;
    let (radio_tx, mut radio_rx) =
        mpsc::unbounded_channel::<(String, Result<Vec<String>, String>)>();
    let mut current_uri: Option<String> = None;
    let mut transition_probe: Option<TransitionProbe> = None;
    let mut preload_started = HashMap::<String, Instant>::new();

    tokio::pin!(spirc_task);
    loop {
        tokio::select! {
            _ = &mut spirc_task => return Err("Spotify Connect stopped unexpectedly".to_string()),
            command = controls.recv() => {
                match command {
                    Some(Control::Toggle) => spirc.play_pause().map_err(|err| format!("could not toggle playback: {err}"))?,
                    Some(Control::Next) => {
                        log::debug!(
                            "playback transition command trigger=next from={}",
                            current_uri.as_deref().unwrap_or("unknown")
                        );
                        transition_probe = Some(TransitionProbe::new("next", current_uri.clone()));
                        spirc.next().map_err(|err| format!("could not skip track: {err}"))?;
                    }
                    Some(Control::Previous) => {
                        log::debug!(
                            "playback transition command trigger=previous from={}",
                            current_uri.as_deref().unwrap_or("unknown")
                        );
                        transition_probe = Some(TransitionProbe::new("previous", current_uri.clone()));
                        spirc.prev().map_err(|err| format!("could not go to previous track: {err}"))?;
                    }
                    Some(Control::SetVolume(volume)) => spirc.set_volume(volume).map_err(|err| format!("could not set volume: {err}"))?,
                    Some(Control::Seek(position_ms)) => spirc.set_position_ms(position_ms).map_err(|err| format!("could not seek: {err}"))?,
                    Some(Control::Shuffle(enabled)) => spirc.shuffle(enabled).map_err(|err| format!("could not change shuffle: {err}"))?,
                    Some(Control::Repeat(enabled)) => spirc.repeat(enabled).map_err(|err| format!("could not change repeat: {err}"))?,
                    Some(Control::PlayUri(uri)) => {
                        radio_last_uri = Some(uri.clone());
                        radio_pending = false;
                        log::debug!(
                            "playback transition command trigger=play-uri from={} target={uri}",
                            current_uri.as_deref().unwrap_or("unknown")
                        );
                        transition_probe = Some(TransitionProbe::new("play-uri", current_uri.clone()));
                        spirc.load(LoadRequest::from_tracks(vec![uri], LoadRequestOptions::default()))
                            .map_err(|err| format!("could not load selected track: {err}"))?;
                        spirc.play().map_err(|err| format!("could not play selected track: {err}"))?;
                    }
                    Some(Control::Search(query)) => spawn_search(
                        session.clone(),
                        query,
                        updates.clone(),
                        search_cache.clone(),
                    ),
                    Some(Control::RequestArtwork { key, cover_id }) => request_artwork(
                        session.clone(), key, cover_id, updates.clone(), artwork_cache.clone(), artwork_pending.clone(),
                    ),
                    Some(Control::Quit) | None => {
                        let _ = spirc.shutdown();
                        session.shutdown();
                        return Ok(());
                    }
                }
            }
            Some((seed_uri, result)) = radio_rx.recv() => {
                radio_pending = false;
                if radio_last_uri.as_deref() == Some(seed_uri.as_str()) {
                    match result {
                        Ok(tracks) => {
                            let tracks = tracks.into_iter().filter(|uri| uri != &seed_uri).collect::<Vec<_>>();
                            if let Some(last_uri) = tracks.last() {
                                radio_last_uri = Some(last_uri.clone());
                                if let Err(error) = spirc.load(LoadRequest::from_tracks(tracks, LoadRequestOptions::default())) {
                                    log::warn!("could not load Spotify radio: {error}");
                                    radio_last_uri = None;
                                } else if let Err(error) = spirc.play() {
                                    log::warn!("could not play Spotify radio: {error}");
                                    radio_last_uri = None;
                                }
                            } else {
                                radio_last_uri = None;
                            }
                        }
                        Err(error) => {
                            log::warn!("could not continue playback with Spotify radio: {error}");
                            radio_last_uri = None;
                        }
                    }
                }
            }
            event = player_events.recv() => {
                let Some(event) = event else { continue; };
                match event {
                    PlayerEvent::Playing { track_id, position_ms, .. } => {
                        let uri = track_id.to_string();
                        if let Some(probe) = transition_probe.take() {
                            log::debug!(
                                "playback transition event=playing trigger={} from={} target={} elapsed_ms={}",
                                probe.trigger,
                                probe.from_uri.as_deref().unwrap_or("unknown"),
                                uri,
                                probe.elapsed_ms()
                            );
                        }
                        if let Some(started_at) = preload_started.remove(&uri) {
                            log::debug!(
                                "playback transition preload_to_play target={} elapsed_ms={}",
                                uri,
                                started_at.elapsed().as_millis()
                            );
                        }
                        current_uri = Some(uri.clone());
                        // shortcut: one decoded preload slot; build a queue cache if multiple tracks must be ready at once.
                        if let Some(next_uri) = next_track_uri(&queue, &uri) {
                            match SpotifyUri::from_uri(next_uri) {
                                Ok(track_id) => player.preload(track_id),
                                Err(err) => log::debug!("skipping preload for invalid URI {next_uri}: {err}"),
                            }
                        }
                        let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Playing));
                        let _ = updates.send(PlayerUpdate::Track { uri: uri.clone(), position_ms });
                        if let Some(cover_id) = queue.iter().find(|item| item.uri == uri).and_then(|item| item.cover_id.clone()) {
                            request_artwork(session.clone(), uri.clone(), cover_id, updates.clone(), artwork_cache.clone(), artwork_pending.clone());
                        }
                        request_lyrics(session.clone(), uri, updates.clone(), lyrics_cache.clone(), lyrics_pending.clone());
                    }
                    PlayerEvent::Paused { track_id, position_ms, .. } => {
                        let uri = track_id.to_string();
                        current_uri = Some(uri.clone());
                        let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Paused));
                        let _ = updates.send(PlayerUpdate::Track { uri: uri.clone(), position_ms });
                        request_lyrics(session.clone(), uri, updates.clone(), lyrics_cache.clone(), lyrics_pending.clone());
                    }
                    PlayerEvent::PositionChanged { track_id, position_ms, .. }
                    | PlayerEvent::PositionCorrection { track_id, position_ms, .. }
                    | PlayerEvent::Seeked { track_id, position_ms, .. } => {
                        let _ = updates.send(PlayerUpdate::Position { uri: track_id.to_string(), position_ms });
                    }
                    PlayerEvent::Stopped { track_id, .. } => {
                        let uri = track_id.to_string();
                        log::debug!("playback transition event=stopped track={uri}");
                        current_uri = Some(uri.clone());
                        let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Stopped));
                        let _ = updates.send(PlayerUpdate::Track { uri, position_ms: 0 });
                    }
                    PlayerEvent::Preloading { track_id } => {
                        let uri = track_id.to_string();
                        preload_started.retain(|_, started_at| {
                            started_at.elapsed() < Duration::from_secs(180)
                        });
                        preload_started.insert(uri.clone(), Instant::now());
                        log::debug!("playback transition event=preloading target={uri}");
                    }
                    PlayerEvent::TimeToPreloadNextTrack { track_id, .. } => {
                        log::debug!(
                            "playback transition event=time-to-preload current={}",
                            track_id
                        );
                    }
                    PlayerEvent::Loading { track_id, position_ms, .. } => {
                        let uri = track_id.to_string();
                        if let Some(probe) = transition_probe.as_ref() {
                            log::debug!(
                                "playback transition event=loading trigger={} target={} position_ms={} elapsed_ms={}",
                                probe.trigger,
                                uri,
                                position_ms,
                                probe.elapsed_ms()
                            );
                        } else {
                            log::debug!(
                                "playback transition event=loading trigger=unknown target={} position_ms={}",
                                uri,
                                position_ms
                            );
                        }
                    }
                    PlayerEvent::EndOfTrack { track_id, .. } => {
                        let uri = track_id.to_string();
                        log::debug!("playback transition event=end-of-track current={uri}");
                        if transition_probe.is_none() {
                            transition_probe = Some(TransitionProbe::new("automatic", Some(uri.clone())));
                        }
                        if !radio_pending && radio_last_uri.as_deref() == Some(uri.as_str()) {
                            radio_pending = true;
                            let session = session.clone();
                            let sender = radio_tx.clone();
                            let seed_uri = uri.clone();
                            tokio::spawn(async move {
                                let result =
                                    player::radio_tracks_with_session(&session, &seed_uri).await;
                                let _ = sender.send((seed_uri, result));
                            });
                        }
                    }
                    PlayerEvent::Unavailable { track_id, .. } => {
                        let uri = track_id.to_string();
                        preload_started.remove(&uri);
                        if let Some(probe) = transition_probe.take() {
                            log::debug!(
                                "playback transition event=unavailable trigger={} track={} elapsed_ms={}",
                                probe.trigger,
                                uri,
                                probe.elapsed_ms()
                            );
                        } else {
                            log::debug!("playback transition event=unavailable track={uri}");
                        }
                    }
                    PlayerEvent::TrackChanged { audio_item } => {
                        log::debug!("playback transition event=track-changed track={}", audio_item.uri);
                    }
                    PlayerEvent::VolumeChanged { volume } => { let _ = updates.send(PlayerUpdate::Volume(volume)); }
                    PlayerEvent::ShuffleChanged { shuffle } => { let _ = updates.send(PlayerUpdate::Shuffle(shuffle)); }
                    PlayerEvent::RepeatChanged { context, .. } => { let _ = updates.send(PlayerUpdate::Repeat(context)); }
                    _ => {}
                }
            }
        }
    }
}

fn spawn_search(
    session: Session,
    query: String,
    updates: mpsc::UnboundedSender<PlayerUpdate>,
    cache: Arc<Mutex<SearchResultCache>>,
) {
    tokio::spawn(async move {
        if let Some(results) = cache.lock().await.get(&query) {
            let _ = updates.send(PlayerUpdate::SearchResults { query, results });
            return;
        }

        let candidates = match player::search_tracks_with_session(&session, &query, 50).await {
            Ok(candidates) => candidates,
            Err(error) => {
                let _ = updates.send(PlayerUpdate::SearchError { query, error });
                return;
            }
        };
        let results = match queue_items_from_candidates(candidates) {
            Ok(results) => results,
            Err(error) => {
                let _ = updates.send(PlayerUpdate::SearchError { query, error });
                return;
            }
        };
        cache.lock().await.insert(query.clone(), results.clone());
        let _ = updates.send(PlayerUpdate::SearchResults { query, results });
    });
}

fn queue_items_from_candidates(
    candidates: Vec<player::SearchCandidate>,
) -> Result<Vec<QueueItem>, String> {
    candidates
        .into_iter()
        .map(queue_item_from_candidate)
        .collect()
}

fn queue_item_from_candidate(candidate: player::SearchCandidate) -> Result<QueueItem, String> {
    let title = candidate
        .metadata
        .get("title")
        .cloned()
        .ok_or_else(|| format!("Spotify search result `{}` has no title", candidate.uri))?;
    let artist = candidate
        .metadata
        .get("artist")
        .cloned()
        .ok_or_else(|| format!("Spotify search result `{}` has no artist", candidate.uri))?;
    let album = candidate
        .metadata
        .get("album")
        .cloned()
        .ok_or_else(|| format!("Spotify search result `{}` has no album", candidate.uri))?;
    let duration_ms = candidate
        .metadata
        .get("duration_ms")
        .and_then(|duration| duration.parse::<u32>().ok())
        .ok_or_else(|| {
            format!(
                "Spotify search result `{}` has invalid duration",
                candidate.uri
            )
        })?;

    Ok(QueueItem {
        title,
        artist,
        album,
        version: candidate.metadata.get("version").cloned(),
        uri: candidate.uri,
        duration_ms,
        cover_id: candidate.metadata.get("cover_id").cloned(),
        match_score: None,
    })
}

async fn queue_item_from_uri(session: &Session, uri: &str) -> Result<QueueItem, String> {
    let spotify_uri = SpotifyUri::from_uri(uri)
        .map_err(|err| format!("Spotify returned an invalid track URI `{uri}`: {err}"))?;
    let track = SpotifyTrack::get(session, &spotify_uri)
        .await
        .map_err(|err| format!("could not load metadata for `{uri}`: {err}"))?;
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

    Ok(QueueItem {
        title: track.name.clone(),
        artist,
        album: track.album.name.clone(),
        version,
        uri: uri.to_string(),
        duration_ms: track.duration.max(0) as u32,
        cover_id,
        match_score: None,
    })
}

fn request_artwork(
    session: Session,
    key: String,
    cover_id: String,
    updates: mpsc::UnboundedSender<PlayerUpdate>,
    cache: Arc<Mutex<HashMap<String, Arc<DynamicImage>>>>,
    pending: Arc<Mutex<HashMap<String, Vec<String>>>>,
) {
    tokio::spawn(async move {
        if let Some(image) = cache.lock().await.get(&cover_id).cloned() {
            let _ = updates.send(PlayerUpdate::Artwork { key, image });
            return;
        }
        {
            let mut pending = pending.lock().await;
            if !register_artwork_request(&mut pending, &cover_id, key) {
                return;
            }
        }

        let loaded = async {
            let file_id = file_id_from_hex(&cover_id)?;
            let cache_path = platform::spotify_cache_dir()
                .ok()
                .map(|directory| directory.join("artwork").join(format!("{cover_id}.img")));
            if let Some(path) = cache_path.as_ref() {
                let path = path.to_path_buf();
                if let Some(image) = tokio::task::spawn_blocking(move || load_cached_artwork(&path))
                    .await
                    .map_err(|err| format!("album artwork cache task failed: {err}"))?
                {
                    return Ok(image);
                }
            }

            let bytes = session
                .spclient()
                .get_image(&file_id)
                .await
                .map_err(|err| format!("could not download album artwork: {err}"))?
                .to_vec();
            tokio::task::spawn_blocking(move || {
                let image = image::load_from_memory(&bytes)
                    .map_err(|err| format!("could not decode album artwork: {err}"))?;
                if let Some(path) = cache_path
                    && let Err(error) = save_cached_artwork(&path, &bytes)
                {
                    log::debug!("could not cache album artwork: {error}");
                }
                Ok::<_, String>(Arc::new(image))
            })
            .await
            .map_err(|err| format!("album artwork decoder task failed: {err}"))?
        }
        .await;

        let keys = pending.lock().await.remove(&cover_id).unwrap_or_default();
        if let Ok(image) = loaded {
            cache.lock().await.insert(cover_id, image.clone());
            for key in keys {
                let _ = updates.send(PlayerUpdate::Artwork {
                    key,
                    image: image.clone(),
                });
            }
        }
    });
}

fn register_artwork_request(
    pending: &mut HashMap<String, Vec<String>>,
    cover_id: &str,
    key: String,
) -> bool {
    if let Some(keys) = pending.get_mut(cover_id) {
        keys.push(key);
        false
    } else {
        pending.insert(cover_id.to_string(), vec![key]);
        true
    }
}

fn load_cached_artwork(path: &Path) -> Option<Arc<DynamicImage>> {
    fs::read(path)
        .ok()
        .and_then(|bytes| image::load_from_memory(&bytes).ok())
        .map(Arc::new)
}

fn save_cached_artwork(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "album artwork cache has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|err| format!("could not create album artwork cache: {err}"))?;
    platform::secure_cache_dir(parent)?;
    fs::write(path, bytes).map_err(|err| format!("could not save album artwork cache: {err}"))
}

fn request_lyrics(
    session: Session,
    uri: String,
    updates: mpsc::UnboundedSender<PlayerUpdate>,
    cache: LyricsCache,
    pending: Arc<Mutex<HashSet<String>>>,
) {
    tokio::spawn(async move {
        if let Some((provider, lines)) = cache.lock().await.get(&uri).cloned() {
            let _ = updates.send(PlayerUpdate::Lyrics {
                uri,
                provider,
                lines,
            });
            return;
        }
        {
            let mut pending = pending.lock().await;
            if !pending.insert(uri.clone()) {
                return;
            }
        }
        let result = async {
            let spotify_uri = SpotifyUri::from_uri(&uri)
                .map_err(|err| format!("invalid track URI for lyrics: {err}"))?;
            let spotify_id = SpotifyId::try_from(&spotify_uri)
                .map_err(|err| format!("invalid track ID for lyrics: {err}"))?;
            let lyrics = Lyrics::get(&session, &spotify_id)
                .await
                .map_err(|err| format!("lyrics unavailable: {err}"))?;
            let provider = lyrics.lyrics.provider_display_name.clone();
            let lines = lyrics
                .lyrics
                .lines
                .into_iter()
                .map(|line| LyricsLine {
                    start_ms: line.start_time_ms.parse::<u32>().unwrap_or(0),
                    end_ms: line.end_time_ms.parse::<u32>().unwrap_or(0),
                    text: line.words,
                })
                .collect::<Vec<_>>();
            Ok::<_, String>((provider, lines))
        }
        .await;
        pending.lock().await.remove(&uri);
        match result {
            Ok((provider, lines)) => {
                cache
                    .lock()
                    .await
                    .insert(uri.clone(), (provider.clone(), lines.clone()));
                let _ = updates.send(PlayerUpdate::Lyrics {
                    uri,
                    provider,
                    lines,
                });
            }
            Err(error) => {
                let _ = updates.send(PlayerUpdate::LyricsError { uri, error });
            }
        }
    });
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
    fn parses_spotify_image_file_id() {
        let id = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(file_id_from_hex(id).unwrap().to_string(), id);
    }

    #[test]
    fn persists_track_metadata_for_startup_reuse() {
        let path =
            std::env::temp_dir().join(format!("riff-track-metadata-{}.json", std::process::id()));
        let mut cache = TrackMetadataCache::default();
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
        cache.insert(item.clone());
        cache.save_to(&path).unwrap();

        assert_eq!(
            TrackMetadataCache::load_from(&path).get(&item.uri),
            Some(&item)
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn visualizer_shows_logarithmic_spectrum_and_normalizes_master_volume() {
        let coefficients = visualizer_coefficients();
        let peak_bin = 13;
        let frequency = MIN_FREQUENCY_HZ
            * (MAX_FREQUENCY_HZ / MIN_FREQUENCY_HZ)
                .powf(peak_bin as f64 / (VISUALIZER_BINS - 1) as f64);
        let loud: Vec<_> = (0..SPECTRUM_WINDOW_FRAMES)
            .flat_map(|frame| {
                let sample = 0.5
                    * (std::f64::consts::TAU * frequency * frame as f64 / f64::from(SAMPLE_RATE))
                        .sin();
                [sample, sample]
            })
            .collect();
        let quiet: Vec<_> = loud.iter().map(|sample| sample * 0.01).collect();
        let levels = visualizer_levels(&loud, &coefficients);
        let quiet_levels = visualizer_levels(&quiet, &coefficients);

        assert_eq!(levels[peak_bin], u8::MAX);
        assert!(
            levels
                .iter()
                .zip(quiet_levels)
                .all(|(loud, quiet)| loud.abs_diff(quiet) <= 1)
        );
        assert_eq!(visualizer_levels(&[0.0; 64], &coefficients), [0; 32]);
    }
    #[test]
    fn visualizer_attacks_fast_and_releases_smoothly() {
        let mut previous = [0.0; VISUALIZER_BINS];
        let attack = smooth_visualizer_levels(&mut previous, &[u8::MAX; VISUALIZER_BINS]);
        let release = smooth_visualizer_levels(&mut previous, &[0; VISUALIZER_BINS]);

        assert_eq!(attack[0], 204);
        assert_eq!(release[0], 133);
    }
    #[test]
    fn search_cache_reuses_results_and_evicts_the_oldest_entry_at_capacity() {
        let item = |uri: String| QueueItem {
            title: "Title".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            version: None,
            uri,
            duration_ms: 0,
            cover_id: None,
            match_score: None,
        };
        let mut cache = SearchResultCache::default();
        cache.insert("first".into(), vec![item("first-result".into())]);

        assert_eq!(cache.get("first").unwrap()[0].uri, "first-result");
        for index in 0..SEARCH_CACHE_CAPACITY {
            let query = format!("query-{index}");
            cache.insert(query.clone(), vec![item(query)]);
        }

        assert!(cache.get("first").is_none());
        assert_eq!(cache.get("query-0").unwrap()[0].uri, "query-0");
    }
    #[test]
    fn workbench_search_preserves_spotify_order_and_track_metadata() {
        let candidate = |uri: &str, title: &str, score: &str| player::SearchCandidate {
            uri: uri.into(),
            metadata: BTreeMap::from([
                ("title".into(), title.into()),
                ("artist".into(), "Artist".into()),
                ("album".into(), "Album".into()),
                ("duration_ms".into(), "123000".into()),
                ("match".into(), score.into()),
            ]),
        };

        let results = queue_items_from_candidates(vec![
            candidate("spotify:track:first", "First result", "10"),
            candidate("spotify:track:second", "Second result", "99"),
        ])
        .unwrap();

        assert_eq!(
            results
                .iter()
                .map(|item| item.uri.as_str())
                .collect::<Vec<_>>(),
            ["spotify:track:first", "spotify:track:second"]
        );
        assert_eq!(results[0].title, "First result");
        assert_eq!(results[0].duration_ms, 123_000);
        assert!(results.iter().all(|item| item.match_score.is_none()));
    }
    #[test]
    fn queues_all_requests_for_the_same_pending_cover() {
        let mut pending = HashMap::new();

        assert!(register_artwork_request(
            &mut pending,
            "cover",
            "track-one".into()
        ));
        assert!(!register_artwork_request(
            &mut pending,
            "cover",
            "track-two".into()
        ));
        assert_eq!(pending.remove("cover").unwrap(), ["track-one", "track-two"]);
    }

    #[test]
    fn persists_and_reloads_decodable_artwork() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let directory = std::env::temp_dir().join(format!(
            "riff-artwork-cache-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = directory.join("artwork/cover.img");
        let mut bytes = Vec::new();
        DynamicImage::new_rgb8(2, 2)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();

        save_cached_artwork(&path, &bytes).unwrap();

        assert!(load_cached_artwork(&path).is_some());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn finds_only_the_track_immediately_after_the_current_queue_item() {
        let item = |uri: &str| QueueItem {
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            version: None,
            uri: uri.to_string(),
            duration_ms: 0,
            cover_id: None,
            match_score: None,
        };
        let queue = vec![item("spotify:track:1"), item("spotify:track:2")];

        assert_eq!(
            next_track_uri(&queue, "spotify:track:1"),
            Some("spotify:track:2")
        );
        assert_eq!(next_track_uri(&queue, "spotify:track:2"), None);
        assert_eq!(next_track_uri(&queue, "spotify:track:3"), None);
    }
}
