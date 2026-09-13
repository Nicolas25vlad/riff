from pathlib import Path

platform = Path("src/platform.rs")
text = platform.read_text()
old = '''pub fn spotify_cache_dir() -> Result<PathBuf, String> {
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
'''
new = '''pub fn riff_cache_dir() -> Result<PathBuf, String> {
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

pub fn tui_log_path() -> Result<PathBuf, String> {
    Ok(riff_cache_dir()?.join("tui.log"))
}
'''
assert old in text, "spotify cache dir block not found"
platform.write_text(text.replace(old, new, 1))

player = Path("src/player.rs")
text = player.read_text()
text = text.replace("use env_logger::Env;", "use env_logger::{Env, Target};")
anchor = '''pub fn init_cli_logging() {
    let env = Env::default().filter_or("RIFF_LOG", "riff=info,librespot=info");
    let _ = env_logger::Builder::from_env(env).try_init();
}
'''
replacement = '''pub fn init_cli_logging() {
    let env = Env::default().filter_or("RIFF_LOG", "riff=info,librespot=info");
    let _ = env_logger::Builder::from_env(env).try_init();
}

pub fn init_tui_logging() -> Result<std::path::PathBuf, String> {
    let log_path = platform::tui_log_path()?;
    let parent = log_path
        .parent()
        .ok_or_else(|| "could not determine TUI log directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|err| format!("could not create TUI log directory: {err}"))?;
    platform::secure_cache_dir(parent)?;

    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|err| format!("could not open TUI log file `{}`: {err}", log_path.display()))?;
    let env = Env::default().filter_or("RIFF_LOG", "riff=warn,librespot=warn");
    env_logger::Builder::from_env(env)
        .target(Target::Pipe(Box::new(file)))
        .format_timestamp_secs()
        .try_init()
        .map_err(|err| format!("could not initialize TUI file logging: {err}"))?;
    log::debug!("TUI logging initialized at {}", log_path.display());
    Ok(log_path)
}
'''
assert anchor in text, "CLI logger anchor not found"
player.write_text(text.replace(anchor, replacement, 1))

main = Path("src/main.rs")
text = main.read_text()
anchor = '''    if args.first().is_some_and(|arg| arg == "tui") {
        let Some(path) = args.get(1) else {
'''
replacement = '''    if args.first().is_some_and(|arg| arg == "tui") {
        if let Err(err) = riff::player::init_tui_logging() {
            eprintln!("riff: could not initialize TUI logging: {err}");
            process::exit(1);
        }

        let Some(path) = args.get(1) else {
'''
assert anchor in text, "TUI command anchor not found"
main.write_text(text.replace(anchor, replacement, 1))
