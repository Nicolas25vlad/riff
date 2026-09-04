use std::{
    env, fs,
    path::{Path, PathBuf},
};

use librespot::playback::audio_backend::{self, SinkBuilder};

pub fn audio_sink_builder() -> Result<SinkBuilder, String> {
    if let Some(name) = env::var_os("RIFF_AUDIO_BACKEND") {
        let name = name.to_string_lossy().into_owned();
        return audio_backend::find(Some(name.clone())).ok_or_else(|| {
            format!("unsupported audio backend `{name}`; unset RIFF_AUDIO_BACKEND to use automatic selection")
        });
    }

    if is_wsl() && env::var_os("PULSE_SERVER").is_some() {
        return audio_backend::find(Some("pulseaudio".to_string())).ok_or_else(|| {
            "WSL/WSLg was detected, but the PulseAudio backend is not available in this build"
                .to_string()
        });
    }

    audio_backend::find(None).ok_or_else(|| "no supported audio backend was found".to_string())
}

pub fn riff_cache_dir() -> Result<PathBuf, String> {
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

pub fn is_wsl() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }

    if env::var_os("WSL_INTEROP").is_some() || env::var_os("WSL_DISTRO_NAME").is_some() {
        return true;
    }

    fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|release| release.to_ascii_lowercase().contains("microsoft"))
        .unwrap_or(false)
}

#[cfg(unix)]
pub fn secure_cache_dir(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .map_err(|err| format!("could not inspect Spotify cache permissions: {err}"))?
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
        .map_err(|err| format!("could not secure Spotify cache directory: {err}"))
}

#[cfg(not(unix))]
pub fn secure_cache_dir(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_linux_targets_are_not_wsl() {
        if !cfg!(target_os = "linux") {
            assert!(!is_wsl());
        }
    }
}
