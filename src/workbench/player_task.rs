use std::{
    collections::{HashMap, HashSet},
    fs,
    sync::Arc,
    time::Duration,
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
        config::{AudioFormat, PlayerConfig},
        mixer,
        mixer::MixerConfig,
        player::{Player, PlayerEvent},
    },
};
use riff::{Playlist, platform, player};
use tokio::sync::{Mutex, mpsc};

use super::model::{LyricsLine, PlaybackStatus, QueueItem};

type LyricsCache = Arc<Mutex<HashMap<String, (String, Vec<LyricsLine>)>>>;

const OAUTH_REDIRECT_URI: &str = "http://127.0.0.1:8898/login";
const OAUTH_SCOPES: &[&str] = &[
    "streaming",
    "user-read-playback-state",
    "user-modify-playback-state",
];

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
    Error(String),
}

pub async fn resolve_queue(playlist: &Playlist) -> Result<Vec<QueueItem>, String> {
    let (session_config, cache, credentials) = session_parts()?;
    let session = Session::new(session_config, Some(cache));
    session
        .connect(credentials, false)
        .await
        .map_err(|err| format!("could not connect to Spotify for TUI metadata: {err}"))?;

    let mut queue = Vec::with_capacity(playlist.tracks.len());
    for request in &playlist.tracks {
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

        queue.push(queue_item_from_uri(&session, &uri).await?);
    }

    session.shutdown();
    Ok(queue)
}

pub async fn run_player(
    queue: Vec<QueueItem>,
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
    let artwork_pending = Arc::new(Mutex::new(HashSet::<String>::new()));
    let lyrics_cache = Arc::new(Mutex::new(
        HashMap::<String, (String, Vec<LyricsLine>)>::new(),
    ));
    let lyrics_pending = Arc::new(Mutex::new(HashSet::<String>::new()));

    tokio::pin!(spirc_task);
    loop {
        tokio::select! {
            _ = &mut spirc_task => return Err("Spotify Connect stopped unexpectedly".to_string()),
            command = controls.recv() => {
                match command {
                    Some(Control::Toggle) => spirc.play_pause().map_err(|err| format!("could not toggle playback: {err}"))?,
                    Some(Control::Next) => spirc.next().map_err(|err| format!("could not skip track: {err}"))?,
                    Some(Control::Previous) => spirc.prev().map_err(|err| format!("could not go to previous track: {err}"))?,
                    Some(Control::SetVolume(volume)) => spirc.set_volume(volume).map_err(|err| format!("could not set volume: {err}"))?,
                    Some(Control::Seek(position_ms)) => spirc.set_position_ms(position_ms).map_err(|err| format!("could not seek: {err}"))?,
                    Some(Control::Shuffle(enabled)) => spirc.shuffle(enabled).map_err(|err| format!("could not change shuffle: {err}"))?,
                    Some(Control::Repeat(enabled)) => spirc.repeat(enabled).map_err(|err| format!("could not change repeat: {err}"))?,
                    Some(Control::PlayUri(uri)) => {
                        spirc.load(LoadRequest::from_tracks(vec![uri], LoadRequestOptions::default()))
                            .map_err(|err| format!("could not load selected track: {err}"))?;
                        spirc.play().map_err(|err| format!("could not play selected track: {err}"))?;
                    }
                    Some(Control::Search(query)) => spawn_search(session.clone(), query, updates.clone()),
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
            event = player_events.recv() => {
                let Some(event) = event else { continue; };
                match event {
                    PlayerEvent::Playing { track_id, position_ms, .. } => {
                        let uri = track_id.to_string();
                        let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Playing));
                        let _ = updates.send(PlayerUpdate::Track { uri: uri.clone(), position_ms });
                        if let Some(cover_id) = queue.iter().find(|item| item.uri == uri).and_then(|item| item.cover_id.clone()) {
                            request_artwork(session.clone(), uri.clone(), cover_id, updates.clone(), artwork_cache.clone(), artwork_pending.clone());
                        }
                        request_lyrics(session.clone(), uri, updates.clone(), lyrics_cache.clone(), lyrics_pending.clone());
                    }
                    PlayerEvent::Paused { track_id, position_ms, .. } => {
                        let uri = track_id.to_string();
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
                        let _ = updates.send(PlayerUpdate::Status(PlaybackStatus::Stopped));
                        let _ = updates.send(PlayerUpdate::Track { uri: track_id.to_string(), position_ms: 0 });
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

fn spawn_search(session: Session, query: String, updates: mpsc::UnboundedSender<PlayerUpdate>) {
    tokio::spawn(async move {
        let candidates = match player::search(&query, 20).await {
            Ok(candidates) => candidates,
            Err(error) => {
                let _ = updates.send(PlayerUpdate::SearchError { query, error });
                return;
            }
        };

        let mut results = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let score = candidate
                .metadata
                .get("match")
                .and_then(|value| value.parse::<u8>().ok());
            match queue_item_from_uri(&session, &candidate.uri).await {
                Ok(mut item) => {
                    item.match_score = score;
                    results.push(item);
                }
                Err(error) => {
                    let _ = updates.send(PlayerUpdate::SearchError {
                        query: query.clone(),
                        error,
                    });
                    return;
                }
            }
        }
        let _ = updates.send(PlayerUpdate::SearchResults { query, results });
    });
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
    pending: Arc<Mutex<HashSet<String>>>,
) {
    tokio::spawn(async move {
        if let Some(image) = cache.lock().await.get(&cover_id).cloned() {
            let _ = updates.send(PlayerUpdate::Artwork { key, image });
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
            let _ = updates.send(PlayerUpdate::Artwork { key, image });
        }
    });
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
}
