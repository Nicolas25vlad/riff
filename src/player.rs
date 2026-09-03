use std::{env, fs, path::PathBuf};

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
pub struct PlayerOptions {
    pub context_uri: Option<String>,
    pub device_name: String,
}

impl Default for PlayerOptions {
    fn default() -> Self {
        Self {
            context_uri: None,
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

    let credentials = match cache.credentials() {
        Some(credentials) => credentials,
        None => oauth_credentials(&session_config)?,
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

    if let Some(uri) = options.context_uri {
        spirc
            .load(LoadRequest::from_context_uri(
                uri,
                LoadRequestOptions::default(),
            ))
            .map_err(|err| format!("could not load Spotify URI: {err}"))?;
        spirc
            .play()
            .map_err(|err| format!("could not start playback: {err}"))?;
    }

    println!("Riff player is online as `{}`.", options.device_name);
    println!("Select it from Spotify Connect and press play in Spotify.");
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
}
