use std::{collections::BTreeMap, env, fs, path::PathBuf};

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
        player::Player,
    },
};

const OAUTH_REDIRECT_URI: &str = "http://127.0.0.1:8898/login";
const OAUTH_SCOPES: &[&str] = &[
    "streaming",
    "user-read-playback-state",
    "user-modify-playback-state",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackRequest {
    pub label: String,
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchCandidate {
    pub uri: String,
    pub metadata: BTreeMap<String, String>,
}

impl SearchCandidate {
    pub fn display_name(&self) -> String {
        for key in ["title", "name", "track_name"] {
            if let Some(value) = self.metadata.get(key) {
                return value.clone();
            }
        }
        self.uri.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerOptions {
    pub context_uri: Option<String>,
    pub tracks: Vec<TrackRequest>,
    pub device_name: String,
}

impl Default for PlayerOptions {
    fn default() -> Self {
        Self {
            context_uri: None,
            tracks: Vec::new(),
            device_name: "Riff".to_string(),
        }
    }
}

pub async fn run(options: PlayerOptions) -> Result<(), String> {
    init_logging();

    let (session_config, cache, credentials) = session_parts()?;
    let player_config = PlayerConfig::default();
    let audio_format = AudioFormat::default();
    let mixer_config = MixerConfig::default();
    let connect_config = ConnectConfig {
        name: options.device_name.clone(),
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

    let (spirc, spirc_task) =
        Spirc::new(connect_config, session.clone(), credentials, player, mixer)
            .await
            .map_err(|err| format!("could not start Spotify Connect: {err}"))?;

    spirc
        .activate()
        .map_err(|err| format!("could not activate Spotify Connect device: {err}"))?;

    if !options.tracks.is_empty() {
        let resolved = resolve_tracks(&session, &options.tracks).await?;
        println!("Resolved {} track(s). Starting playlist...", resolved.len());
        spirc
            .load(LoadRequest::from_tracks(
                resolved,
                LoadRequestOptions::default(),
            ))
            .map_err(|err| format!("could not load resolved playlist: {err}"))?;
        spirc
            .play()
            .map_err(|err| format!("could not start playlist playback: {err}"))?;
    } else if let Some(uri) = options.context_uri.as_ref() {
        spirc
            .load(LoadRequest::from_context_uri(
                uri.clone(),
                LoadRequestOptions::default(),
            ))
            .map_err(|err| format!("could not load Spotify URI: {err}"))?;
        spirc
            .play()
            .map_err(|err| format!("could not start playback: {err}"))?;
    }

    println!("Riff player is online as `{}`.", options.device_name);
    if options.tracks.is_empty() && options.context_uri.is_none() {
        println!("Select it from Spotify Connect and press play in Spotify.");
    }
    println!("Playback diagnostics are enabled. For full logs: RIFF_LOG=debug riff player");
    println!("Press Ctrl+C to stop.");

    tokio::select! {
        _ = spirc_task => {
            Err("Spotify Connect stopped unexpectedly. Re-run with RIFF_LOG=debug riff player for details.".to_string())
        },
        result = tokio::signal::ctrl_c() => {
            result.map_err(|err| format!("could not listen for Ctrl+C: {err}"))?;
            session.shutdown();
            Ok(())
        }
    }
}

pub async fn search(query: &str, limit: usize) -> Result<Vec<SearchCandidate>, String> {
    init_logging();
    let session = discovery_session().await?;
    let result = search_with_session(&session, query, limit).await;
    session.shutdown();
    result
}

pub async fn inspect_track(uri: &str) -> Result<SearchCandidate, String> {
    if !is_spotify_track_uri(uri) {
        return Err("track id must look like `spotify:track:<id>`".to_string());
    }

    init_logging();
    let session = discovery_session().await?;
    let context = session
        .spclient()
        .get_context(uri)
        .await
        .map_err(|err| format!("could not inspect `{uri}`: {err}"))?;

    let mut candidate = None;
    for track in context.pages.into_iter().flat_map(|page| page.tracks) {
        let Some(track_uri) = track.uri else {
            continue;
        };
        if track_uri == uri {
            candidate = Some(SearchCandidate {
                uri: track_uri,
                metadata: track.metadata.into_iter().collect(),
            });
            break;
        }
    }

    session.shutdown();
    Ok(candidate.unwrap_or_else(|| SearchCandidate {
        uri: uri.to_string(),
        metadata: BTreeMap::new(),
    }))
}

async fn resolve_tracks(session: &Session, tracks: &[TrackRequest]) -> Result<Vec<String>, String> {
    let mut resolved = Vec::with_capacity(tracks.len());

    for (index, track) in tracks.iter().enumerate() {
        if let Some(uri) = track.id.as_deref() {
            if !is_spotify_track_uri(uri) {
                return Err(format!(
                    "invalid pinned id for `{}`: expected `spotify:track:<id>`, got `{uri}`",
                    track.label
                ));
            }
            println!("  [{}/{}] pinned {}", index + 1, tracks.len(), track.label);
            println!("       -> {uri}");
            resolved.push(uri.to_string());
            continue;
        }

        println!("  [{}/{}] resolving {}", index + 1, tracks.len(), track.label);
        let candidates = search_with_session(session, &track.label, 1).await?;
        let candidate = candidates
            .into_iter()
            .next()
            .ok_or_else(|| format!("no Spotify track found for `{}`", track.label))?;
        println!("       -> {}", candidate.uri);
        resolved.push(candidate.uri);
    }

    Ok(resolved)
}

async fn discovery_session() -> Result<Session, String> {
    let (session_config, cache, credentials) = session_parts()?;
    let session = Session::new(session_config, Some(cache));
    session
        .connect(credentials, false)
        .await
        .map_err(|err| format!("could not connect to Spotify: {err}"))?;
    Ok(session)
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

async fn search_with_session(
    session: &Session,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchCandidate>, String> {
    let search_uri = spotify_search_uri(query);
    let context = session
        .spclient()
        .get_context(&search_uri)
        .await
        .map_err(|err| format!("Spotify internal search failed for `{query}`: {err}"))?;

    let mut candidates = Vec::new();
    for track in context.pages.into_iter().flat_map(|page| page.tracks) {
        let Some(uri) = track.uri else {
            continue;
        };
        if !is_spotify_track_uri(&uri) {
            continue;
        }
        candidates.push(SearchCandidate {
            uri,
            metadata: track.metadata.into_iter().collect(),
        });
        if candidates.len() == limit {
            break;
        }
    }
    Ok(candidates)
}

fn is_spotify_track_uri(uri: &str) -> bool {
    uri.starts_with("spotify:track:") && uri.len() > "spotify:track:".len()
}

fn spotify_search_uri(query: &str) -> String {
    let mut encoded = String::with_capacity(query.len());
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    for byte in query.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            _ => {
                encoded.push('%');
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 0x0f) as usize] as char);
            }
        }
    }

    format!("spotify:search:{encoded}")
}

fn init_logging() {
    let env = Env::default().filter_or("RIFF_LOG", "librespot=info");
    let _ = env_logger::Builder::from_env(env).try_init();
}

fn oauth_credentials(session_config: &SessionConfig) -> Result<Credentials, String> {
    println!("No cached Spotify login found. Opening Spotify authorization in your browser...");

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
    fn default_device_name_is_riff() {
        assert_eq!(PlayerOptions::default().device_name, "Riff");
    }

    #[test]
    fn builds_spotify_search_uri() {
        assert_eq!(
            spotify_search_uri("Black Sabbath - War Pigs"),
            "spotify:search:Black+Sabbath+-+War+Pigs"
        );
    }

    #[test]
    fn percent_encodes_non_ascii_search_text() {
        assert_eq!(
            spotify_search_uri("Motörhead"),
            "spotify:search:Mot%C3%B6rhead"
        );
    }

    #[test]
    fn validates_spotify_track_ids() {
        assert!(is_spotify_track_uri("spotify:track:abc123"));
        assert!(!is_spotify_track_uri("spotify:album:abc123"));
        assert!(!is_spotify_track_uri("spotify:track:"));
    }
}
