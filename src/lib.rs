use std::{fmt, fs, path::PathBuf};

pub mod player;

const DEFAULT_PLAYLIST: &str = r#"playlist \"my-playlist\" {
    track \"Black Sabbath - War Pigs\"
    track \"Dio - Holy Diver\"
}
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    Version,
    Doctor,
    Init(PathBuf),
    Validate(PathBuf),
    Inspect(PathBuf),
    Player(Option<String>),
}

impl Command {
    pub fn parse(args: &[String]) -> Result<Self, RiffError> {
        match args {
            [] => Ok(Self::Help),
            [flag] if flag == "-h" || flag == "--help" || flag == "help" => Ok(Self::Help),
            [flag] if flag == "-V" || flag == "--version" || flag == "version" => Ok(Self::Version),
            [cmd] if cmd == "doctor" => Ok(Self::Doctor),
            [cmd] if cmd == "init" => Ok(Self::Init("playlist.riff".into())),
            [cmd, path] if cmd == "init" => Ok(Self::Init(path.into())),
            [cmd, path] if cmd == "validate" => Ok(Self::Validate(path.into())),
            [cmd, path] if cmd == "inspect" => Ok(Self::Inspect(path.into())),
            [cmd] if cmd == "player" => Ok(Self::Player(None)),
            [cmd, uri] if cmd == "player" => Ok(Self::Player(Some(uri.clone()))),
            _ => Err(RiffError::Usage(
                "unknown command or invalid arguments".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    pub name: String,
    pub tracks: Vec<String>,
}

impl Playlist {
    pub fn parse(source: &str) -> Result<Self, RiffError> {
        let mut lines = source
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'));

        let header = lines
            .next()
            .ok_or_else(|| RiffError::Parse("missing playlist declaration".into()))?;

        let name = parse_playlist_header(header)?;
        let mut tracks = Vec::new();
        let mut closed = false;

        for line in lines.by_ref() {
            if line == "}" {
                closed = true;
                break;
            }

            let value = line
                .strip_prefix("track ")
                .ok_or_else(|| RiffError::Parse(format!("unsupported statement: `{line}`")))?;

            let track = parse_quoted(value, "track")?;
            if track.is_empty() {
                return Err(RiffError::Parse("track cannot be empty".into()));
            }

            tracks.push(track.to_string());
        }

        if !closed {
            return Err(RiffError::Parse("playlist block is not closed".into()));
        }

        if lines.next().is_some() {
            return Err(RiffError::Parse(
                "unexpected content after playlist block".into(),
            ));
        }

        Ok(Self {
            name: name.to_string(),
            tracks,
        })
    }
}

fn parse_playlist_header(header: &str) -> Result<&str, RiffError> {
    let value = header
        .strip_prefix("playlist ")
        .and_then(|value| value.strip_suffix('{'))
        .map(str::trim)
        .ok_or_else(|| RiffError::Parse("expected `playlist \"name\" {`".into()))?;

    let name = parse_quoted(value, "playlist name")?;
    if name.is_empty() {
        return Err(RiffError::Parse("playlist name cannot be empty".into()));
    }

    Ok(name)
}

fn parse_quoted<'a>(value: &'a str, label: &str) -> Result<&'a str, RiffError> {
    let value = value.trim();
    if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
        return Err(RiffError::Parse(format!("{label} must be quoted")));
    }

    Ok(&value[1..value.len() - 1])
}

#[derive(Debug)]
pub enum RiffError {
    Io(std::io::Error),
    Parse(String),
    Usage(String),
    AlreadyExists(PathBuf),
    Player(String),
}

impl fmt::Display for RiffError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Parse(msg) => write!(f, "parse error: {msg}"),
            Self::Usage(msg) => write!(f, "{msg}\n\n{}", help()),
            Self::AlreadyExists(path) => write!(
                f,
                "{} already exists; choose another path or remove it first",
                path.display()
            ),
            Self::Player(msg) => write!(f, "player error: {msg}"),
        }
    }
}

impl From<std::io::Error> for RiffError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub async fn run(command: Command) -> Result<String, RiffError> {
    match command {
        Command::Help => Ok(help()),
        Command::Version => Ok(format!("riff {}", env!("CARGO_PKG_VERSION"))),
        Command::Doctor => Ok(doctor()),
        Command::Init(path) => init(path),
        Command::Validate(path) => {
            let source = fs::read_to_string(&path)?;
            let playlist = Playlist::parse(&source)?;
            Ok(format!(
                "✓ {} is valid ({} tracks)",
                playlist.name,
                playlist.tracks.len()
            ))
        }
        Command::Inspect(path) => {
            let source = fs::read_to_string(&path)?;
            let playlist = Playlist::parse(&source)?;
            let body = playlist
                .tracks
                .iter()
                .enumerate()
                .map(|(index, track)| format!("{:>3}. {track}", index + 1))
                .collect::<Vec<_>>()
                .join("\n");

            if body.is_empty() {
                Ok(format!("{}\n0 tracks", playlist.name))
            } else {
                Ok(format!(
                    "{}\n{} tracks\n\n{}",
                    playlist.name,
                    playlist.tracks.len(),
                    body
                ))
            }
        }
        Command::Player(context_uri) => {
            let options = player::PlayerOptions {
                context_uri,
                ..player::PlayerOptions::default()
            };
            player::run(options).await.map_err(RiffError::Player)?;
            Ok(String::new())
        }
    }
}

fn init(path: PathBuf) -> Result<String, RiffError> {
    if path.exists() {
        return Err(RiffError::AlreadyExists(path));
    }

    fs::write(&path, DEFAULT_PLAYLIST)?;
    Ok(format!(
        "✓ created {}\n\nNext:\n  riff validate {}\n  riff inspect {}",
        path.display(),
        path.display(),
        path.display()
    ))
}

pub fn help() -> String {
    format!(
        "riff {}\nMusic as code, from the terminal.\n\nUSAGE:\n  riff <COMMAND>\n\nCOMMANDS:\n  init [file]       Create a starter playlist (default: playlist.riff)\n  doctor            Check the local Riff environment\n  validate <file>   Validate a .riff playlist\n  inspect <file>    Parse and print a .riff playlist\n  player [uri]      Start Riff as a local Spotify Connect player\n  help              Print this help\n  version           Print version",
        env!("CARGO_PKG_VERSION")
    )
}

fn doctor() -> String {
    format!(
        "Riff doctor\n  version      {}\n  platform     {}-{}\n  playlist DSL ready\n  spotify      player core available (Premium required)\n  playback     librespot / local audio backend",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_playlist() {
        let source = r#"
            playlist "coding-metal" {
                track "Black Sabbath - War Pigs"
                track "Dio - Holy Diver"
            }
        "#;

        let playlist = Playlist::parse(source).expect("playlist should parse");
        assert_eq!(playlist.name, "coding-metal");
        assert_eq!(playlist.tracks.len(), 2);
    }

    #[test]
    fn rejects_unknown_statement() {
        let source = "playlist \"x\" {\nshuffle true\n}";
        assert!(Playlist::parse(source).is_err());
    }

    #[test]
    fn rejects_unquoted_playlist_name() {
        let source = "playlist coding {\n}";
        assert!(Playlist::parse(source).is_err());
    }

    #[test]
    fn rejects_unquoted_track() {
        let source = "playlist \"coding\" {\ntrack Black Sabbath - War Pigs\n}";
        assert!(Playlist::parse(source).is_err());
    }

    #[test]
    fn rejects_content_after_playlist() {
        let source = "playlist \"coding\" {\n}\ntrack \"orphan\"";
        assert!(Playlist::parse(source).is_err());
    }

    #[test]
    fn init_defaults_to_playlist_riff() {
        let args = vec!["init".to_string()];
        assert_eq!(
            Command::parse(&args).expect("command should parse"),
            Command::Init(PathBuf::from("playlist.riff"))
        );
    }

    #[test]
    fn parses_player_with_context_uri() {
        let args = vec![
            "player".to_string(),
            "spotify:album:1ATL5GLyefJaxhQzSPVrLX".to_string(),
        ];
        assert_eq!(
            Command::parse(&args).expect("command should parse"),
            Command::Player(Some("spotify:album:1ATL5GLyefJaxhQzSPVrLX".to_string()))
        );
    }
}
