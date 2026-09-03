use std::{env, fs, path::PathBuf};

use env_logger::Env;
use librespot::{
    connect::{ConnectConfig, LoadRequest, LoadRequestOptions, Spirc},
    core::{authentication::Credentials, cache::Cache, config::SessionConfig, session::Session},
    oauth::{OAuthClient, OAuthClientBuilder},
    playback::{
        audio_backend,
        config::{AudioFormat, PlayerConfig},
        mixer,
        mixer::MixerConfig,
        player::Player,
    },
};
use serde::Deserialize;

const OAUTH_REDIRECT_URI: &str = "http://127.0.0.1:8898/login";
const OAUTH_SCOPES: &[&str] = &[
    "streaming",
    "user-read-playback-state",
    "user-modify-playback-state",
];
const OAUTH_REFRESH_TOKEN_FILE: &str = "oauth-refresh-token";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerOptions {
    pub context_uri: Option<String>,
    pub track_queries: Vec<String>,
    pub device_name: String,
}

impl Default for PlayerOptions {
    fn default() -> Self {
        Self {
            context_uri: None,
            track_queries: Vec::new(),
            device_name: "Riff".to_string(),
        }
    }
}

pub async fn run(options: PlayerOptions) -> Result<(), String> {
    init_logging();

    let cache_dir = cache_dir()?;
    let files_dir = cache_dir.join("files");
    fs::create_dir_all(&files_dir)
        .map_err(|err| format!("could not create Spotify cache directory: {err}"))?;
    secure_cache_dir(&cache_dir)?;

    let session_config = SessionConfig::default();
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

    let cache = Cache::new(
        Some(cache_dir.clone()),
        Some(cache_dir.clone()),
        Some(files_dir),
        None,
    )
    .map_err(|err| format!("could not initialize Spotify cache: {err}"))?;

    let (credentials, web_api_token) = if options.track_queries.is_empty() {
        let credentials = match cache.credentials() {
            Some(credentials) => credentials,
            None => oauth_credentials(&session_config)?,
        };
        (credentials, None)
    } else {
        let (credentials, access_token) =
            oauth_credentials_with_web_token(&session_config, &cache_dir)?;
        (credentials, Some(access_token))
    };

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

    if !options.track_queries.is_empty() {
        let access_token = web_api_token
            .as_deref()
            .ok_or_else(|| "Spotify Web API token was not initialized".to_string())?;
        let resolved = resolve_track_queries(access_token, &options.track_queries).await?;
        println!("Resolved {} track(s). Starting playlist...", resolved.len());
        spirc
            .load(LoadRequest::from_tracks(
                resolved.into_iter().map(|track| track.uri).collect(),
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
    if options.track_queries.is_empty() && options.context_uri.is_none() {
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

#[derive(Debug, Clone)]
struct ResolvedTrack {
    uri: String,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    tracks: SearchTracks,
}

#[derive(Debug, Deserialize)]
struct SearchTracks {
    items: Vec<SearchTrack>,
}

#[derive(Debug, Deserialize)]
struct SearchTrack {
    uri: String,
    name: String,
    artists: Vec<SearchArtist>,
}

#[derive(Debug, Deserialize)]
struct SearchArtist {
    name: String,
}

async fn resolve_track_queries(
    access_token: &str,
    queries: &[String],
) -> Result<Vec<ResolvedTrack>, String> {
    let client = reqwest::Client::new();
    let mut resolved = Vec::with_capacity(queries.len());

    for (index, query) in queries.iter().enumerate() {
        println!("  [{}/{}] resolving {query}", index + 1, queries.len());

        let response = client
            .get("https://api.spotify.com/v1/search")
            .bearer_auth(access_token)
            .query(&[("q", query.as_str()), ("type", "track"), ("limit", "5")])
            .send()
            .await
            .map_err(|err| format!("Spotify search failed for `{query}`: {err}"))?
            .error_for_status()
            .map_err(|err| format!("Spotify rejected search for `{query}`: {err}"))?
            .json::<SearchResponse>()
            .await
            .map_err(|err| format!("could not decode Spotify search for `{query}`: {err}"))?;

        let track = choose_track(query, response.tracks.items)
            .ok_or_else(|| format!("no Spotify track found for `{query}`"))?;
        let artists = track
            .artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        println!("       -> {artists} - {}", track.name);
        resolved.push(ResolvedTrack { uri: track.uri });
    }

    Ok(resolved)
}

fn choose_track(query: &str, tracks: Vec<SearchTrack>) -> Option<SearchTrack> {
    let (expected_artist, expected_title) = query
        .split_once(" - ")
        .map(|(artist, title)| (Some(artist.trim()), title.trim()))
        .unwrap_or((None, query.trim()));

    let exact_index = tracks.iter().position(|track| {
        track.name.eq_ignore_ascii_case(expected_title)
            && expected_artist.is_none_or(|artist| {
                track
                    .artists
                    .iter()
                    .any(|candidate| candidate.name.eq_ignore_ascii_case(artist))
            })
    });

    match exact_index {
        Some(index) => tracks.into_iter().nth(index),
        None => tracks.into_iter().next(),
    }
}

fn init_logging() {
    let env = Env::default().filter_or("RIFF_LOG", "librespot=info");
    let _ = env_logger::Builder::from_env(env).try_init();
}

fn oauth_client(session_config: &SessionConfig) -> Result<OAuthClient, String> {
    OAuthClientBuilder::new(
        &session_config.client_id,
        OAUTH_REDIRECT_URI,
        OAUTH_SCOPES.to_vec(),
    )
    .open_in_browser()
    .with_custom_message("Riff is connected. You can close this tab and return to the terminal.")
    .build()
    .map_err(|err| format!("could not initialize Spotify OAuth: {err}"))
}

fn oauth_credentials(session_config: &SessionConfig) -> Result<Credentials, String> {
    println!("No cached Spotify login found. Opening Spotify authorization in your browser...");

    oauth_client(session_config)?
        .get_access_token()
        .map(|token| Credentials::with_access_token(token.access_token))
        .map_err(|err| format!("Spotify authorization failed: {err}"))
}

fn oauth_credentials_with_web_token(
    session_config: &SessionConfig,
    cache_dir: &std::path::Path,
) -> Result<(Credentials, String), String> {
    let client = oauth_client(session_config)?;
    let refresh_path = cache_dir.join(OAUTH_REFRESH_TOKEN_FILE);
    let cached_refresh = fs::read_to_string(&refresh_path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let token = match cached_refresh.as_deref() {
        Some(refresh_token) => match client.refresh_token(refresh_token) {
            Ok(token) => token,
            Err(err) => {
                println!("Cached Spotify OAuth token could not be refreshed ({err}). Re-authorizing...");
                client
                    .get_access_token()
                    .map_err(|err| format!("Spotify authorization failed: {err}"))?
            }
        },
        None => {
            println!("Spotify Web API authorization is required for .riff track resolution.");
            client
                .get_access_token()
                .map_err(|err| format!("Spotify authorization failed: {err}"))?
        }
    };

    if !token.refresh_token.is_empty() {
        write_secret_file(&refresh_path, token.refresh_token.as_bytes())?;
    }

    let access_token = token.access_token;
    let credentials = Credentials::with_access_token(access_token.clone());
    Ok((credentials, access_token))
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

#[cfg(unix)]
fn write_secret_file(path: &std::path::Path, contents: &[u8]) -> Result<(), String> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true).mode(0o600);
    use std::io::Write;
    let mut file = options
        .open(path)
        .map_err(|err| format!("could not store Spotify OAuth refresh token: {err}"))?;
    file.write_all(contents)
        .map_err(|err| format!("could not store Spotify OAuth refresh token: {err}"))?;

    let mut permissions = file
        .metadata()
        .map_err(|err| format!("could not inspect OAuth token permissions: {err}"))?
        .permissions();
    permissions.set_mode(0o600);
    file.set_permissions(permissions)
        .map_err(|err| format!("could not secure OAuth token file: {err}"))
}

#[cfg(not(unix))]
fn write_secret_file(path: &std::path::Path, contents: &[u8]) -> Result<(), String> {
    fs::write(path, contents).map_err(|err| format!("could not store Spotify OAuth refresh token: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(uri: &str, name: &str, artist: &str) -> SearchTrack {
        SearchTrack {
            uri: uri.to_string(),
            name: name.to_string(),
            artists: vec![SearchArtist {
                name: artist.to_string(),
            }],
        }
    }

    #[test]
    fn default_device_name_is_riff() {
        assert_eq!(PlayerOptions::default().device_name, "Riff");
    }

    #[test]
    fn chooses_exact_artist_and_title_when_available() {
        let tracks = vec![
            track("spotify:track:wrong", "War Pigs", "Cover Band"),
            track("spotify:track:right", "War Pigs", "Black Sabbath"),
        ];

        let selected = choose_track("Black Sabbath - War Pigs", tracks).expect("track expected");
        assert_eq!(selected.uri, "spotify:track:right");
    }

    #[test]
    fn falls_back_to_first_search_result() {
        let tracks = vec![track("spotify:track:first", "Other Name", "Other Artist")];
        let selected = choose_track("Unknown - Query", tracks).expect("track expected");
        assert_eq!(selected.uri, "spotify:track:first");
    }
}
